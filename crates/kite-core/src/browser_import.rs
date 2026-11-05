//! Bookmarks of other browsers on this machine, read from their profiles: the
//! Firefox family, the Chromium family and Falkon, also as Flatpak and Snap.

use crate::bookmarks::{Bookmarks, Kind, Node};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Firefox,
    Chromium,
    Falkon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub browser: String,
    pub profile: String,
    pub family: Family,
    /// The file holding the bookmarks.
    pub path: PathBuf,
}

impl Source {
    pub fn label(&self) -> String {
        if self.profile.is_empty() { self.browser.clone() } else { format!("{} ({})", self.browser, self.profile) }
    }
}

// (browser, folder under the home directory, Flatpak app)
const FIREFOXES: &[(&str, &str, &str)] = &[
    ("Firefox", ".mozilla/firefox", "org.mozilla.firefox"),
    ("LibreWolf", ".librewolf", "io.gitlab.librewolf-community"),
    ("Waterfox", ".waterfox", "net.waterfox.waterfox"),
];
// (browser, folder under the config directory, Flatpak app)
const CHROMIUMS: &[(&str, &str, &str)] = &[
    ("Chromium", "chromium", "org.chromium.Chromium"),
    ("Google Chrome", "google-chrome", "com.google.Chrome"),
    ("Brave", "BraveSoftware/Brave-Browser", "com.brave.Browser"),
    ("Microsoft Edge", "microsoft-edge", "com.microsoft.Edge"),
    ("Vivaldi", "vivaldi", "com.vivaldi.Vivaldi"),
    ("Opera", "opera", "com.opera.Opera"),
];

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    out.sort();
    out
}

fn firefox_profiles(browser: &str, dir: &Path, out: &mut Vec<Source>) {
    for p in subfolders(dir) {
        let path = p.join("places.sqlite");
        if path.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let profile = name.split_once('.').map_or(name.as_str(), |(_, n)| n).to_owned();
            out.push(Source { browser: browser.into(), profile, family: Family::Firefox, path });
        }
    }
}

