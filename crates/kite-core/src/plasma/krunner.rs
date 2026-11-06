//! Search from the desktop: KRunner, and the launchers built on it, find what's open
//! in Kite, its bookmarks and its history.
//!
//! The interface is `org.kde.krunner1`. KRunner learns of it from a small file in
//! `krunner/dbusplugins`, which names the service with a wildcard: each running Kite
//! has a name of its own, and with none running nothing is asked.
//! <https://develop.kde.org/docs/plasma/krunner/>

use crate::shared::Shared;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

const PATH: &str = "/krunner";
const NAME: &str = "org.kde.kite.krunner";
const FILE_NAME: &str = "org.kde.kite.desktop";
const MIN_LETTERS: usize = 3;
const MAX_BOOKMARKS: usize = 5;
const MAX_HISTORY: usize = 5;
/// How long to wait for the interface to say what tabs there are. A stuck window
/// shouldn't hold up the desktop's search.
const TABS_TIMEOUT: Duration = Duration::from_millis(300);
/// Asked on every key press, tabs don't need asking for each.
const TABS_FRESH: Duration = Duration::from_secs(2);
// KRunner::QueryMatch::CategoryRelevance
const HIGH: i32 = 70;
const MODERATE: i32 = 50;
const LOW: i32 = 30;

const RUNNER_FILE: &str = "[Desktop Entry]
Name=Kite
Comment=Find Kite's open tabs, bookmarks and history
X-KDE-ServiceTypes=Plasma/Runner
Type=Service
Icon=org.kde.kite
X-KDE-PluginInfo-Name=org.kde.kite
X-KDE-PluginInfo-EnabledByDefault=true
X-Plasma-API=DBus
X-Plasma-DBusRunner-Service=org.kde.kite.krunner*
X-Plasma-DBusRunner-Path=/krunner
X-Plasma-Request-Actions-Once=true
X-Plasma-Runner-Min-Letter-Count=3
X-Plasma-Runner-Syntaxes=:q:
X-Plasma-Runner-Syntax-Descriptions=Finds Kite's open tabs, bookmarks and history matching :q:
";

/// What's being searched. All off: not searchable at all.
#[derive(Deserialize, Clone, Copy, Default, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    pub tabs: bool,
    pub bookmarks: bool,
    pub history: bool,
}

impl Config {
    fn any(&self) -> bool {
        self.tabs || self.bookmarks || self.history
    }
}

/// An open tab, as the interface sees it.
#[derive(Deserialize, Clone, PartialEq, Debug)]
pub struct Tab {
    pub tab: i32,
    pub title: String,
    pub url: String,
}

/// One thing to offer.
#[derive(Clone, PartialEq, Debug)]
pub struct Found {
    /// "tab:<id>" or "url:<address>".
    pub id: String,
    pub text: String,
    pub subtext: String,
    pub category: &'static str,
    pub relevance_class: i32,
    pub relevance: f64,
    pub url: String,
}

/// What was picked.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Picked {
    pub tab: Option<i32>,
    pub url: Option<String>,
    /// To be allowed to take the focus, on Wayland.
    pub token: String,
}

fn openable(url: &str) -> bool {
    let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "file")
}

fn matches(terms: &[String], title: &str, url: &str) -> bool {
    let (title, url) = (title.to_lowercase(), url.to_lowercase());
    !terms.is_empty() && terms.iter().all(|t| title.contains(t) || url.contains(t))
}

/// How well it fits, of what `matches`.
fn closeness(query: &str, title: &str, url: &str) -> f64 {
    let title = title.to_lowercase();
    let host = url
        .to_lowercase()
        .split_once("://")
        .map(|(_, rest)| rest.trim_start_matches("www.").to_owned())
        .unwrap_or_default();
    if title == query {
        1.0
    } else if title.starts_with(query) || host.starts_with(query) {
        0.9
    } else if title.contains(query) {
        0.7
    } else {
        0.5
    }
}

fn shown_title(title: &str, url: &str) -> String {
    let title = title.trim();
    if title.is_empty() { url.to_owned() } else { title.to_owned() }
}

