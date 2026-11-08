//! Dangerous-site protection ("browsing.safeBrowsing"): page loads and
//! downloads are checked against lists of phishing and malware addresses.
//!
//! * "open": the malware-filter project's phishing and malware (URLhaus)
//!   lists, downloaded once a day; checks are local.
//! * "google": Google Safe Browsing's Update API (v4) with the user's API
//!   key. Kite keeps the lists of 4-byte SHA-256 prefixes locally (updated as
//!   often as Google allows, at least every 30 minutes); only when an
//!   address's hash starts with one of them does Kite ask Google for the full
//!   hashes with that prefix, to confirm. Full addresses are never sent.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::webext::amo::sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off = 0,
    Google = 1,
    Open = 2,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s {
            "google" => Mode::Google,
            "open" => Mode::Open,
            _ => Mode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Threat {
    Phishing,
    Malware,
    Unwanted,
}

impl Threat {
    pub fn as_str(self) -> &'static str {
        match self {
            Threat::Phishing => "phishing",
            Threat::Malware => "malware",
            Threat::Unwanted => "unwanted",
        }
    }
    fn from_google(t: &str) -> Option<Threat> {
        match t {
            "SOCIAL_ENGINEERING" => Some(Threat::Phishing),
            "MALWARE" | "POTENTIALLY_HARMFUL_APPLICATION" => Some(Threat::Malware),
            "UNWANTED_SOFTWARE" => Some(Threat::Unwanted),
            _ => None,
        }
    }
}

// ---- open lists ---------------------------------------------------------------

pub const OPEN_LISTS: [(&str, Threat); 2] = [
    ("https://malware-filter.gitlab.io/malware-filter/phishing-filter.txt", Threat::Phishing),
    ("https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-online.txt", Threat::Malware),
];
const OPEN_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Entries of the open lists: whole hosts (and their subdomains), or
/// addresses under a host (`host/path…`).
#[derive(Default)]
pub struct OpenLists {
    hosts: HashMap<String, Threat>,
    paths: HashMap<String, Vec<(String, Threat)>>,
}

impl OpenLists {
    /// Add a list in the malware-filter format: comments start with `!`;
    /// entries are `host`, `host/path` or `||host/path^$all`.
    pub fn add(&mut self, text: &str, threat: Threat) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('!') || line.starts_with('#') {
                continue;
            }
            let entry = line.trim_start_matches("||");
            let entry = entry.split('$').next().unwrap_or("").trim_end_matches('^');
            if entry.is_empty() {
                continue;
            }
            match entry.split_once('/') {
                None => {
                    self.hosts.insert(entry.to_ascii_lowercase(), threat);
                }
                Some((host, path)) => self.paths.entry(host.to_ascii_lowercase()).or_default().push((format!("/{path}"), threat)),
            }
        }
    }

    pub fn len(&self) -> usize {
        self.hosts.len() + self.paths.values().map(Vec::len).sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn check(&self, url: &url::Url) -> Option<Threat> {
        let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
        // The host and its parent domains.
        let mut h = host.as_str();
        loop {
            if let Some(t) = self.hosts.get(h) {
                return Some(*t);
            }
            match h.split_once('.') {
                Some((_, parent)) if parent.contains('.') => h = parent,
                _ => break,
            }
        }
        let rest = match url.query() {
            Some(q) => format!("{}?{q}", url.path()),
            None => url.path().to_owned(),
        };
        self.paths.get(&host)?.iter().find(|(p, _)| rest.starts_with(p.as_str())).map(|(_, t)| *t)
    }
}

// ---- Google Safe Browsing (Update API v4) ------------------------------------

const GOOGLE_API: &str = "https://safebrowsing.googleapis.com/v4";
const GOOGLE_LISTS: [&str; 3] = ["MALWARE", "SOCIAL_ENGINEERING", "UNWANTED_SOFTWARE"];
const GOOGLE_MIN_WAIT: Duration = Duration::from_secs(30 * 60);

/// One of Google's lists: its client state and sorted hash prefixes.
#[derive(Default, Clone)]
struct PrefixList {
    state: String,
    prefixes: Vec<Vec<u8>>,
}