fn chromium_profiles(browser: &str, dir: &Path, out: &mut Vec<Source>) {
    let names: Value = std::fs::read_to_string(dir.join("Local State")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    for p in subfolders(dir) {
        let path = p.join("Bookmarks");
        let folder = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if path.is_file() && !matches!(folder.as_str(), "Guest Profile" | "System Profile") {
            let profile = names["profile"]["info_cache"][&folder]["name"].as_str().unwrap_or(&folder).to_owned();
            out.push(Source { browser: browser.into(), profile, family: Family::Chromium, path });
        }
    }
}

fn falkon_profiles(browser: &str, dir: &Path, out: &mut Vec<Source>) {
    for p in subfolders(&dir.join("profiles")) {
        let path = p.join("bookmarks.json");
        if path.is_file() {
            let profile = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            out.push(Source { browser: browser.into(), profile, family: Family::Falkon, path });
        }
    }
}

/// The profiles found, for a home directory and its config directory.
pub fn detect(home: &Path, config: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    let flatpak = |app: &str| home.join(".var/app").join(app);
    for (browser, dir, app) in FIREFOXES {
        firefox_profiles(browser, &home.join(dir), &mut out);
        firefox_profiles(&format!("{browser} (Flatpak)"), &flatpak(app).join(dir), &mut out);
    }
    firefox_profiles("Firefox (Snap)", &home.join("snap/firefox/common/.mozilla/firefox"), &mut out);
    for (browser, dir, app) in CHROMIUMS {
        chromium_profiles(browser, &config.join(dir), &mut out);
        chromium_profiles(&format!("{browser} (Flatpak)"), &flatpak(app).join("config").join(dir), &mut out);
    }
    chromium_profiles("Chromium (Snap)", &home.join("snap/chromium/common/chromium"), &mut out);
    falkon_profiles("Falkon", &config.join("falkon"), &mut out);
    falkon_profiles("Falkon (Flatpak)", &flatpak("org.kde.falkon").join("config/falkon"), &mut out);
    out
}

/// A profile's bookmarks: its top folders, with what is in them.
pub fn read(source: &Source) -> anyhow::Result<Vec<Node>> {
    match source.family {
        Family::Firefox => read_firefox(&source.path),
        Family::Chromium => read_chromium(&source.path),
        Family::Falkon => Ok(Bookmarks::load(source.path.clone()).root.children),
    }
}

fn read_chromium(path: &Path) -> anyhow::Result<Vec<Node>> {
    fn node(v: &Value) -> Option<Node> {
        let name = v["name"].as_str().unwrap_or("");
        match v["type"].as_str()? {
            "url" => Some(Node::url(name, v["url"].as_str()?)),
            "folder" => {
                let mut f = Node::folder(name);
                f.children = v["children"].as_array()?.iter().filter_map(node).collect();
                Some(f)
            }
            _ => None,
        }
    }
    let file: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let roots = file["roots"].as_object().ok_or_else(|| anyhow::anyhow!("no bookmarks in {}", path.display()))?;
    let mut out = Vec::new();
    for (key, title) in [("bookmark_bar", "Bookmarks Toolbar"), ("other", "Other Bookmarks"), ("synced", "Mobile Bookmarks")] {
        if let Some(mut f) = roots.get(key).and_then(node) {
            f.name = title.into();
            out.push(f);
        }
    }
    Ok(out)
}

/// A folder of its own for the copy, removed afterwards: it holds what was visited.
struct PrivateCopy(PathBuf);
impl PrivateCopy {
    fn new() -> anyhow::Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        let base = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = base.join(format!("kite-import-{}-{n}", std::process::id()));
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        Ok(Self(dir))
    }
}
impl Drop for PrivateCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// From a copy of the database, as Firefox keeps it locked while running; with its
/// write-ahead log, which holds the latest changes.
fn read_firefox(path: &Path) -> anyhow::Result<Vec<Node>> {
    let copy = PrivateCopy::new()?;
    let db_path = copy.0.join("places.sqlite");
    std::fs::copy(path, &db_path)?;
    let wal = path.with_file_name("places.sqlite-wal");
    if wal.is_file() {
        std::fs::copy(wal, copy.0.join("places.sqlite-wal"))?;
    }
    let db = rusqlite::Connection::open(&db_path)?;
    struct Row { id: i64, kind: i64, title: String, url: String, guid: String }
    let mut stmt = db.prepare(
        "SELECT b.id, b.parent, b.type, COALESCE(b.title, ''), COALESCE(p.url, ''), COALESCE(b.guid, '')
         FROM moz_bookmarks b LEFT JOIN moz_places p ON b.fk = p.id ORDER BY b.parent, b.position",
    )?;
    let mut by_parent: HashMap<i64, Vec<Row>> = HashMap::new();
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(1)?, Row { id: r.get(0)?, kind: r.get(2)?, title: r.get(3)?, url: r.get(4)?, guid: r.get(5)? })))?;
    for row in rows {
        let (parent, row) = row?;
        by_parent.entry(parent).or_default().push(row);
    }
    fn children(of: i64, by_parent: &HashMap<i64, Vec<Row>>) -> Vec<Node> {
        let mut out = Vec::new();
        for r in by_parent.get(&of).map_or(&[][..], |v| v) {
            match r.kind {
                1 => out.push(Node::url(&r.title, &r.url)),
                2 => {
                    let mut f = Node::folder(&r.title);
                    f.children = children(r.id, by_parent);
                    out.push(f);
                }
                3 => out.push(Node::separator()),
                _ => {}
            }
        }
        out
    }
    let root = db.query_row("SELECT id FROM moz_bookmarks WHERE guid = 'root________'", [], |r| r.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for (guid, title) in [("toolbar_____", "Bookmarks Toolbar"), ("menu________", "Bookmarks Menu"), ("unfiled_____", "Other Bookmarks"), ("mobile______", "Mobile Bookmarks")] {
        if let Some(r) = by_parent.get(&root).and_then(|v| v.iter().find(|r| r.guid == guid)) {
            let mut f = Node::folder(title);
            f.children = children(r.id, &by_parent);
            out.push(f);
        }
    }
    Ok(out)
}