/// What fits the query: tabs, then bookmarks, then history, with each address once.
pub fn find(query: &str, tabs: &[Tab], bookmarks: &[(String, String)], history: &[(String, String)]) -> Vec<Found> {
    let query = query.trim().to_lowercase();
    if query.chars().count() < MIN_LETTERS {
        return vec![];
    }
    let terms: Vec<String> = query.split_whitespace().map(str::to_owned).collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for t in tabs {
        if matches(&terms, &t.title, &t.url) {
            seen.insert(t.url.clone());
            let close = closeness(&query, &t.title, &t.url);
            out.push(Found {
                id: format!("tab:{}", t.tab),
                text: shown_title(&t.title, &t.url),
                subtext: t.url.clone(),
                category: "Kite Tabs",
                relevance_class: if close >= 0.9 { HIGH } else { MODERATE },
                relevance: close,
                url: t.url.clone(),
            });
        }
    }
    let mut add = |list: &[(String, String)], most: usize, category: &'static str, class: i32, out: &mut Vec<Found>| {
        let mut left = most;
        for (title, url) in list {
            if left == 0 {
                break;
            }
            if openable(url) && matches(&terms, title, url) && seen.insert(url.clone()) {
                left -= 1;
                out.push(Found {
                    id: format!("url:{url}"),
                    text: shown_title(title, url),
                    subtext: url.clone(),
                    category,
                    relevance_class: class,
                    relevance: closeness(&query, title, url),
                    url: url.clone(),
                });
            }
        }
    };
    add(bookmarks, MAX_BOOKMARKS, "Kite Bookmarks", MODERATE, &mut out);
    add(history, MAX_HISTORY, "Kite History", LOW, &mut out);
    out
}

/// What the desktop asks for, turned into what the browser is to do. Whatever isn't
/// something `find` could have offered is left alone.
fn picked(id: &str, token: String) -> Option<Picked> {
    if let Some(tab) = id.strip_prefix("tab:") {
        return Some(Picked { tab: Some(tab.parse().ok()?), url: None, token });
    }
    let url = id.strip_prefix("url:")?;
    openable(url).then(|| Picked { tab: None, url: Some(url.to_owned()), token })
}

fn data_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")),
    }
}