impl PrefixList {
    fn sizes(&self) -> Vec<usize> {
        let mut s: Vec<usize> = self.prefixes.iter().map(Vec::len).collect();
        s.sort_unstable();
        s.dedup();
        s
    }
    fn matching(&self, hash: &[u8; 32]) -> Vec<Vec<u8>> {
        let mut out = vec![];
        for n in self.sizes() {
            let p = &hash[..n];
            if self.prefixes.binary_search_by(|x| x.as_slice().cmp(p)).is_ok() {
                out.push(p.to_vec());
            }
        }
        out
    }
    /// Apply one list update from the API; false if its checksum didn't
    /// match (the list then starts over with a full update).
    fn apply(&mut self, update: &Value) -> bool {
        if update["responseType"].as_str() == Some("FULL_UPDATE") {
            self.prefixes.clear();
        }
        // Removals are indices into the list before this update's additions.
        let mut remove: Vec<usize> = vec![];
        for r in update["removals"].as_array().into_iter().flatten() {
            for i in r["rawIndices"]["indices"].as_array().into_iter().flatten() {
                if let Some(i) = i.as_u64() {
                    remove.push(i as usize);
                }
            }
        }
        if !remove.is_empty() {
            let remove: HashSet<usize> = remove.into_iter().collect();
            self.prefixes = std::mem::take(&mut self.prefixes).into_iter().enumerate().filter(|(i, _)| !remove.contains(i)).map(|(_, p)| p).collect();
        }
        for a in update["additions"].as_array().into_iter().flatten() {
            let size = a["rawHashes"]["prefixSize"].as_u64().unwrap_or(4) as usize;
            let Some(raw) = a["rawHashes"]["rawHashes"].as_str() else { continue };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(raw) else { continue };
            if size == 0 {
                continue;
            }
            self.prefixes.extend(bytes.chunks_exact(size).map(<[u8]>::to_vec));
        }
        self.prefixes.sort_unstable();
        self.prefixes.dedup();
        self.state = update["newClientState"].as_str().unwrap_or_default().to_owned();
        if let Some(expected) = update["checksum"]["sha256"].as_str() {
            let all: Vec<u8> = self.prefixes.concat();
            let got = base64::engine::general_purpose::STANDARD.encode(sha256(&all));
            if got != expected {
                log::warn!("safebrowsing: checksum mismatch, the list starts over");
                self.prefixes.clear();
                self.state.clear();
                return false;
            }
        }
        true
    }
}

/// Google's canonical form of an address: (host, path, query), as the
/// Safe Browsing API specifies (fragment dropped, repeatedly unescaped,
/// host lowercased without stray dots, path with `.`/`..` and `//` resolved).
pub fn canonicalize(url: &str) -> Option<(String, String, Option<String>)> {
    let cleaned: String = url.trim().chars().filter(|c| !matches!(c, '\t' | '\r' | '\n')).collect();
    let cleaned = cleaned.split('#').next().unwrap_or("");
    let mut unescaped = cleaned.to_owned();
    for _ in 0..10 {
        let next = percent_encoding::percent_decode_str(&unescaped).decode_utf8_lossy().into_owned();
        if next == unescaped {
            break;
        }
        unescaped = next;
    }
    let parsed = url::Url::parse(&unescaped).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let mut host = parsed.host_str()?.to_ascii_lowercase();
    while host.contains("..") {
        host = host.replace("..", ".");
    }
    let host = host.trim_matches('.').to_owned();
    let mut path = parsed.path().to_owned();
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    let escape = |s: &str| -> String {
        s.bytes()
            .map(|b| if b <= 32 || b >= 127 || b == b'#' || b == b'%' { format!("%{b:02X}") } else { (b as char).to_string() })
            .collect()
    };
    Some((escape(&host), escape(&path), parsed.query().map(escape)))
}

/// The host/path combinations of an address that Safe Browsing hashes: the
/// host and up to four parent domains, times the path with and without its
/// query and up to four of its leading directories.
pub fn expressions(url: &str) -> Vec<String> {
    let Some((host, path, query)) = canonicalize(url) else { return vec![] };
    let mut hosts = vec![host.clone()];
    if host.parse::<std::net::IpAddr>().is_err() && !host.starts_with('[') {
        let parts: Vec<&str> = host.split('.').collect();
        let start = parts.len().saturating_sub(5).max(1);
        for i in start..parts.len().saturating_sub(1) {
            hosts.push(parts[i..].join("."));
        }
    }
    let mut paths = vec![];
    if let Some(q) = &query {
        paths.push(format!("{path}?{q}"));
    }
    paths.push(path.clone());
    paths.push("/".to_owned());
    let dirs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let mut prefix = String::from("/");
    for d in dirs.iter().take(dirs.len().saturating_sub(1)).take(3) {
        prefix.push_str(d);
        prefix.push('/');
        paths.push(prefix.clone());
    }
    let mut seen = HashSet::new();
    let mut out = vec![];
    for h in &hosts {
        for p in &paths {
            let e = format!("{h}{p}");
            if seen.insert(e.clone()) {
                out.push(e);
            }
        }
    }
    out
}

