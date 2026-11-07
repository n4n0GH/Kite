//! Content blocking (Falkon's AdBlock), backed by Brave's `adblock` engine.
//!
//! Subscriptions are plain ABP/uBO filter lists downloaded into
//! `<profile>/adblock/`. The engine is rebuilt on a worker thread and swapped
//! in atomically, so lookups from the network interceptor never wait on I/O.

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::resources::{PermissionMask, Resource};
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
/// Sooner for uBO's quick fixes: what sites change to get around blockers is answered there.
const QUICK_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(8 * 3600);

/// Starts `engine.dat`, followed by the fingerprint of what the engine was built
/// from and the serialized engine. In one file, so they can't disagree after a crash.
const CACHE_MAGIC: &[u8] = b"KITEAB1\n";

/// uBlock Origin's scriptlets and redirect resources, which its lists rely on
/// (packaging/update-adblock-resources.py makes this file).
const RESOURCES: &str = include_str!("../resources/ublock-resources.json");

/// What scriptlets marked as needing trust (they can rewrite what pages receive) ask of a list.
const TRUSTED: u8 = 1;

/// uBO only lets its own lists use those, and what the user writes is theirs.
fn trusted_list(url: &str) -> bool {
    url.starts_with("https://ublockorigin.github.io/uAssets/") || url.starts_with("https://raw.githubusercontent.com/uBlockOrigin/uAssets/")
}

fn parse_options(trusted: bool) -> ParseOptions {
    ParseOptions { permissions: PermissionMask::from_bits(if trusted { TRUSTED } else { 0 }), ..Default::default() }
}

fn load_resources(engine: &mut Engine) {
    #[derive(Deserialize)]
    struct Bundle {
        resources: Vec<Resource>,
    }
    match serde_json::from_str::<Bundle>(RESOURCES) {
        Ok(bundle) => engine.use_resources(bundle.resources),
        Err(e) => log::warn!("adblock: no scriptlets: {e}"),
    }
}

/// Is this `!#if` condition true here? Kite counts as uBO on Chromium, and what isn't
/// known is false, as in uBO.
fn condition_holds(text: &str) -> bool {
    fn token_holds(name: &str) -> bool {
        matches!(name, "ext_ublock" | "env_chromium" | "cap_user_stylesheet" | "true")
    }
    // (Lowest first: or, and, not, with parentheses.)
    fn or(tokens: &[String], at: &mut usize) -> bool {
        let mut value = and(tokens, at);
        while tokens.get(*at).is_some_and(|t| t == "||") {
            *at += 1;
            let right = and(tokens, at);
            value = value || right;
        }
        value
    }
    fn and(tokens: &[String], at: &mut usize) -> bool {
        let mut value = not(tokens, at);
        while tokens.get(*at).is_some_and(|t| t == "&&") {
            *at += 1;
            let right = not(tokens, at);
            value = value && right;
        }
        value
    }
    fn not(tokens: &[String], at: &mut usize) -> bool {
        match tokens.get(*at).map(String::as_str) {
            Some("!") => {
                *at += 1;
                !not(tokens, at)
            }
            Some("(") => {
                *at += 1;
                let value = or(tokens, at);
                *at += 1;
                value
            }
            Some(name) => {
                *at += 1;
                token_holds(name)
            }
            None => false,
        }
    }
    let mut tokens = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let len = if rest.starts_with("&&") || rest.starts_with("||") {
            2
        } else if rest.starts_with(['!', '(', ')']) {
            1
        } else {
            rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len()).max(1)
        };
        tokens.push(rest[..len].to_owned());
        rest = rest[len..].trim_start();
    }
    or(&tokens, &mut 0)
}

