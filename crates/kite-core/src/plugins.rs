//! Optional features ("plugins"), off or on per user, with nothing of them running
//! while off. They use Kite's internals, so nothing here stays compatible between
//! versions.
//!
//! Each one is a folder described by its plugin.json: those shipped in the binary
//! are in crates/kite/plugins/<id>/, and those the user installed from a file in
//! <data dir>/plugins/<id>/. An installed one is untrusted until the user turns it
//! on, and then runs with all of Kite's rights: everything in its manifest is
//! checked here, it starts off whatever it says, and showing it must not load or
//! run anything of it.
//!
//! Its state is stored as the settings "plugins.<id>.enabled" and
//! "plugins.<id>.<key>"; those not set by the user come from here.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub default_enabled: bool,
    /// Installed from a file, not shipped with Kite. (Never from its manifest.)
    #[serde(skip_deserializing)]
    pub installed: bool,
    /// What the paths of its files are relative to, as an address.
    #[serde(skip_deserializing)]
    pub base_url: String,
    /// QML item made once per browser window while the plugin is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    /// QML item shown in Settings → Plugins while the plugin is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_page: Option<String>,
    /// Its entries among the shortcut actions. Its window item's runAction(id)
    /// does the work, so it needs one.
    #[serde(default)]
    pub actions: Vec<PluginAction>,
    /// Scripts for web pages, while the plugin is on.
    #[serde(default)]
    pub page_scripts: Vec<PageScript>,
    /// The plugin's settings, with their defaults. Page scripts see them, as they
    /// are when the page loads, as KITE_PLUGIN_SETTINGS.
    #[serde(default)]
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginAction {
    /// "<plugin id>.<name>".
    pub id: String,
    pub group: String,
    pub name: String,
    #[serde(default)]
    pub keys: Vec<String>,
    pub icon: String,
}

/// One script for pages, of one or more files put together in order. The other
/// fields are those of QWebEngineScript, as in Main.qml's scripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageScript {
    pub files: Vec<String>,
    pub world_id: u32,
    pub runs_on_sub_frames: bool,
    pub injection_point: u32,
}

impl Plugin {
    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let p: Self = serde_json::from_str(json)?;
        let good_id = !p.id.is_empty() && p.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        anyhow::ensure!(good_id, "plugin id {:?}: only a-z, 0-9 and -", p.id);
        // What's shown of it is only text: Qt's labels take anything looking like
        // markup for it, and an image in that would be fetched just by listing it.
        let plain = |what: &str, text: &str, max: usize| -> anyhow::Result<()> {
            let spoofs = |c: char| c.is_control() || ('\u{202a}'..='\u{202e}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c);
            anyhow::ensure!(!text.contains('<') && !text.chars().any(spoofs), "plugin {}: its {what} can't contain markup or control characters", p.id);
            anyhow::ensure!(text.chars().count() <= max, "plugin {}: its {what} is too long", p.id);
            Ok(())
        };
        plain("name", &p.name, 64)?;
        anyhow::ensure!(!p.name.trim().is_empty(), "plugin {}: its name is empty", p.id);
        plain("description", &p.description, 600)?;
        plain("version", p.version.as_deref().unwrap_or(""), 32)?;
        for action in &p.actions {
            plain("action's name", &action.name, 80)?;
            plain("action's group", &action.group, 40)?;
        }
        // A name from the icon theme. (An address would be fetched just by listing it.)
        let good_icon = !p.icon.is_empty() && p.icon.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
        anyhow::ensure!(good_icon, "plugin {}: its icon has to be an icon theme's name", p.id);
        anyhow::ensure!(!p.settings.contains_key("enabled"), "plugin {}: \"enabled\" is not a setting, see defaultEnabled", p.id);
        let folder = format!("plugins/{}/", p.id);
        for file in p.files() {
            anyhow::ensure!(file.starts_with(&folder) && !file.contains(".."), "plugin {}: {file} is outside its folder", p.id);
        }
        for action in &p.actions {
            anyhow::ensure!(action.id.starts_with(&format!("{}.", p.id)), "plugin {}: action {} must start with its id", p.id, action.id);
        }
        anyhow::ensure!(p.actions.is_empty() || p.window.is_some(), "plugin {}: actions need a window item to run them", p.id);
        for script in &p.page_scripts {
            anyhow::ensure!(!script.files.is_empty(), "plugin {}: a page script without files", p.id);
        }
        Ok(p)
    }