#[derive(Default)]
struct GoogleDb {
    lists: HashMap<String, PrefixList>,
    /// When Google lets Kite ask for updates again.
    next_update: Option<SystemTime>,
}

#[derive(Default)]
struct Cache {
    /// Confirmed full hashes, until they expire.
    positive: HashMap<[u8; 32], (Threat, Instant)>,
    /// Prefixes Google said are clean, until they expire.
    negative: HashMap<Vec<u8>, Instant>,
}

// ---- the checker ----------------------------------------------------------------

pub struct SafeBrowsing {
    dir: PathBuf,
    mode: AtomicU8,
    key: RwLock<String>,
    open: RwLock<OpenLists>,
    google: Mutex<GoogleDb>,
    cache: Mutex<Cache>,
    /// Hosts the user chose to visit anyway this session.
    allowed: Mutex<HashSet<String>>,
    /// The last update's outcome, for Settings.
    status: Mutex<String>,
}

impl SafeBrowsing {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            mode: AtomicU8::new(0),
            key: RwLock::default(),
            open: RwLock::default(),
            google: Mutex::default(),
            cache: Mutex::default(),
            allowed: Mutex::default(),
            status: Mutex::new(String::new()),
        }
    }

    pub fn mode(&self) -> Mode {
        match self.mode.load(Relaxed) {
            1 => Mode::Google,
            2 => Mode::Open,
            _ => Mode::Off,
        }
    }

    pub fn configure(&self, mode: Mode, key: &str) {
        self.mode.store(mode as u8, Relaxed);
        *self.key.write().unwrap() = key.trim().to_owned();
    }

    pub fn status(&self) -> String {
        self.status.lock().unwrap().clone()
    }

    fn set_status(&self, s: String) {
        *self.status.lock().unwrap() = s;
    }

    /// Load the lists saved by earlier runs (fast; at startup).
    pub fn load_cached(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
        let mut open = OpenLists::default();
        for (url, threat) in OPEN_LISTS {
            if let Ok(text) = std::fs::read_to_string(crate::adblock::list_path(&self.dir, url)) {
                open.add(&text, threat);
            }
        }
        *self.open.write().unwrap() = open;
        if let Ok(text) = std::fs::read_to_string(self.dir.join("google.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                let mut db = self.google.lock().unwrap();
                for (name, l) in v["lists"].as_object().into_iter().flatten() {
                    let size = l["prefixSize"].as_u64().unwrap_or(4) as usize;
                    let bytes = base64::engine::general_purpose::STANDARD.decode(l["prefixes"].as_str().unwrap_or("")).unwrap_or_default();
                    let mut list = PrefixList { state: l["state"].as_str().unwrap_or("").to_owned(), prefixes: vec![] };
                    if size > 0 {
                        list.prefixes = bytes.chunks_exact(size).map(<[u8]>::to_vec).collect();
                    }
                    db.lists.insert(name.clone(), list);
                }
                db.next_update = v["nextUpdate"].as_u64().map(|s| UNIX_EPOCH + Duration::from_secs(s));
            }
        }
    }

    fn save_google(&self, db: &GoogleDb) {
        let mut lists = serde_json::Map::new();
        for (name, l) in &db.lists {
            // (Google's lists use one prefix size; longer prefixes are rare
            // and refetched with the next full update.)
            let size = l.prefixes.first().map(Vec::len).unwrap_or(4);
            let same: Vec<u8> = l.prefixes.iter().filter(|p| p.len() == size).flatten().copied().collect();
            lists.insert(name.clone(), json!({"state": l.state, "prefixSize": size, "prefixes": base64::engine::general_purpose::STANDARD.encode(same)}));
        }
        let next = db.next_update.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
        let _ = crate::util::write_atomic(&self.dir.join("google.json"), json!({"lists": lists, "nextUpdate": next}).to_string().as_bytes());
    }

    /// Bring the lists of the current mode up to date (blocking: on a worker
    /// thread). Returns how long until the next update is due.
    pub fn update(&self) -> Duration {
        match self.mode() {
            Mode::Off => Duration::from_secs(3600),
            Mode::Open => self.update_open(),
            Mode::Google => self.update_google(),
        }
    }

    fn update_open(&self) -> Duration {
        let mut open = OpenLists::default();
        let mut errors = vec![];
        for (url, threat) in OPEN_LISTS {
            let path = crate::adblock::list_path(&self.dir, url);
            let stale = std::fs::metadata(&path).and_then(|m| m.modified()).map(|t| t.elapsed().unwrap_or_default() > OPEN_MAX_AGE).unwrap_or(true);
            if stale {
                match fetch(url) {
                    Ok(body) => {
                        let _ = crate::util::write_atomic(&path, body.as_bytes());
                    }
                    Err(e) => errors.push(format!("{url}: {e}")),
                }
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                open.add(&text, threat);
            }
        }
        let n = open.len();
        *self.open.write().unwrap() = open;
        self.set_status(if errors.is_empty() { format!("{n} dangerous addresses known") } else { format!("Update failed: {}", errors.join("; ")) });
        Duration::from_secs(3600)
    }

    fn update_google(&self) -> Duration {
        let key = self.key.read().unwrap().clone();
        if key.is_empty() {
            self.set_status("An API key is needed".into());
            return Duration::from_secs(3600);
        }
        {
            let db = self.google.lock().unwrap();
            if let Some(t) = db.next_update {
                if let Ok(wait) = t.duration_since(SystemTime::now()) {
                    return wait;
                }
            }
        }
        let states: Vec<(String, String)> = {
            let db = self.google.lock().unwrap();
            GOOGLE_LISTS.iter().map(|l| (l.to_string(), db.lists.get(*l).map(|x| x.state.clone()).unwrap_or_default())).collect()
        };
        let body = json!({
            "client": {"clientId": "kite", "clientVersion": env!("CARGO_PKG_VERSION")},
            "listUpdateRequests": states.iter().map(|(l, state)| json!({
                "threatType": l, "platformType": "ANY_PLATFORM", "threatEntryType": "URL", "state": state,
                "constraints": {"supportedCompressions": ["RAW"]},
            })).collect::<Vec<_>>(),
        });
        let reply = post(&format!("{GOOGLE_API}/threatListUpdates:fetch?key={key}"), &body, Duration::from_secs(30));
        let mut db = self.google.lock().unwrap();
        let wait = match reply {
            Ok(v) => {
                for u in v["listUpdateResponses"].as_array().into_iter().flatten() {
                    let name = u["threatType"].as_str().unwrap_or("").to_owned();
                    db.lists.entry(name).or_default().apply(u);
                }
                let wait = parse_duration(v["minimumWaitDuration"].as_str()).unwrap_or(GOOGLE_MIN_WAIT).max(GOOGLE_MIN_WAIT);
                let n: usize = db.lists.values().map(|l| l.prefixes.len()).sum();
                self.set_status(format!("{n} hash prefixes"));
                wait
            }
            Err(e) => {
                self.set_status(format!("Update failed: {e}"));
                GOOGLE_MIN_WAIT
            }
        };
        db.next_update = Some(SystemTime::now() + wait);
        self.save_google(&db);
        wait
    }

    pub fn allow(&self, host: &str) {
        self.allowed.lock().unwrap().insert(host.to_ascii_lowercase());
    }

    /// The threat a page load or download of `url` is known for, if any.
    /// (Google mode asks Google only when a hash prefix matches; that call
    /// waits at most 1.5 s.)
    pub fn check(&self, url: &str) -> Option<Threat> {
        let mode = self.mode();
        if mode == Mode::Off || !(url.starts_with("http:") || url.starts_with("https:")) {
            return None;
        }
        let parsed = url::Url::parse(url).ok()?;
        if self.allowed.lock().unwrap().contains(&parsed.host_str()?.to_ascii_lowercase()) {
            return None;
        }
        match mode {
            Mode::Open => self.open.read().unwrap().check(&parsed),
            Mode::Google => self.check_google(url),
            Mode::Off => None,
        }
    }

    fn check_google(&self, url: &str) -> Option<Threat> {
        let hashes: Vec<[u8; 32]> = expressions(url).iter().map(|e| sha256(e.as_bytes())).collect();
        // Local prefix matches, and what the cache already knows.
        let mut ask: Vec<Vec<u8>> = vec![];
        {
            let db = self.google.lock().unwrap();
            let mut cache = self.cache.lock().unwrap();
            let now = Instant::now();
            cache.positive.retain(|_, (_, until)| *until > now);
            cache.negative.retain(|_, until| *until > now);
            for h in &hashes {
                if let Some((t, _)) = cache.positive.get(h) {
                    return Some(*t);
                }
                for l in db.lists.values() {
                    for p in l.matching(h) {
                        if !cache.negative.contains_key(&p) && !ask.contains(&p) {
                            ask.push(p);
                        }
                    }
                }
            }
        }
        if ask.is_empty() {
            return None;
        }
        let key = self.key.read().unwrap().clone();
        let states: Vec<String> = self.google.lock().unwrap().lists.values().map(|l| l.state.clone()).collect();
        let body = json!({
            "client": {"clientId": "kite", "clientVersion": env!("CARGO_PKG_VERSION")},
            "clientStates": states,
            "threatInfo": {
                "threatTypes": GOOGLE_LISTS, "platformTypes": ["ANY_PLATFORM"], "threatEntryTypes": ["URL"],
                "threatEntries": ask.iter().map(|p| json!({"hash": base64::engine::general_purpose::STANDARD.encode(p)})).collect::<Vec<_>>(),
            },
        });
        let reply = match post(&format!("{GOOGLE_API}/fullHashes:find?key={key}"), &body, Duration::from_millis(1500)) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("safebrowsing: full hash lookup failed: {e}");
                return None;
            }
        };
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        let mut found = None;
        for m in reply["matches"].as_array().into_iter().flatten() {
            let Some(threat) = Threat::from_google(m["threatType"].as_str().unwrap_or("")) else { continue };
            let Ok(full) = base64::engine::general_purpose::STANDARD.decode(m["threat"]["hash"].as_str().unwrap_or("")) else { continue };
            let Ok(full): Result<[u8; 32], _> = full.try_into() else { continue };
            let ttl = parse_duration(m["cacheDuration"].as_str()).unwrap_or(Duration::from_secs(300));
            cache.positive.insert(full, (threat, now + ttl));
            if hashes.contains(&full) && found.is_none() {
                found = Some(threat);
            }
        }
        if found.is_none() {
            let ttl = parse_duration(reply["negativeCacheDuration"].as_str()).unwrap_or(Duration::from_secs(300));
            for p in ask {
                cache.negative.insert(p, now + ttl);
            }
        }
        found
    }
}

