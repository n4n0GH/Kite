//! Content blocking (Falkon's AdBlock), backed by Brave's `adblock` engine.
//!
//! Subscriptions are plain ABP/uBO filter lists downloaded into
//! `<profile>/adblock/`. The engine is rebuilt on a worker thread and swapped
//! in atomically, so lookups from the network interceptor never wait on I/O.

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Subscription {
    pub title: String,
    pub url: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Allow,
    Block,
    Redirect(String),
}

/// Refresh downloaded lists older than this.
const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(4 * 24 * 3600);

/// Starts `engine.dat`, followed by the fingerprint of what the engine was built
/// from and the serialized engine. In one file, so they can't disagree after a crash.
const CACHE_MAGIC: &[u8] = b"KITEAB1\n";

type Fingerprint = [u8; 32];

pub struct AdBlock {
    dir: PathBuf,
    engine: RwLock<Option<Arc<Engine>>>,
    /// What the current engine was built from, if known.
    built_from: Mutex<Option<Fingerprint>>,
    allowlist: RwLock<Vec<String>>,
    enabled: std::sync::atomic::AtomicBool,
    pub blocked_count: std::sync::atomic::AtomicU64,
}

pub fn list_path(dir: &Path, url: &str) -> PathBuf {
    let name: String = url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    dir.join(format!("{}.txt", &name[name.len().saturating_sub(80)..]))
}

/// Map Qt's resource types to the adblock crate's request types.
pub fn request_type(qt: &str) -> &'static str {
    match qt {
        "mainFrame" => "document",
        "subFrame" => "subdocument",
        "stylesheet" => "stylesheet",
        "script" | "serviceWorker" | "sharedWorker" | "worker" => "script",
        "image" | "favicon" => "image",
        "fontResource" => "font",
        "media" => "media",
        "object" | "pluginResource" => "object",
        "xhr" => "xmlhttprequest",
        "ping" | "cspReport" => "ping",
        "websocket" => "websocket",
        _ => "other",
    }
}

impl AdBlock {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            engine: RwLock::new(None),
            built_from: Mutex::new(None),
            allowlist: RwLock::new(vec![]),
            enabled: true.into(),
            blocked_count: 0.into(),
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn set_allowlist(&self, hosts: Vec<String>) {
        *self.allowlist.write().unwrap() = hosts;
    }

    fn allowlisted(&self, first_party: &str) -> bool {
        let host = crate::util::host_of(first_party);
        self.allowlist.read().unwrap().iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
    }

    /// Build a new engine from cached lists (downloading missing/stale ones
    /// when `download` is true) plus custom rules. Blocking; call off the UI thread.
    pub fn rebuild(&self, subs: &[Subscription], custom_rules: &str, download: bool) -> anyhow::Result<usize> {
        if download {
            self.download_stale(subs);
        }
        self.build(subs, custom_rules)
    }

    /// What a start does: download missing/stale lists, then build a new engine only
    /// if the one loaded by `load_cached` wasn't built from these lists and rules.
    /// Returns the filter lines if it built one. Blocking; call off the UI thread.
    pub fn refresh(&self, subs: &[Subscription], custom_rules: &str) -> anyhow::Result<Option<usize>> {
        self.download_stale(subs);
        if *self.built_from.lock().unwrap() == Some(self.fingerprint(subs, custom_rules)) {
            return Ok(None);
        }
        self.build(subs, custom_rules).map(Some)
    }

    fn download_stale(&self, subs: &[Subscription]) {
        for s in subs.iter().filter(|s| s.enabled) {
            let path = list_path(&self.dir, &s.url);
            let stale = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .map(|t| t.elapsed().unwrap_or_default() > MAX_AGE)
                .unwrap_or(true);
            if stale {
                match fetch(&s.url) {
                    Ok(body) => {
                        let _ = crate::util::write_atomic(&path, body.as_bytes());
                    }
                    Err(e) => log::warn!("adblock: failed to fetch {}: {e}", s.url),
                }
            }
        }
    }