/// Addresses that mean something in another browser too. (Not Firefox's `place:` smart
/// folders, `chrome:` pages or `javascript:`.)
fn usable(url: &str) -> bool {
    let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "ftp" | "file")
}

pub fn urls_in(nodes: &[Node], out: &mut HashSet<String>) {
    for n in nodes {
        if n.kind == Kind::Url {
            out.insert(n.url.clone());
        }
        urls_in(&n.children, out);
    }
}

/// What's worth bringing over: without what's known already, unusable addresses,
/// and folders left with nothing in them. Answers it with how many are new, and how
/// many were there already.
pub fn select(nodes: Vec<Node>, known: &HashSet<String>) -> (Vec<Node>, usize, usize) {
    let (mut new, mut already) = (0, 0);
    let mut out = Vec::new();
    for mut n in nodes {
        match n.kind {
            Kind::Url => {
                if known.contains(&n.url) {
                    already += 1;
                } else if usable(&n.url) {
                    new += 1;
                    if n.name.is_empty() {
                        n.name = n.url.clone();
                    }
                    out.push(n);
                }
            }
            Kind::Separator => out.push(n),
            _ => {
                let (children, a, b) = select(std::mem::take(&mut n.children), known);
                new += a;
                already += b;
                if children.iter().any(|c| c.kind != Kind::Separator) {
                    n.children = children;
                    out.push(n);
                }
            }
        }
    }
    (out, new, already)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn places(path: &Path) -> rusqlite::Connection {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, position INTEGER, title TEXT, guid TEXT);
             INSERT INTO moz_places VALUES (1, 'https://a.example/'), (2, 'https://b.example/'), (3, 'place:sort=8'), (4, 'javascript:void(0)');
             INSERT INTO moz_bookmarks VALUES
               (1, 2, NULL, 0, 0, '', 'root________'),
               (2, 2, NULL, 1, 0, 'menu', 'menu________'),
               (3, 2, NULL, 1, 1, 'toolbar', 'toolbar_____'),
               (4, 2, NULL, 1, 2, 'tags', 'tags________'),
               (5, 2, NULL, 1, 3, 'unfiled', 'unfiled_____'),
               (6, 2, NULL, 1, 4, 'mobile', 'mobile______'),
               (10, 1, 1, 3, 1, 'A', 'x1'),
               (11, 2, NULL, 3, 0, 'Sub', 'x2'),
               (12, 1, 2, 11, 0, 'B', 'x3'),
               (13, 3, NULL, 11, 1, NULL, 'x4'),
               (14, 1, 3, 2, 0, 'Smart', 'x5'),
               (15, 1, 4, 2, 1, 'Script', 'x6'),
               (16, 2, NULL, 4, 0, 'atag', 'x7'),
               (17, 1, 1, 16, 0, NULL, 'x8');",
        )
        .unwrap();
        db
    }

    #[test]
    fn finds_the_profiles_of_all_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let (home, config) = (dir.path().join("h"), dir.path().join("h/.config"));
        places(&home.join(".mozilla/firefox/abc.default-release/places.sqlite"));
        std::fs::create_dir_all(home.join(".mozilla/firefox/Crash Reports")).unwrap();
        places(&home.join(".var/app/org.mozilla.firefox/.mozilla/firefox/x.work/places.sqlite"));
        write(&config.join("chromium/Default/Bookmarks"), "{}");
        write(&config.join("chromium/Profile 1/Bookmarks"), "{}");
        write(&config.join("chromium/System Profile/Bookmarks"), "{}");
        write(&config.join("chromium/Local State"), &json!({"profile": {"info_cache": {"Profile 1": {"name": "Work"}}}}).to_string());
        write(&config.join("BraveSoftware/Brave-Browser/Default/Bookmarks"), "{}");
        write(&config.join("falkon/profiles/default/bookmarks.json"), "{}");
        let found: Vec<String> = detect(&home, &config).iter().map(Source::label).collect();
        assert_eq!(found, [
            "Firefox (default-release)", "Firefox (Flatpak) (work)", "Chromium (Default)", "Chromium (Work)", "Brave (Default)", "Falkon (default)",
        ]);
    }

    #[test]
    fn nothing_there_finds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect(dir.path(), &dir.path().join("c")).is_empty());
    }

    #[test]
    fn reads_firefox() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.sqlite");
        drop(places(&path));
        let nodes = read(&Source { browser: "F".into(), profile: String::new(), family: Family::Firefox, path }).unwrap();
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Bookmarks Toolbar", "Bookmarks Menu", "Other Bookmarks", "Mobile Bookmarks"]);
        // Sub comes first by position, with its bookmark and separator.
        assert_eq!(nodes[0].children[0].name, "Sub");
        assert_eq!(nodes[0].children[0].children[0].url, "https://b.example/");
        assert_eq!(nodes[0].children[0].children[1].kind, Kind::Separator);
        assert_eq!(nodes[0].children[1].name, "A");
        // After selecting: no tags, smart folders or scripts, no empty roots.
        let (kept, new, already) = select(nodes, &HashSet::new());
        assert_eq!((new, already), (2, 0));
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn reads_firefox_while_it_holds_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.sqlite");
        let open = places(&path);
        open.execute_batch("PRAGMA journal_mode=WAL; BEGIN IMMEDIATE; INSERT INTO moz_places VALUES (9, 'https://wal.example/'); COMMIT; BEGIN EXCLUSIVE;").unwrap();
        let nodes = read(&Source { browser: "F".into(), profile: String::new(), family: Family::Firefox, path }).unwrap();
        assert_eq!(nodes.len(), 4);
    }

    #[test]
    fn leaves_no_copy_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.sqlite");
        drop(places(&path));
        let copy = PrivateCopy::new().unwrap();
        let there = copy.0.clone();
        assert!(there.is_dir());
        drop(copy);
        assert!(!there.exists());
        assert!(read(&Source { browser: "F".into(), profile: String::new(), family: Family::Firefox, path }).is_ok());
    }

    #[test]
    fn reads_chromium() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Bookmarks");
        write(&path, &json!({"roots": {
            "bookmark_bar": {"type": "folder", "name": "Lesezeichenleiste", "children": [
                {"type": "url", "name": "A", "url": "https://a.example/"},
                {"type": "folder", "name": "Sub", "children": [{"type": "url", "name": "B", "url": "https://b.example/"}]}
            ]},
            "other": {"type": "folder", "name": "x", "children": [{"type": "url", "name": "S", "url": "chrome://settings"}]},
            "synced": {"type": "folder", "name": "y", "children": []}
        }}).to_string());
        let nodes = read(&Source { browser: "C".into(), profile: String::new(), family: Family::Chromium, path }).unwrap();
        assert_eq!(nodes[0].name, "Bookmarks Toolbar");
        assert_eq!(nodes[0].children[1].children[0].url, "https://b.example/");
        let (kept, new, _) = select(nodes, &HashSet::new());
        assert_eq!(new, 2);
        assert_eq!(kept.len(), 1, "the rest has nothing usable");
    }

    #[test]
    fn damaged_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        write(&path, "not this");
        assert!(read(&Source { browser: "C".into(), profile: String::new(), family: Family::Chromium, path: path.clone() }).is_err());
        assert!(read(&Source { browser: "F".into(), profile: String::new(), family: Family::Firefox, path }).is_err());
    }

    #[test]
    fn known_addresses_are_skipped_so_importing_twice_is_harmless() {
        let mut folder = Node::folder("F");
        folder.children = vec![Node::url("A", "https://a.example/"), Node::url("", "https://b.example/")];
        let known: HashSet<String> = ["https://a.example/".to_string()].into();
        let (kept, new, already) = select(vec![folder.clone()], &known);
        assert_eq!((new, already), (1, 1));
        assert_eq!(kept[0].children.len(), 1);
        assert_eq!(kept[0].children[0].name, "https://b.example/", "a name to show");
        let all: HashSet<String> = ["https://a.example/".to_string(), "https://b.example/".to_string()].into();
        let (kept, new, already) = select(vec![folder], &all);
        assert_eq!((kept.len(), new, already), (0, 0, 2));
    }
}