/// Google's durations: "1800.5s". At most a day: a reply's "inf", NaN or a
/// huge number would panic in Duration or overflow `Instant + ttl`.
fn parse_duration(s: Option<&str>) -> Option<Duration> {
    let secs: f64 = s?.trim_end_matches('s').parse().ok()?;
    if !secs.is_finite() {
        return None;
    }
    Some(Duration::from_secs_f64(secs.clamp(0.0, 86_400.0)))
}

fn fetch(url: &str) -> anyhow::Result<String> {
    let mut resp = ureq::get(url).header("User-Agent", "Kite").call()?;
    Ok(resp.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string()?)
}

fn post(url: &str, body: &Value, timeout: Duration) -> anyhow::Result<Value> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into();
    let mut resp = agent.post(url).header("Content-Type", "application/json").send(body.to_string())?;
    let status = resp.status();
    let text = resp.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string()?;
    if !status.is_success() {
        let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_owned)).unwrap_or_else(|| status.to_string());
        anyhow::bail!("{msg}");
    }
    Ok(serde_json::from_str(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_lists_match_hosts_and_paths() {
        let mut l = OpenLists::default();
        l.add("! Title: test\nevil.example\n1.2.3.4\n||files.example.org/bad/tool.exe^$all\n", Threat::Malware);
        l.add("phish.test\n", Threat::Phishing);
        let u = |s: &str| url::Url::parse(s).unwrap();
        assert_eq!(l.check(&u("https://evil.example/login")), Some(Threat::Malware));
        assert_eq!(l.check(&u("https://www.evil.example/")), Some(Threat::Malware));
        assert_eq!(l.check(&u("http://1.2.3.4/x")), Some(Threat::Malware));
        assert_eq!(l.check(&u("https://files.example.org/bad/tool.exe")), Some(Threat::Malware));
        assert_eq!(l.check(&u("https://files.example.org/good.exe")), None);
        assert_eq!(l.check(&u("https://phish.test/")), Some(Threat::Phishing));
        assert_eq!(l.check(&u("https://example.org/")), None);
        assert_eq!(l.len(), 4);
    }

    // Examples from Google's Safe Browsing API documentation.
    #[test]
    fn canonical_forms() {
        let c = |u: &str| canonicalize(u).map(|(h, p, q)| format!("{h}{p}{}", q.map(|q| format!("?{q}")).unwrap_or_default()));
        assert_eq!(c("http://host/%25%32%35").as_deref(), Some("host/%25"));
        assert_eq!(c("http://host/%25%32%35%25%32%35").as_deref(), Some("host/%25%25"));
        assert_eq!(c("http://www.google.com/blah/..").as_deref(), Some("www.google.com/"));
        assert_eq!(c("http://www.GOOgle.com/").as_deref(), Some("www.google.com/"));
        assert_eq!(c("http://www.google.com.../").as_deref(), Some("www.google.com/"));
        assert_eq!(c("http://www.google.com/foo\tbar\rbaz\n2").as_deref(), Some("www.google.com/foobarbaz2"));
        assert_eq!(c("http://www.google.com/q?r?").as_deref(), Some("www.google.com/q?r?"));
        assert_eq!(c("http://www.evil.com/blah#frag").as_deref(), Some("www.evil.com/blah"));
        assert_eq!(c("http://3279880203/blah").as_deref(), Some("195.127.0.11/blah"));
        assert_eq!(c("http://host.com//twoslashes?more//slashes").as_deref(), Some("host.com/twoslashes?more//slashes"));
    }

    #[test]
    fn expressions_like_the_spec() {
        let e = expressions("http://a.b.c/1/2.html?param=1");
        for want in ["a.b.c/1/2.html?param=1", "a.b.c/1/2.html", "a.b.c/", "a.b.c/1/", "b.c/1/2.html?param=1", "b.c/1/2.html", "b.c/", "b.c/1/"] {
            assert!(e.contains(&want.to_owned()), "{want} missing in {e:?}");
        }
        assert_eq!(e.len(), 8);
        let e = expressions("http://a.b.c.d.e.f.g/1.html");
        assert!(e.contains(&"a.b.c.d.e.f.g/1.html".to_owned()) && e.contains(&"c.d.e.f.g/1.html".to_owned()) && !e.contains(&"b.c.d.e.f.g/1.html".to_owned()));
        assert!(expressions("http://1.2.3.4/1/").contains(&"1.2.3.4/1/".to_owned()));
    }

    #[test]
    fn prefix_updates_removals_and_checksum() {
        let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
        let mut l = PrefixList::default();
        let raw: Vec<u8> = [&b"aaaa"[..], b"cccc", b"bbbb"].concat();
        let sorted: Vec<u8> = [&b"aaaa"[..], b"bbbb", b"cccc"].concat();
        assert!(l.apply(&json!({"responseType": "FULL_UPDATE", "additions": [{"compressionType": "RAW", "rawHashes": {"prefixSize": 4, "rawHashes": b64(&raw)}}], "newClientState": "s1", "checksum": {"sha256": b64(&sha256(&sorted))}})));
        assert_eq!(l.prefixes.len(), 3);
        // Remove "bbbb" (index 1), add "dddd".
        let after: Vec<u8> = [&b"aaaa"[..], b"cccc", b"dddd"].concat();
        assert!(l.apply(&json!({"responseType": "PARTIAL_UPDATE", "removals": [{"rawIndices": {"indices": [1]}}], "additions": [{"rawHashes": {"prefixSize": 4, "rawHashes": b64(b"dddd")}}], "newClientState": "s2", "checksum": {"sha256": b64(&sha256(&after))}})));
        assert_eq!(l.prefixes, vec![b"aaaa".to_vec(), b"cccc".to_vec(), b"dddd".to_vec()]);
        assert_eq!(l.state, "s2");
        let mut h = [0u8; 32];
        h[..4].copy_from_slice(b"cccc");
        assert_eq!(l.matching(&h), vec![b"cccc".to_vec()]);
        // A wrong checksum: the list starts over.
        assert!(!l.apply(&json!({"responseType": "PARTIAL_UPDATE", "newClientState": "s3", "checksum": {"sha256": b64(&[0u8; 32])}})));
        assert!(l.prefixes.is_empty() && l.state.is_empty());
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration(Some("1800s")), Some(Duration::from_secs(1800)));
        assert_eq!(parse_duration(Some("300.5s")), Some(Duration::from_secs_f64(300.5)));
        assert_eq!(parse_duration(None), None);
        assert_eq!(parse_duration(Some("infs")), None);
        assert_eq!(parse_duration(Some("NaNs")), None);
        assert_eq!(parse_duration(Some("1e300s")), Some(Duration::from_secs(86_400)));
        assert_eq!(parse_duration(Some("-5s")), Some(Duration::ZERO));
    }
}