    /// Changes when anything an engine is built from does: which lists are enabled,
    /// their files, the custom rules, and Kite itself (which may bring a new engine).
    fn fingerprint(&self, subs: &[Subscription], custom_rules: &str) -> Fingerprint {
        let mut input = format!("{}\n", env!("CARGO_PKG_VERSION"));
        for s in subs.iter().filter(|s| s.enabled) {
            let file = std::fs::metadata(list_path(&self.dir, &s.url)).ok();
            let modified = file.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            input += &format!("{}\0{:?}\0{:?}\n", s.url, file.map(|m| m.len()), modified);
        }
        input += custom_rules;
        crate::webext::amo::sha256(input.as_bytes())
    }

    fn build(&self, subs: &[Subscription], custom_rules: &str) -> anyhow::Result<usize> {
        // (Before reading the lists: if one changes meanwhile, the next start notices.)
        let fingerprint = self.fingerprint(subs, custom_rules);
        let mut set = FilterSet::new(false);
        let mut lines = 0usize;
        for s in subs.iter().filter(|s| s.enabled) {
            if let Ok(text) = std::fs::read_to_string(list_path(&self.dir, &s.url)) {
                lines += text.lines().count();
                set.add_filter_list(text, ParseOptions::default());
            }
        }
        set.add_filter_list(custom_rules.to_owned(), ParseOptions::default());
        let engine = Engine::new_with_filter_set(set);
        // Cache the compiled engine so the next start blocks from the first request.
        let cache = [CACHE_MAGIC, &fingerprint[..], &engine.serialize()].concat();
        let _ = crate::util::write_atomic(&self.dir.join("engine.dat"), &cache);
        *self.engine.write().unwrap() = Some(Arc::new(engine));
        *self.built_from.lock().unwrap() = Some(fingerprint);
        Ok(lines)
    }

    /// Load the engine compiled by a previous run (fast; call at startup).
    pub fn load_cached(&self) -> bool {
        let Ok(bytes) = std::fs::read(self.dir.join("engine.dat")) else { return false };
        // (A file from before the fingerprint still blocks until `refresh` replaces it.)
        let (fingerprint, data) = match bytes.strip_prefix(CACHE_MAGIC).filter(|rest| rest.len() >= 32) {
            Some(rest) => {
                let (f, data) = rest.split_at(32);
                (Fingerprint::try_from(f).ok(), data)
            }
            None => (None, &bytes[..]),
        };
        let mut engine = Engine::default();
        if engine.deserialize(data).is_err() {
            return false;
        }
        *self.engine.write().unwrap() = Some(Arc::new(engine));
        *self.built_from.lock().unwrap() = fingerprint;
        true
    }

    pub fn check(&self, url: &str, first_party: &str, qt_type: &str) -> Verdict {
        if !self.is_enabled() || !(url.starts_with("http:") || url.starts_with("https:") || url.starts_with("ws")) {
            return Verdict::Allow;
        }
        if qt_type == "mainFrame" || self.allowlisted(first_party) {
            return Verdict::Allow;
        }
        let Some(engine) = self.engine.read().unwrap().clone() else { return Verdict::Allow };
        let source = if first_party.is_empty() { url } else { first_party };
        let Ok(req) = Request::new(url, source, request_type(qt_type), "GET") else { return Verdict::Allow };
        let r = engine.check_network_request(&req);
        if r.should_block() {
            self.blocked_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some(redirect) = r.redirect {
                return Verdict::Redirect(redirect);
            }
            return Verdict::Block;
        }
        if let Some(rewritten) = r.rewritten_url {
            return Verdict::Redirect(rewritten);
        }
        Verdict::Allow
    }

    /// CSS (element hiding) and scriptlet JS for a page.
    pub fn cosmetic(&self, url: &str) -> (String, String) {
        if !self.is_enabled() || self.allowlisted(url) {
            return (String::new(), String::new());
        }
        let Some(engine) = self.engine.read().unwrap().clone() else { return Default::default() };
        let res = engine.url_cosmetic_resources(url);
        let mut css = String::new();
        let selectors: Vec<&String> = res.hide_selectors.iter().collect();
        for chunk in selectors.chunks(500) {
            css += &chunk.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",\n");
            css += " { display: none !important; }\n";
        }
        (css, res.injected_script)
    }
}