/// Whether it had to be written.
fn install(data_home: &Path) -> std::io::Result<bool> {
    let dir = data_home.join("krunner/dbusplugins");
    let file = dir.join(FILE_NAME);
    if std::fs::read_to_string(&file).is_ok_and(|t| t == RUNNER_FILE) {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(file, RUNNER_FILE)?;
    Ok(true)
}

/// Whether there was one.
fn uninstall(data_home: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(data_home.join("krunner/dbusplugins").join(FILE_NAME)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// KRunner reads that folder when it starts. Its configuration changing should make it
/// read it again, which is what this says, as System Settings would.
fn tell_krunner() -> zbus::Result<()> {
    let connection = Connection::session()?;
    let changed: HashMap<&str, Vec<Vec<u8>>> = HashMap::from([("Plugins", vec![b"org.kde.kiteEnabled".to_vec()])]);
    connection.emit_signal(None::<&str>, "/kconfig/krunnerrc", "org.kde.kconfig.notify", "ConfigChanged", &(changed,))
}

type RemoteMatch = (String, String, String, i32, f64, HashMap<String, OwnedValue>);

fn remote(found: Found) -> RemoteMatch {
    let mut properties = HashMap::new();
    let mut put = |key: &str, v: Value<'_>| {
        if let Ok(v) = v.try_to_owned() {
            properties.insert(key.to_owned(), v);
        }
    };
    put("subtext", Value::from(found.subtext));
    put("category", Value::from(found.category));
    put("urls", Value::from(vec![found.url]));
    put("actions", Value::from(Vec::<String>::new()));
    (found.id, found.text, "org.kde.kite".to_owned(), found.relevance_class, found.relevance, properties)
}

#[derive(Default)]
struct TabsCache {
    tabs: Vec<Tab>,
    asked: Option<Instant>,
    waiting: Vec<Sender<Vec<Tab>>>,
}

/// What it's given to talk to the browser's interface.
pub trait Browser: Send + Sync + 'static {
    /// Should end up in `Runner::answer_tabs`.
    fn wants_tabs(&self);
    fn picked(&self, picked: Picked);
}

struct Inner {
    shared: Shared,
    browser: Box<dyn Browser>,
    config: Mutex<Config>,
    tabs: Mutex<TabsCache>,
    token: Mutex<String>,
}

impl Inner {
    fn current_tabs(&self) -> Vec<Tab> {
        let wait: Receiver<Vec<Tab>> = {
            let mut cache = self.tabs.lock().unwrap_or_else(|e| e.into_inner());
            if cache.asked.is_some_and(|t| t.elapsed() < TABS_FRESH) {
                return cache.tabs.clone();
            }
            let (answer, wait) = channel();
            cache.waiting.push(answer);
            wait
        };
        self.browser.wants_tabs();
        wait.recv_timeout(TABS_TIMEOUT).unwrap_or_default()
    }
}

struct Service(Arc<Inner>);

#[zbus::interface(name = "org.kde.krunner1")]
impl Service {
    #[zbus(name = "Match")]
    fn find_matches(&self, query: String) -> Vec<RemoteMatch> {
        if query.trim().chars().count() < MIN_LETTERS {
            return vec![];
        }
        let config = *self.0.config.lock().unwrap_or_else(|e| e.into_inner());
        let tabs = if config.tabs { self.0.current_tabs() } else { vec![] };
        let bookmarks: Vec<(String, String)> = if config.bookmarks {
            // (By one word: `find` looks at them all.)
            self.0.shared.bookmarks().search(query.split_whitespace().next().unwrap_or("")).into_iter().map(|n| (n.name.clone(), n.url.clone())).collect()
        } else {
            vec![]
        };
        let history: Vec<(String, String)> = if config.history {
            crate::history::complete(&self.0.shared.db(), &query, MAX_HISTORY * 4)
                .unwrap_or_default()
                .into_iter()
                .map(|e| (e.title, e.url))
                .collect()
        } else {
            vec![]
        };
        find(&query, &tabs, &bookmarks, &history).into_iter().map(remote).collect()
    }

    /// None: results open what they're about.
    fn actions(&self) -> Vec<(String, String, String)> {
        Vec::new()
    }

    fn set_activation_token(&self, token: String) {
        *self.0.token.lock().unwrap_or_else(|e| e.into_inner()) = token;
    }

    fn run(&self, match_id: String, _action_id: String) {
        let token = std::mem::take(&mut *self.0.token.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(p) = picked(&match_id, token) {
            self.0.browser.picked(p);
        }
    }

    fn teardown(&self) {
        let mut cache = self.0.tabs.lock().unwrap_or_else(|e| e.into_inner());
        cache.tabs = Vec::new();
        cache.asked = None;
    }
}

pub struct Runner {
    inner: Arc<Inner>,
    to_bus: Sender<Config>,
}

impl Runner {
    pub fn start(shared: Shared, browser: impl Browser) -> Self {
        let inner = Arc::new(Inner { shared, browser: Box::new(browser), config: Mutex::default(), tabs: Mutex::default(), token: Mutex::default() });
        let (to_bus, from_browser) = channel::<Config>();
        let service = inner.clone();
        std::thread::Builder::new()
            .name("kite-krunner".into())
            .spawn(move || {
                let mut connection = None;
                for config in from_browser {
                    *service.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
                    if config.any() && connection.is_none() {
                        connection = serve(&service);
                    } else if !config.any() {
                        connection = None;
                    }
                    if let Some(home) = data_home() {
                        let changed = if config.any() { install(&home) } else { uninstall(&home) };
                        match changed {
                            Ok(true) => {
                                if let Err(e) = tell_krunner() {
                                    log::debug!("KRunner: {e}");
                                }
                            }
                            Ok(false) => {}
                            Err(e) => log::warn!("KRunner: {e}"),
                        }
                    }
                }
            })
            .expect("KRunner thread");
        Self { inner, to_bus }
    }

    pub fn configure(&self, config: Config) {
        let _ = self.to_bus.send(config);
    }

    pub fn answer_tabs(&self, tabs: Vec<Tab>) {
        let mut cache = self.inner.tabs.lock().unwrap_or_else(|e| e.into_inner());
        cache.tabs = tabs.clone();
        cache.asked = Some(Instant::now());
        for waiting in cache.waiting.drain(..) {
            let _ = waiting.send(tabs.clone());
        }
    }
}

fn serve(inner: &Arc<Inner>) -> Option<Connection> {
    let name = format!("{NAME}.instance{}", std::process::id());
    let built = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(name))
        .and_then(|b| b.serve_at(PATH, Service(inner.clone())))
        .and_then(|b| b.build());
    match built {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("KRunner: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(tab: i32, title: &str, url: &str) -> Tab {
        Tab { tab, title: title.into(), url: url.into() }
    }
    fn pair(title: &str, url: &str) -> (String, String) {
        (title.into(), url.into())
    }

    #[test]
    fn short_queries_find_nothing() {
        assert!(find("gi", &[tab(1, "GitHub", "https://github.com/")], &[], &[]).is_empty());
        assert!(find("  gi ", &[tab(1, "GitHub", "https://github.com/")], &[], &[]).is_empty());
    }

    #[test]
    fn every_word_has_to_be_there_in_title_or_address() {
        let tabs = [tab(1, "Kite issues", "https://github.com/kite"), tab(2, "Kites for sale", "https://shop.example/")];
        let found = find("kite github", &tabs, &[], &[]);
        assert_eq!(found.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["tab:1"]);
        assert_eq!(find("KITE", &tabs, &[], &[]).len(), 2);
    }

    #[test]
    fn an_address_comes_once_and_the_tab_wins() {
        let url = "https://github.com/";
        let found = find("github", &[tab(1, "GitHub", url)], &[pair("GitHub", url), pair("My GitHub", "https://github.com/me")], &[pair("GitHub", url), pair("My GitHub", "https://github.com/me")]);
        let ids: Vec<&str> = found.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["tab:1", "url:https://github.com/me"]);
        assert_eq!(found[1].category, "Kite Bookmarks");
    }

    #[test]
    fn close_tabs_rank_above_loose_ones_and_history_below() {
        let found = find(
            "github",
            &[tab(1, "GitHub", "https://github.com/"), tab(2, "On github today", "https://news.example/")],
            &[],
            &[pair("GitHub stars", "https://github.com/stars")],
        );
        assert_eq!(found[0].relevance_class, HIGH);
        assert_eq!(found[1].relevance_class, MODERATE);
        assert_eq!(found[2].relevance_class, LOW);
        assert!(found[0].relevance > found[1].relevance);
    }

    #[test]
    fn bookmarks_and_history_have_limits() {
        let many: Vec<_> = (0..20).map(|i| pair("github", &format!("https://github.com/{i}"))).collect();
        let found = find("github", &[], &many, &many);
        assert_eq!(found.iter().filter(|f| f.category == "Kite Bookmarks").count(), MAX_BOOKMARKS);
        assert_eq!(found.iter().filter(|f| f.category == "Kite History").count(), MAX_HISTORY);
    }

    #[test]
    fn only_what_a_browser_tab_may_open_is_offered() {
        let bad = [pair("github", "javascript:alert(1)"), pair("github", "data:text/html,github"), pair("github", "kite:github")];
        assert!(find("github", &[], &bad, &bad).is_empty());
    }

    #[test]
    fn no_title_shows_the_address() {
        let found = find("github", &[tab(1, "  ", "https://github.com/")], &[], &[]);
        assert_eq!(found[0].text, "https://github.com/");
    }

    #[test]
    fn what_can_be_picked() {
        assert_eq!(picked("tab:5", "t".into()).unwrap().tab, Some(5));
        assert_eq!(picked("url:https://a.example/", "".into()).unwrap().url.as_deref(), Some("https://a.example/"));
        assert_eq!(picked("url:javascript:alert(1)", "".into()), None);
        assert_eq!(picked("tab:x", "".into()), None);
        assert_eq!(picked("something", "".into()), None);
    }

    #[test]
    fn the_file_for_krunner_comes_and_goes() {
        let dir = std::env::temp_dir().join(format!("kite-krunner-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(install(&dir).unwrap());
        assert!(!install(&dir).unwrap(), "already there");
        assert!(RUNNER_FILE.contains(&format!("X-Plasma-DBusRunner-Service={NAME}*")));
        assert!(RUNNER_FILE.contains(&format!("X-Plasma-DBusRunner-Path={PATH}")));
        assert!(uninstall(&dir).unwrap());
        assert!(!uninstall(&dir).unwrap(), "already gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