/// Applies a list's `!#if` directives, which the engine doesn't know: without this, what's
/// meant for other browsers applies as well. Lines left out keep the others' numbers apart.
fn preprocess(list: &str) -> String {
    // For each open `!#if`: is this part being kept, and was what's around it?
    let mut open: Vec<(bool, bool)> = Vec::new();
    let mut out = String::with_capacity(list.len());
    for line in list.lines() {
        if let Some(condition) = line.strip_prefix("!#if ") {
            let around = open.last().is_none_or(|o| o.0);
            open.push((around && condition_holds(condition), around));
        } else if line.starts_with("!#else") {
            if let Some(o) = open.last_mut() {
                o.0 = o.1 && !o.0;
            }
        } else if line.starts_with("!#endif") {
            open.pop();
        } else if open.last().is_none_or(|o| o.0) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// What a page gets from the lists, before it exists (WebTab.prepareAdblock).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PagePlan {
    /// Selectors to hide. Some aren't CSS but uBO's own: js/adblock_content.js sorts that out.
    pub hide: Vec<String>,
    /// Filters with an action, in the engine's JSON form.
    pub procedural: Vec<String>,
    /// Scriptlets, to run in the page's world before its own scripts.
    pub script: String,
    /// Generic rules don't apply here.
    pub generichide: bool,
    /// Classes and ids generic rules aren't to hide, for `generic_selectors`.
    pub exceptions: Vec<String>,
}

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
                .map(|t| t.elapsed().unwrap_or_default() > if s.url.contains("quick-fixes") { QUICK_MAX_AGE } else { MAX_AGE })
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
        // (The version of how lists are read, as Kite's own isn't always bumped in between.)
        let mut input = format!("{}\n2\n", env!("CARGO_PKG_VERSION"));
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
                set.add_filter_list(preprocess(&text), parse_options(trusted_list(&s.url)));
            }
        }
        set.add_filter_list(preprocess(custom_rules), parse_options(true));
        let mut engine = Engine::new_with_filter_set(set);
        load_resources(&mut engine);
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
        load_resources(&mut engine);
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

    /// What a page at this address gets, or None if blocking is off for it.
    pub fn page_plan(&self, url: &str) -> Option<PagePlan> {
        if !self.is_enabled() || self.allowlisted(url) {
            return None;
        }
        let engine = self.engine.read().unwrap().clone()?;
        let res = engine.url_cosmetic_resources(url);
        let script = if res.injected_script.is_empty() {
            String::new()
        } else {
            // (What uBO's scriptlets expect around them. Inside a function, for the page not to see them.)
            format!("(function(){{const scriptletGlobals={{}};\n{}\n}})();", res.injected_script)
        };
        Some(PagePlan {
            hide: res.hide_selectors.into_iter().collect(),
            procedural: res.procedural_actions.into_iter().collect(),
            script,
            generichide: res.generichide,
            exceptions: res.exceptions.into_iter().collect(),
        })
    }

    /// What generic rules hide among these classes and ids.
    pub fn generic_selectors(&self, classes: &[String], ids: &[String], exceptions: &[String]) -> Vec<String> {
        let Some(engine) = self.engine.read().unwrap().clone() else { return Vec::new() };
        let exceptions: std::collections::HashSet<String> = exceptions.iter().cloned().collect();
        engine.hidden_class_id_selectors(classes, ids, &exceptions)
    }
}