fn fetch(url: &str) -> anyhow::Result<String> {
    let mut resp = ureq::get(url).header("User-Agent", crate::webext::amo::USER_AGENT).call()?;
    Ok(resp.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_and_hides() {
        let dir = tempfile::tempdir().unwrap();
        let ab = AdBlock::new(dir.path().into());
        ab.rebuild(&[], "||ads.example.com^\nexample.org##.banner\n@@||ads.example.com/ok^", false).unwrap();
        assert_eq!(ab.check("https://ads.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
        assert_eq!(ab.check("https://ads.example.com/ok/x.js", "https://news.test/", "script"), Verdict::Allow);
        assert_eq!(ab.check("https://ads.example.com/", "", "mainFrame"), Verdict::Allow);
        assert!(ab.cosmetic("https://example.org/").0.contains(".banner"));
        ab.set_allowlist(vec!["news.test".into()]);
        assert_eq!(ab.check("https://ads.example.com/x.js", "https://www.news.test/", "script"), Verdict::Allow);
        let cached = AdBlock::new(dir.path().into());
        assert!(cached.load_cached());
        assert_eq!(cached.check("https://ads.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
    }

    // A list already on disk, so `refresh` has nothing to download.
    fn sub(dir: &Path, name: &str, rules: &str) -> Subscription {
        let url = format!("http://127.0.0.1:9/{name}");
        std::fs::write(list_path(dir, &url), rules).unwrap();
        Subscription { title: name.into(), url, enabled: true }
    }

    fn started(dir: &Path) -> AdBlock {
        let ab = AdBlock::new(dir.into());
        ab.load_cached();
        ab
    }

    #[test]
    fn a_start_with_nothing_changed_builds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let subs = [sub(dir.path(), "a", "||ads.example.com^")];
        assert_eq!(started(dir.path()).refresh(&subs, "").unwrap(), Some(1), "nothing cached yet");
        let ab = started(dir.path());
        assert_eq!(ab.refresh(&subs, "").unwrap(), None);
        assert_eq!(ab.check("https://ads.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
    }

    #[test]
    fn a_start_builds_when_what_the_engine_came_from_changed() {
        let dir = tempfile::tempdir().unwrap();
        let (a, mut b) = (sub(dir.path(), "a", "||ads.example.com^"), sub(dir.path(), "b", "||other.example.com^"));
        started(dir.path()).refresh(&[a.clone(), b.clone()], "").unwrap();

        // A list switched off in Settings, which only sets the setting.
        b.enabled = false;
        let ab = started(dir.path());
        assert!(ab.refresh(&[a.clone(), b.clone()], "").unwrap().is_some());
        assert_eq!(ab.check("https://other.example.com/x.js", "https://news.test/", "script"), Verdict::Allow);

        // The custom rules changed (Settings, or kite:config).
        let ab = started(dir.path());
        assert!(ab.refresh(&[a.clone(), b.clone()], "||custom.example.com^").unwrap().is_some());
        assert_eq!(ab.check("https://custom.example.com/x.js", "https://news.test/", "script"), Verdict::Block);

        // A list file changed.
        std::fs::write(list_path(dir.path(), &a.url), "||new.example.com^\n||newer.example.com^").unwrap();
        let ab = started(dir.path());
        assert!(ab.refresh(&[a.clone(), b.clone()], "||custom.example.com^").unwrap().is_some());
        assert_eq!(ab.check("https://new.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
    }

    #[test]
    fn a_stale_list_that_cant_be_downloaded_builds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let subs = [sub(dir.path(), "a", "||ads.example.com^")];
        let old = std::time::SystemTime::now() - 2 * MAX_AGE;
        std::fs::File::options().write(true).open(list_path(dir.path(), &subs[0].url)).unwrap().set_modified(old).unwrap();
        started(dir.path()).refresh(&subs, "").unwrap();
        assert_eq!(started(dir.path()).refresh(&subs, "").unwrap(), None);
    }

    #[test]
    fn a_cache_from_before_the_fingerprint_blocks_until_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let subs = [sub(dir.path(), "a", "||ads.example.com^")];
        let mut set = FilterSet::new(false);
        set.add_filter_list("||old.example.com^".to_owned(), ParseOptions::default());
        std::fs::write(dir.path().join("engine.dat"), Engine::new_with_filter_set(set).serialize()).unwrap();

        let ab = AdBlock::new(dir.path().into());
        assert!(ab.load_cached());
        assert_eq!(ab.check("https://old.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
        assert_eq!(ab.refresh(&subs, "").unwrap(), Some(1));
        assert_eq!(ab.check("https://ads.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
        assert_eq!(started(dir.path()).refresh(&subs, "").unwrap(), None);
    }
}