    /// The files it names, relative to the kite crate.
    pub fn files(&self) -> impl Iterator<Item = &String> {
        self.window.iter().chain(self.settings_page.iter()).chain(self.page_scripts.iter().flat_map(|s| s.files.iter()))
    }
}

/// All the plugins, from their plugin.json texts; each must be valid, and be
/// the only one with its id.
pub fn parse_all(manifests: &[&str]) -> anyhow::Result<Vec<Plugin>> {
    let mut out: Vec<Plugin> = Vec::new();
    for text in manifests {
        let p = Plugin::parse(text)?;
        anyhow::ensure!(out.iter().all(|o| o.id != p.id), "two plugins are called {}", p.id);
        out.push(p);
    }
    Ok(out)
}

// ---- plugins installed from a file ----

/// Bigger than any plugin should be.
pub const MAX_PACKAGE: u64 = 64 * 1024 * 1024;

fn package_names(zip: &zip::ZipArchive<Cursor<&[u8]>>) -> anyhow::Result<Vec<String>> {
    Ok(zip.file_names().map(|n| n.map(|n| n.into_owned())).collect::<Result<_, _>>()?)
}

/// Where its plugin.json is in the archive: at the top, or in the one folder that
/// holds everything (what zipping a folder gives).
fn package_root(zip: &zip::ZipArchive<Cursor<&[u8]>>) -> anyhow::Result<String> {
    let names = package_names(zip)?;
    if names.iter().any(|n| n == "plugin.json") {
        return Ok(String::new());
    }
    let mut manifests = names.iter().filter(|n| n.matches('/').count() == 1 && n.ends_with("/plugin.json"));
    let (Some(one), None) = (manifests.next(), manifests.next()) else { anyhow::bail!("the file has no plugin.json") };
    let root = one.strip_suffix("plugin.json").unwrap_or("").to_owned();
    anyhow::ensure!(names.iter().all(|n| n.starts_with(&root)), "the file holds more than one plugin's folder");
    Ok(root)
}

/// What a plugin file is, without unpacking it. It may not have the name of one
/// that comes with Kite (`builtin`).
pub fn inspect(bytes: &[u8], builtin: &[String]) -> anyhow::Result<Plugin> {
    anyhow::ensure!(bytes.len() as u64 <= MAX_PACKAGE, "the file is too large for a plugin");
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow::anyhow!("not a plugin file (a .zip)"))?;
    let root = package_root(&zip)?;
    let mut text = String::new();
    zip.by_name(&format!("{root}plugin.json"))?.read_to_string(&mut text)?;
    let p = Plugin::parse(&text)?;
    anyhow::ensure!(!builtin.contains(&p.id), "Kite has its own plugin called {}", p.id);
    let names = package_names(&zip)?;
    let folder = format!("plugins/{}/", p.id);
    for file in p.files() {
        let inside = format!("{root}{}", file.strip_prefix(&folder).unwrap_or(file));
        anyhow::ensure!(names.contains(&inside), "the file doesn't have {file}, which its plugin.json names");
    }
    Ok(p)
}