fn fetch(url: &str) -> anyhow::Result<String> {
    let mut resp = ureq::get(url).header("User-Agent", crate::webext::amo::USER_AGENT).call()?;
    Ok(resp.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with(rules: &str, trusted: bool) -> AdBlock {
        let ab = AdBlock::new(std::path::PathBuf::from("/nonexistent"));
        let mut set = FilterSet::new(false);
        set.add_filter_list(preprocess(rules), parse_options(trusted));
        let mut engine = Engine::new_with_filter_set(set);
        load_resources(&mut engine);
        *ab.engine.write().unwrap() = Some(Arc::new(engine));
        ab
    }

    #[test]
    fn conditions_like_ubo() {
        for yes in ["env_chromium", "!env_firefox", "ext_ublock && !ext_ubol", "env_firefox || env_chromium", "!(env_firefox || env_safari)", "cap_user_stylesheet", "!unknown_thing"] {
            assert!(condition_holds(yes), "{yes}");
        }
        for no in ["env_firefox", "env_safari", "cap_html_filtering", "ext_ubol", "!env_chromium", "env_chromium && env_mobile", "unknown_thing"] {
            assert!(!condition_holds(no), "{no}");
        }
    }

    #[test]
    fn directives_leave_out_what_is_for_others() {
        let list = "a\n!#if env_firefox\nb\n!#else\nc\n!#endif\n!#if env_chromium\nd\n!#if env_mobile\ne\n!#else\nf\n!#endif\n!#else\ng\n!#endif\n!#if env_safari\n!#if env_chromium\nh\n!#else\ni\n!#endif\n!#endif\nj\n";
        assert_eq!(preprocess(list), "a\nc\nd\nf\nj\n");
    }

    #[test]
    fn every_scriptlet_loads_with_what_it_depends_on() {
        #[derive(Deserialize)]
        struct Bundle {
            resources: Vec<Resource>,
        }
        let bundle: Bundle = serde_json::from_str(RESOURCES).unwrap();
        assert!(bundle.resources.len() > 150);
        let trusted = engine_with("example.org##+js(trusted-replace-xhr-response, a, b, c)\nexample.net##+js(json-prune-fetch-response, adPlacements)\nexample.com##+js(acs, x, y)\n", true);
        for (host, wants) in [("example.org", "function "), ("example.net", "adPlacements"), ("example.com", "abortCurrentScript")] {
            let plan = trusted.page_plan(&format!("https://{host}/")).unwrap();
            assert!(plan.script.contains(wants), "{host}: {}", &plan.script[..plan.script.len().min(300)]);
            assert!(plan.script.starts_with("(function(){const scriptletGlobals={};"), "wrapped");
        }
    }

    #[test]
    fn trusted_scriptlets_need_a_trusted_list() {
        let rule = "example.org##+js(trusted-replace-xhr-response, a, b, c)\n";
        assert!(!engine_with(rule, true).page_plan("https://example.org/").unwrap().script.is_empty());
        assert!(engine_with(rule, false).page_plan("https://example.org/").unwrap().script.is_empty());
        assert!(trusted_list("https://ublockorigin.github.io/uAssets/filters/filters.min.txt"));
        assert!(!trusted_list("https://easylist.to/easylist/easylist.txt"));
        assert!(!trusted_list("https://ublockorigin.github.io.evil.example/uAssets/x.txt"));
    }

    #[test]
    fn redirects_give_something_harmless_instead() {
        let ab = engine_with("||ads.example/a.js^$script,redirect=noopjs\n||ads.example/b.png^$image,redirect=1x1.gif\n", true);
        for (url, kind, wanted) in [("https://ads.example/a.js", "script", "data:application/javascript"), ("https://ads.example/b.png", "image", "data:image/gif")] {
            match ab.check(url, "https://news.test/", kind) {
                Verdict::Redirect(to) => assert!(to.starts_with(wanted), "{to}"),
                other => panic!("{url}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_page_gets_its_plan_and_generic_rules_on_asking() {
        let ab = engine_with("##.generic-ad\n###generic-id\n##div[data-ad]\nnews.test##.site-only\nnews.test##div:has-text(Sponsored)\nnews.test##.popup:style(color: red)\nkept.test#@#.generic-ad\nnews.test##+js(set, a, 1)\n", true);
        let plan = ab.page_plan("https://news.test/x").unwrap();
        assert!(plan.hide.contains(&".site-only".to_string()), "{:?}", plan.hide);
        assert!(plan.hide.contains(&"div[data-ad]".to_string()));
        assert!(plan.hide.contains(&"div:has-text(Sponsored)".to_string()));
        assert_eq!(plan.procedural.len(), 1, "{:?}", plan.procedural);
        assert!(plan.script.contains("setConstant"));
        assert_eq!(ab.generic_selectors(&["generic-ad".into(), "other".into()], &["generic-id".into()], &[]).len(), 2);
        let kept = ab.page_plan("https://kept.test/").unwrap();
        assert!(ab.generic_selectors(&["generic-ad".into()], &[], &kept.exceptions).is_empty(), "the exception holds");
    }

    #[test]
    fn nothing_for_a_page_blocking_is_off_for() {
        let ab = engine_with("news.test##.x\n", true);
        ab.set_allowlist(vec!["news.test".into()]);
        assert_eq!(ab.page_plan("https://news.test/"), None);
        let ab = engine_with("news.test##.x\n", true);
        ab.set_enabled(false);
        assert_eq!(ab.page_plan("https://news.test/"), None);
    }
    #[test]
    fn blocks_and_hides() {
        let dir = tempfile::tempdir().unwrap();
        let ab = AdBlock::new(dir.path().into());
        ab.rebuild(&[], "||ads.example.com^\nexample.org##.banner\n@@||ads.example.com/ok^", false).unwrap();
        assert_eq!(ab.check("https://ads.example.com/x.js", "https://news.test/", "script"), Verdict::Block);
        assert_eq!(ab.check("https://ads.example.com/ok/x.js", "https://news.test/", "script"), Verdict::Allow);
        assert_eq!(ab.check("https://ads.example.com/", "", "mainFrame"), Verdict::Allow);
        assert!(ab.page_plan("https://example.org/").unwrap().hide.contains(&".banner".to_string()));
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