/// Unpacks a plugin file into `<dir>/<id>/`, replacing what's there only once all of
/// it is unpacked.
pub fn install(bytes: &[u8], builtin: &[String], dir: &Path) -> anyhow::Result<Plugin> {
    let p = inspect(bytes, builtin)?;
    let root = package_root(&zip::ZipArchive::new(Cursor::new(bytes))?)?;
    std::fs::create_dir_all(dir)?;
    let staging = dir.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let done = (|| -> anyhow::Result<()> {
        crate::webext::xpi::extract(bytes, &staging)?;
        let dest = dir.join(&p.id);
        let old = dir.join(format!(".old-{}", uuid::Uuid::new_v4()));
        if dest.exists() {
            std::fs::rename(&dest, &old)?;
        }
        if let Err(e) = std::fs::rename(staging.join(root.trim_end_matches('/')), &dest) {
            if old.exists() {
                std::fs::rename(&old, &dest)?;
            }
            return Err(e.into());
        }
        if old.exists() {
            std::fs::remove_dir_all(&old)?;
        }
        Ok(())
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    done?;
    Ok(p)
}

/// The installed plugins that can be used, in order, and what's wrong with the others.
pub fn list_installed(dir: &Path, builtin: &[String]) -> (Vec<Plugin>, Vec<String>) {
    let (mut out, mut broken) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else { return (out, broken) };
    let mut folders: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    folders.sort();
    for folder in folders {
        let name = folder.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let checked = (|| -> anyhow::Result<Plugin> {
            let mut p = Plugin::parse(&std::fs::read_to_string(folder.join("plugin.json"))?)?;
            anyhow::ensure!(p.id == name, "its folder has another name than its id, {}", p.id);
            anyhow::ensure!(!builtin.contains(&p.id), "Kite has its own plugin called {}", p.id);
            let prefix = format!("plugins/{}/", p.id);
            for file in p.files() {
                anyhow::ensure!(folder.join(file.strip_prefix(&prefix).unwrap_or(file)).is_file(), "{file} is missing");
            }
            p.installed = true;
            // Never on by itself, whatever it says.
            p.default_enabled = false;
            Ok(p)
        })();
        match checked {
            Ok(p) => out.push(p),
            Err(e) => broken.push(format!("{name}: {e}")),
        }
    }
    (out, broken)
}

/// Deletes an installed plugin.
pub fn remove(dir: &Path, id: &str) -> anyhow::Result<()> {
    let good = !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    anyhow::ensure!(good, "no plugin called {id}");
    Ok(std::fs::remove_dir_all(dir.join(id))?)
}

/// A file of an installed plugin, given as in its manifest ("plugins/<id>/page.js",
/// `dir` being the folder of those). Nothing outside `dir`, whatever is in there.
pub fn read_installed_file(dir: &Path, file: &str) -> anyhow::Result<String> {
    let rel = file.strip_prefix("plugins/").ok_or_else(|| anyhow::anyhow!("not a plugin's file: {file}"))?;
    anyhow::ensure!(!rel.split('/').any(|part| part == ".." || part.is_empty()), "not a plugin's file: {file}");
    let path = dir.join(rel).canonicalize()?;
    anyhow::ensure!(path.starts_with(dir.canonicalize()?), "{file} is outside the plugins' folder");
    Ok(std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal() {
        let p = Plugin::parse(r#"{"id": "a-1", "name": "A", "description": "d", "icon": "i"}"#).unwrap();
        assert!(!p.default_enabled);
        assert_eq!(p.files().count(), 0);
    }

    #[test]
    fn refuses_what_would_go_wrong_later() {
        let with = |rest: &str| Plugin::parse(&format!(r#"{{"name": "A", "description": "d", "icon": "i", {rest}}}"#));
        assert!(with(r#""id": "Bad.id""#).is_err());
        assert!(with(r#""id": """#).is_err());
        assert!(with(r#""id": "a", "typo": 1"#).is_err());
        assert!(with(r#""id": "a", "settings": {"enabled": true}"#).is_err());
        assert!(with(r#""id": "a", "window": "plugins/b/W.qml""#).is_err());
        assert!(with(r#""id": "a", "window": "plugins/a/../b/W.qml""#).is_err());
        assert!(with(r#""id": "a", "window": "plugins/a/W.qml""#).is_ok());
        let action = r#""actions": [{"id": "a.go", "group": "G", "name": "N", "icon": "i"}]"#;
        assert!(with(&format!(r#""id": "a", {action}"#)).is_err(), "no window to run it");
        assert!(with(&format!(r#""id": "a", "window": "plugins/a/W.qml", {action}"#)).is_ok());
        assert!(with(&format!(r#""id": "b", "window": "plugins/b/W.qml", {action}"#)).is_err(), "another's id");
        let script = |world: u32, file: &str| format!(r#""id": "a", "pageScripts": [{{"files": ["{file}"], "worldId": {world}, "runsOnSubFrames": true, "injectionPoint": 0}}]"#);
        assert!(with(&script(1, "plugins/a/p.js")).is_ok());
        assert!(with(&script(0, "plugins/a/p.js")).is_ok(), "the page's world: Main.qml gives it nothing secret");
        assert!(with(&script(1, "plugins/b/p.js")).is_err(), "another's file");
    }

    #[test]
    fn ids_are_unique() {
        let a = r#"{"id": "a", "name": "A", "description": "d", "icon": "i"}"#;
        let b = r#"{"id": "b", "name": "B", "description": "d", "icon": "i"}"#;
        assert_eq!(parse_all(&[a, b]).unwrap().len(), 2);
        assert!(parse_all(&[a, b, a]).is_err());
    }

    // ---- installed from a file ----
    use crate::webext::xpi::make_xpi;

    const MANIFEST: &str = r#"{"id": "wc", "name": "WC", "description": "d", "icon": "i", "version": "1.2", "defaultEnabled": true,
        "window": "plugins/wc/W.qml", "pageScripts": [{"files": ["plugins/wc/p.js"], "worldId": 1, "runsOnSubFrames": false, "injectionPoint": 0}]}"#;
    fn package() -> Vec<u8> {
        make_xpi(&[("plugin.json", MANIFEST), ("W.qml", "Item {}"), ("p.js", "1")])
    }

    #[test]
    fn a_file_is_described_before_anything_is_unpacked() {
        let p = inspect(&package(), &[]).unwrap();
        assert_eq!((p.id.as_str(), p.version.as_deref()), ("wc", Some("1.2")));
        assert!(inspect(b"not a zip", &[]).is_err());
        assert!(inspect(&make_xpi(&[("other.txt", "x")]), &[]).is_err(), "no manifest");
        assert!(inspect(&make_xpi(&[("plugin.json", "{")]), &[]).is_err());
        let missing = inspect(&make_xpi(&[("plugin.json", MANIFEST), ("W.qml", "")]), &[]).unwrap_err();
        assert!(missing.to_string().contains("p.js"), "{missing}");
        assert!(inspect(&package(), &["wc".to_owned()]).unwrap_err().to_string().contains("its own"));
        assert!(inspect(&vec![0u8; MAX_PACKAGE as usize + 1], &[]).unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn a_manifest_cant_claim_more_than_it_is() {
        let with = |extra: &str| make_xpi(&[("plugin.json", &MANIFEST.replacen('{', &format!("{{{extra}, "), 1)), ("W.qml", ""), ("p.js", "")]);
        assert!(inspect(&with(r#""installed": false"#), &[]).is_err());
        assert!(inspect(&with(r#""baseUrl": "qrc:/""#), &[]).is_err());
        // Listing it can't fetch anything.
        let icon = |i: &str| Plugin::parse(&format!(r#"{{"id": "a", "name": "A", "description": "d", "icon": "{i}"}}"#));
        assert!(icon("https://evil.example/x.png").is_err());
        assert!(icon("file:///etc/passwd").is_err());
        assert!(icon("../x").is_err());
        assert!(icon("kite-pip").is_ok());
    }

    #[test]
    fn whats_shown_of_a_manifest_is_only_text() {
        let with = |name: &str, description: &str| Plugin::parse(&serde_json::json!({"id": "a", "name": name, "description": description, "icon": "i"}).to_string());
        assert!(with("Fine & dandy", "It's > all right.").is_ok());
        for bad in ["<b>A</b>", "<img src=\"http://x/\">", "a\nb", "evil\u{202e}txt"] {
            assert!(with(bad, "d").is_err(), "name {bad:?}");
            assert!(with("A", bad).is_err(), "description {bad:?}");
        }
        assert!(with(" ", "d").is_err());
        assert!(with(&"x".repeat(65), "d").is_err());
        assert!(with("A", &"x".repeat(601)).is_err());
        let action = |name: &str| Plugin::parse(&serde_json::json!({"id": "a", "name": "A", "description": "d", "icon": "i", "window": "plugins/a/W.qml",
            "actions": [{"id": "a.b", "group": "G", "name": name, "icon": "i"}]}).to_string());
        assert!(action("Go").is_ok());
        assert!(action("<img src=x>").is_err());
        let version = |v: &str| Plugin::parse(&serde_json::json!({"id": "a", "name": "A", "description": "d", "icon": "i", "version": v}).to_string());
        assert!(version("1.2.3").is_ok());
        assert!(version("<b>1</b>").is_err());
    }

    #[test]
    fn installs_lists_and_removes() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("plugins");
        assert!(list_installed(&dir, &[]).0.is_empty(), "no folder yet");
        install(&package(), &[], &dir).unwrap();
        assert!(dir.join("wc/W.qml").is_file());
        let (list, broken) = list_installed(&dir, &[]);
        assert!(broken.is_empty(), "{broken:?}");
        assert!(list[0].installed);
        assert!(!list[0].default_enabled, "starts off whatever it says");
        assert_eq!(read_installed_file(&dir, "plugins/wc/p.js").unwrap(), "1");
        // No leftovers.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        remove(&dir, "wc").unwrap();
        assert!(!dir.join("wc").exists());
        assert!(remove(&dir, "../plugins").is_err());
        assert!(remove(&dir, "wc").is_err());
    }

    #[test]
    fn a_zipped_folder_works_too() {
        let t = tempfile::tempdir().unwrap();
        let zipped = make_xpi(&[("wc-1.2/plugin.json", MANIFEST), ("wc-1.2/W.qml", "Item {}"), ("wc-1.2/p.js", "1")]);
        install(&zipped, &[], t.path()).unwrap();
        assert!(t.path().join("wc/plugin.json").is_file());
        // Two folders of plugins, or one with other things beside it, aren't one plugin.
        assert!(inspect(&make_xpi(&[("a/plugin.json", MANIFEST), ("b/plugin.json", MANIFEST)]), &[]).is_err());
        assert!(inspect(&make_xpi(&[("a/plugin.json", MANIFEST), ("a/W.qml", ""), ("a/p.js", ""), ("other.txt", "")]), &[]).is_err());
    }

    #[test]
    fn an_update_replaces_it_and_a_bad_one_changes_nothing() {
        let t = tempfile::tempdir().unwrap();
        install(&package(), &[], t.path()).unwrap();
        std::fs::write(t.path().join("wc/old-only.txt"), "x").unwrap();
        install(&make_xpi(&[("plugin.json", MANIFEST), ("W.qml", "new"), ("p.js", "2")]), &[], t.path()).unwrap();
        assert_eq!(std::fs::read_to_string(t.path().join("wc/W.qml")).unwrap(), "new");
        assert!(!t.path().join("wc/old-only.txt").exists());
        // Writing outside the folder fails, and the installed one stays.
        assert!(install(&make_xpi(&[("plugin.json", MANIFEST), ("W.qml", ""), ("p.js", ""), ("../evil.txt", "x")]), &[], t.path()).is_err());
        assert_eq!(std::fs::read_to_string(t.path().join("wc/W.qml")).unwrap(), "new");
        assert!(!t.path().parent().unwrap().join("evil.txt").exists());
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1, "no leftovers");
    }

    #[test]
    fn broken_folders_are_reported_and_cant_pretend() {
        let t = tempfile::tempdir().unwrap();
        install(&package(), &[], t.path()).unwrap();
        std::fs::create_dir(t.path().join("nomanifest")).unwrap();
        std::fs::create_dir(t.path().join("renamed")).unwrap();
        std::fs::write(t.path().join("renamed/plugin.json"), r#"{"id": "other", "name": "A", "description": "d", "icon": "i"}"#).unwrap();
        std::fs::create_dir(t.path().join("pip")).unwrap();
        std::fs::write(t.path().join("pip/plugin.json"), r#"{"id": "pip", "name": "A", "description": "d", "icon": "i"}"#).unwrap();
        std::fs::create_dir(t.path().join(".staging-1")).unwrap();
        let (list, broken) = list_installed(t.path(), &["pip".to_owned()]);
        assert_eq!(list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["wc"]);
        assert_eq!(broken.len(), 3, "{broken:?}");
        std::fs::remove_file(t.path().join("wc/p.js")).unwrap();
        assert!(list_installed(t.path(), &[]).1.iter().any(|b| b.contains("p.js")));
    }

    #[test]
    fn its_files_stay_in_their_folder() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("plugins");
        install(&package(), &[], &dir).unwrap();
        std::fs::write(t.path().join("secret"), "s").unwrap();
        for bad in ["plugins/../secret", "plugins/wc/../../secret", "secret", "/etc/passwd", "plugins//etc/passwd"] {
            assert!(read_installed_file(&dir, bad).is_err(), "{bad}");
        }
        std::os::unix::fs::symlink(t.path().join("secret"), dir.join("wc/link")).unwrap();
        assert!(read_installed_file(&dir, "plugins/wc/link").is_err(), "through a link");
    }

    #[test]
    fn a_link_in_a_file_comes_out_as_a_plain_file() {
        use std::io::Write;
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("secret"), "s").unwrap();
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let options = zip::write::SimpleFileOptions::default();
            for (name, body) in [("plugin.json", MANIFEST), ("W.qml", ""), ("p.js", "")] {
                z.start_file(name, options).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.add_symlink("link", t.path().join("secret").to_string_lossy(), options).unwrap();
            z.finish().unwrap();
        }
        let dir = t.path().join("plugins");
        install(&buf.into_inner(), &[], &dir).unwrap();
        assert!(!dir.join("wc/link").is_symlink());
    }
}
