//! Parental controls: which profiles are a child's, the parent password, and what a
//! child's profile may do and see.
//!
//! `parental.json` beside the profile directories holds all of it, as one Kite decides
//! about another's profile (the picker, a start with `--profile`). The site filter's
//! lists are in `sitefilter` there, shared by the child profiles.
//!
//! What this is and isn't: it keeps a child inside their profile in Kite and keeps
//! listed sites away from it. The file is the user's own, like the rest of Kite's data:
//! someone who can change files in the home folder, or start another browser, gets
//! around it. A separate system account for the child is what holds against that.
//!
//! Against the easy ways (changing the file, removing it, emptying a list) there is a
//! seal: a hash of the file's state, made like the password's with its salt reversed,
//! and a second copy of the file to put back from. `check` looks once, as a profile
//! loads. It takes no secret to make, so it stops a child, not someone who reads this.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

const FILE: &str = "parental.json";
/// The second copy, to put `FILE` back from. (Among the profiles' folders, away from it.)
const COPY: &str = "profiles/.parental";
const SEAL: &str = "seal";
/// The variable a Kite that may open any profile hands a pass in, to the Kite it starts.
pub const PASS_VARIABLE: &str = "KITE_PARENT_PASS";
const PASS_MAX_AGE: Duration = Duration::from_secs(300);
pub const MIN_PASSWORD: usize = 4;

/// What a child's profile is held to. The defaults are what a new one starts with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Restrictions {
    /// Sites on the lists of adult sites don't open.
    pub block_adult: bool,
    /// The lists used for that (`LISTS` ids).
    pub lists: Vec<String>,
    /// Search engines are held to their safe search; ones that can't be are blocked.
    pub safe_search: bool,
    /// Sites the parent blocked, and ones always allowed, with all under them.
    pub blocked: Vec<String>,
    pub allowed: Vec<String>,
    /// Nothing but the allowed sites opens.
    pub only_allowed: bool,
    /// What the child may do.
    pub extensions: bool,
    pub plugins: bool,
    pub settings: bool,
    pub private_windows: bool,
    pub clear_history: bool,
}

impl Default for Restrictions {
    fn default() -> Self {
        Self {
            block_adult: true,
            lists: LISTS.iter().filter(|l| l.default).map(|l| l.id.to_owned()).collect(),
            safe_search: true,
            blocked: vec![],
            allowed: vec![],
            only_allowed: false,
            extensions: false,
            plugins: false,
            settings: false,
            private_windows: false,
            clear_history: false,
        }
    }
}

/// Something a child's profile may or may not do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Permission {
    Extensions,
    Plugins,
    Settings,
    PrivateWindows,
    ClearHistory,
}

impl Restrictions {
    pub fn allows(&self, what: Permission) -> bool {
        match what {
            Permission::Extensions => self.extensions,
            Permission::Plugins => self.plugins,
            Permission::Settings => self.settings,
            Permission::PrivateWindows => self.private_windows,
            Permission::ClearHistory => self.clear_history,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct PasswordHash {
    salt: String,
    hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
struct Stored {
    password: Option<PasswordHash>,
    /// The password is asked before a profile without restrictions opens.
    lock_unrestricted: bool,
    children: BTreeMap<String, Restrictions>,
    /// Wrong passwords in a row, and when the last one was (ms), to slow guessing down.
    failures: u32,
    last_failure: i64,
    /// The site filter's files as Kite wrote them: name to SHA-256.
    files: BTreeMap<String, String>,
    /// When the file was last put back from its copy (ms), until the parent has seen it.
    restored: i64,
}

impl Default for Stored {
    fn default() -> Self {
        Self { password: None, lock_unrestricted: true, children: BTreeMap::new(), failures: 0, last_failure: 0, files: BTreeMap::new(), restored: 0 }
    }
}

pub struct Parental {
    base: PathBuf,
}

fn b64(v: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(v)
}

/// Argon2id with OWASP's smallest recommended cost: the check runs where the user waits.
fn hash_password(password: &str, salt: &[u8]) -> anyhow::Result<String> {
    let params = argon2::Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut out = [0u8; 32];
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| anyhow::anyhow!("hashing the password failed: {e}"))?;
    Ok(b64(&out))
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

/// The seal of a state (the file's JSON without its seal): hashed as the password is,
/// with the password's salt reversed.
fn seal_of(state: &serde_json::Value, salt: &str) -> Option<String> {
    let mut salt = unb64(salt)?;
    salt.reverse();
    hash_password(&serde_json::to_string(state).ok()?, &salt).ok()
}

/// What a file of parental controls is worth.
#[derive(Debug, PartialEq)]
enum Trust {
    /// Not there, or nothing to protect in it: no password.
    Nothing,
    /// As Kite wrote it.
    Sealed,
    /// It has a password, and isn't as Kite wrote it.
    Changed,
}

fn trust(text: Option<&str>) -> Trust {
    let Some(mut state) = text.and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()).filter(|v| v.is_object()) else { return Trust::Nothing };
    // (Leaving the rest in the order it's written in.)
    let seal = state.as_object_mut().and_then(|o| o.shift_remove(SEAL));
    let Some(salt) = state["password"]["salt"].as_str().map(str::to_owned) else { return Trust::Nothing };
    match (seal.as_ref().and_then(|s| s.as_str()), seal_of(&state, &salt)) {
        (Some(there), Some(right)) if there == right => Trust::Sealed,
        _ => Trust::Changed,
    }
}

/// What `Parental::check` found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Integrity {
    /// As Kite left it (or there are no parental controls).
    Fine,
    /// The file was changed or removed outside Kite, and put back from its copy.
    Restored,
    /// Changed outside Kite, with nothing to put back from: it takes the parent
    /// password to go on with it (`accept`).
    Changed,
}

/// How long to wait after this many wrong passwords in a row.
fn wait_after(failures: u32) -> Duration {
    match failures {
        0..=2 => Duration::ZERO,
        n => Duration::from_secs((5u64 << (n - 3).min(6)).min(300)),
    }
}

impl Parental {
    /// `base`: what holds the `profiles` directory (`paths::base_dir`).
    pub fn at(base: &Path) -> Self {
        Self { base: base.to_owned() }
    }

    pub fn open() -> anyhow::Result<Self> {
        Ok(Self::at(&crate::paths::base_dir()?))
    }

    fn load(&self) -> Stored {
        match std::fs::read_to_string(self.base.join(FILE)) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("parental controls: {FILE} can't be read ({e}); going on without");
                Stored::default()
            }),
            Err(_) => Stored::default(),
        }
    }

    fn write(path: &Path, text: &str) -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        crate::util::write_atomic(path, text.as_bytes())?;
        // (The password's hash is nobody else's to try passwords against.)
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        Ok(())
    }

    /// Writes the file with its seal, and its copy. A file that was changed outside
    /// Kite stays without a seal, whatever is written to it, until `accept`.
    fn save(&self, s: &Stored) -> anyhow::Result<()> {
        let changed = trust(std::fs::read_to_string(self.base.join(FILE)).ok().as_deref()) == Trust::Changed;
        self.save_sealed(s, !changed)
    }

    fn save_sealed(&self, s: &Stored, seal: bool) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.base)?;
        let mut state = serde_json::to_value(s)?;
        let sealed = match s.password.as_ref().filter(|_| seal).and_then(|p| seal_of(&state, &p.salt)) {
            Some(seal) => {
                state[SEAL] = seal.into();
                true
            }
            None => false,
        };
        let text = serde_json::to_string_pretty(&state)?;
        Self::write(&self.base.join(FILE), &text)?;
        if sealed {
            if let Err(e) = Self::write(&self.base.join(COPY), &text) {
                log::warn!("parental controls: no second copy: {e}");
            }
        }
        Ok(())
    }

    /// Looks whether the file is as Kite left it, and puts it back from its copy if it
    /// isn't. Once, as a profile loads: it hashes like a password check does.
    pub fn check(&self) -> Integrity {
        let path = self.base.join(FILE);
        let text = std::fs::read_to_string(&path).ok();
        let copy = std::fs::read_to_string(self.base.join(COPY)).ok();
        match trust(text.as_deref()) {
            Trust::Sealed => {
                if copy != text {
                    let _ = Self::write(&self.base.join(COPY), text.as_deref().unwrap_or_default());
                }
                Integrity::Fine
            }
            now => match copy.as_deref().filter(|c| trust(Some(c)) == Trust::Sealed).and_then(|c| serde_json::from_str::<Stored>(c).ok()) {
                Some(mut kept) => {
                    log::warn!("parental controls: {FILE} was changed or removed outside Kite; put back");
                    kept.restored = crate::util::now_ms();
                    match self.save_sealed(&kept, true) {
                        Ok(()) => Integrity::Restored,
                        Err(e) => {
                            log::warn!("parental controls: can't put {FILE} back: {e}");
                            Integrity::Changed
                        }
                    }
                }
                None if now == Trust::Changed => {
                    log::warn!("parental controls: {FILE} was changed outside Kite");
                    Integrity::Changed
                }
                None => Integrity::Fine,
            },
        }
    }

    /// The parent gave the password for a file changed outside Kite: it counts as it is.
    pub fn accept(&self) -> anyhow::Result<()> {
        self.save_sealed(&self.load(), true)
    }

    /// When the file was last put back from its copy (ms), 0 if the parent knows.
    pub fn restored(&self) -> i64 {
        self.load().restored
    }

    pub fn dismiss_restored(&self) -> anyhow::Result<()> {
        let mut s = self.load();
        s.restored = 0;
        self.save(&s)
    }

    /// The site filter's files as Kite wrote them (name to SHA-256). None without a
    /// password: nothing is held to anything then.
    fn files(&self) -> Option<BTreeMap<String, String>> {
        let s = self.load();
        s.password.is_some().then_some(s.files)
    }

    fn record_files(&self, changes: Vec<(String, Option<String>)>) {
        let mut s = self.load();
        if s.password.is_none() {
            return;
        }
        let before = s.files.clone();
        for (name, digest) in changes {
            match digest {
                Some(d) => s.files.insert(name, d),
                None => s.files.remove(&name),
            };
        }
        if s.files != before {
            if let Err(e) = self.save(&s) {
                log::warn!("parental controls: can't note the lists' state: {e}");
            }
        }
    }

    /// Where the site filter keeps its lists.
    pub fn filter_dir(&self) -> PathBuf {
        self.base.join("sitefilter")
    }

    pub fn has_password(&self) -> bool {
        self.load().password.is_some()
    }

    /// Sets the parent password. One that's set has to be given to change it.
    pub fn set_password(&self, current: &str, new: &str) -> anyhow::Result<()> {
        if self.has_password() {
            self.verify(current)?;
        }
        if new.chars().count() < MIN_PASSWORD {
            anyhow::bail!("The password needs at least {MIN_PASSWORD} characters.");
        }
        let salt: [u8; 16] = crate::passwords::random();
        let mut s = self.load();
        s.password = Some(PasswordHash { salt: b64(&salt), hash: hash_password(new, &salt)? });
        s.failures = 0;
        self.save(&s)
    }

    /// Ok if that's the parent password. After three wrong ones in a row there's a wait
    /// before the next is looked at, growing with each (kept in the file, so that
    /// starting Kite again doesn't shorten it).
    pub fn verify(&self, password: &str) -> anyhow::Result<()> {
        let mut s = self.load();
        let Some(stored) = s.password.clone() else { anyhow::bail!("No parent password is set.") };
        let since = Duration::from_millis((crate::util::now_ms() - s.last_failure).max(0) as u64);
        let wait = wait_after(s.failures);
        if since < wait {
            anyhow::bail!("Too many wrong passwords. Try again in {} seconds.", (wait - since).as_secs() + 1);
        }
        let salt = unb64(&stored.salt).ok_or_else(|| anyhow::anyhow!("The stored password can't be read."))?;
        let given = hash_password(password, &salt)?;
        // (Compared in full, whatever differs: nothing to time.)
        let same = given.len() == stored.hash.len() && given.bytes().zip(stored.hash.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0;
        if same {
            if s.failures != 0 {
                s.failures = 0;
                let _ = self.save(&s);
            }
            return Ok(());
        }
        s.failures += 1;
        s.last_failure = crate::util::now_ms();
        let _ = self.save(&s);
        match wait_after(s.failures) {
            w if w.is_zero() => anyhow::bail!("That isn't the parent password."),
            w => anyhow::bail!("That isn't the parent password. Try again in {} seconds.", w.as_secs()),
        }
    }

    pub fn is_child(&self, profile: &str) -> bool {
        self.load().children.contains_key(profile)
    }

    pub fn children(&self) -> BTreeMap<String, Restrictions> {
        self.load().children
    }

    pub fn restrictions(&self, profile: &str) -> Option<Restrictions> {
        self.load().children.remove(profile)
    }

    /// Makes the profile a child's (or changes what it's held to); None makes it an
    /// ordinary one again. The default profile stays the parent's.
    pub fn set_child(&self, profile: &str, restrictions: Option<Restrictions>) -> anyhow::Result<()> {
        crate::profiles::validate(profile)?;
        let mut s = self.load();
        match restrictions {
            Some(_) if profile == crate::profiles::Profiles::at(&self.base).default_name() => anyhow::bail!("The default profile can't be a child's profile."),
            Some(_) if s.password.is_none() => anyhow::bail!("A child's profile needs a parent password first."),
            Some(mut r) => {
                r.blocked = clean_sites(&r.blocked);
                r.allowed = clean_sites(&r.allowed);
                r.lists.retain(|id| LISTS.iter().any(|l| l.id == id));
                s.children.insert(profile.to_owned(), r);
            }
            None => {
                s.children.remove(profile);
            }
        }
        self.save(&s)
    }

    /// A profile got a new name: what it's held to goes with it.
    pub fn renamed(&self, from: &str, to: &str) {
        let mut s = self.load();
        if let Some(r) = s.children.remove(from) {
            s.children.insert(to.to_owned(), r);
            let _ = self.save(&s);
        }
    }

    pub fn lock_unrestricted(&self) -> bool {
        self.load().lock_unrestricted
    }

    pub fn set_lock_unrestricted(&self, on: bool) -> anyhow::Result<()> {
        let mut s = self.load();
        s.lock_unrestricted = on;
        self.save(&s)
    }

    /// Whether opening this profile (None: a private session, which is held to nothing)
    /// takes the parent password.
    pub fn needs_password(&self, profile: Option<&str>) -> bool {
        let s = self.load();
        s.password.is_some() && s.lock_unrestricted && !profile.is_some_and(|p| s.children.contains_key(p))
    }

    /// Whether managing profiles (new, rename, delete) takes the parent password: once
    /// there is a child's profile, so that it can't be deleted or renamed away.
    pub fn guards_profiles(&self) -> bool {
        let s = self.load();
        s.password.is_some() && !s.children.is_empty()
    }

    fn pass_dir(&self) -> PathBuf {
        self.base.join("parent-pass")
    }

    /// A pass for the Kite about to be started: it opens a profile without the password
    /// being asked again. Good once, and not for long.
    pub fn grant_pass(&self) -> Option<String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = self.pass_dir();
        std::fs::create_dir_all(&dir).ok()?;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        // (Ones never used go.)
        for old in std::fs::read_dir(&dir).ok()?.flatten() {
            if !pass_is_fresh(&old.path()) {
                let _ = std::fs::remove_file(old.path());
            }
        }
        let token: String = crate::passwords::random::<16>().iter().map(|b| format!("{b:02x}")).collect();
        std::fs::write(dir.join(&token), b"").ok()?;
        Some(token)
    }

    fn pass_path(&self, token: &str) -> Option<PathBuf> {
        (token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit())).then(|| self.pass_dir().join(token))
    }

    /// Whether that's a pass that still counts. It stays (the profile picker looks, the
    /// Kite it starts uses it).
    pub fn pass_is_good(&self, token: &str) -> bool {
        self.pass_path(token).is_some_and(|p| pass_is_fresh(&p))
    }

    /// Uses the pass up.
    pub fn redeem_pass(&self, token: &str) -> bool {
        let Some(path) = self.pass_path(token) else { return false };
        let good = pass_is_fresh(&path);
        let _ = std::fs::remove_file(path);
        good
    }
}

fn pass_is_fresh(path: &Path) -> bool {
    std::fs::metadata(path).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age <= PASS_MAX_AGE))
}

/// A site as typed into a list ("https://www.Example.com/page", "example.com") as the
/// name the filter compares: "example.com". None for what isn't one.
pub fn site_name(text: &str) -> Option<String> {
    let t = text.trim().to_lowercase();
    let t = t.split_once("://").map_or(t.as_str(), |(_, rest)| rest);
    let t = t.split(['/', '?', '#']).next().unwrap_or("");
    let t = t.rsplit_once('@').map_or(t, |(_, host)| host);
    let t = t.split(':').next().unwrap_or("").trim_matches('.');
    let t = t.strip_prefix("www.").unwrap_or(t);
    let ok = !t.is_empty() && t.len() <= 253 && t.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_') && !t.contains("..");
    ok.then(|| t.to_owned())
}

fn clean_sites(list: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    list.iter().filter_map(|s| site_name(s)).filter(|s| seen.insert(s.clone())).collect()
}

// ---- the lists of adult sites ------------------------------------------------

pub struct ListInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub home: &'static str,
    /// The list itself, or with `index`, the JSON file naming where it is now.
    pub url: &'static str,
    pub index: bool,
    pub default: bool,
}

/// Lists of adult sites, kept by their communities. One name to a line, or a hosts file.
pub const LISTS: [ListInfo; 5] = [
    ListInfo {
        id: "stevenblack",
        name: "StevenBlack's adult sites",
        about: "About 77,000 sites, gathered from several curated lists.",
        home: "https://github.com/StevenBlack/hosts",
        url: "https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/porn-only/hosts",
        index: false,
        default: true,
    },
    ListInfo {
        id: "antiporn-hosts",
        name: "Anti-Porn HOSTS File",
        about: "About 70,000 sites, from the NoFap community (4skinSkywalker).",
        home: "https://github.com/4skinSkywalker/Anti-Porn-HOSTS-File",
        url: "https://raw.githubusercontent.com/4skinSkywalker/Anti-Porn-HOSTS-File/master/HOSTS.txt",
        index: false,
        default: true,
    },
    ListInfo {
        id: "oisd-nsfw",
        name: "OISD NSFW",
        about: "About 500,000 sites, updated every day and made to break little.",
        home: "https://oisd.nl",
        url: "https://nsfw.oisd.nl/domainswild2",
        index: false,
        default: true,
    },
    ListInfo {
        id: "blocklistproject",
        name: "The Block List Project: Porn",
        about: "About 950,000 sites. Large, and updated less often.",
        home: "https://github.com/blocklistproject/Lists",
        url: "https://blocklistproject.github.io/Lists/alt-version/porn-nl.txt",
        index: false,
        default: false,
    },
    ListInfo {
        id: "bon-appetit",
        name: "Bon Appétit porn domains",
        about: "About 440,000 sites, community-maintained and updated every day.",
        home: "https://github.com/Bon-Appetit/porn-domains",
        url: "https://raw.githubusercontent.com/Bon-Appetit/porn-domains/main/meta.json",
        index: true,
        default: false,
    },
];

/// Lists are fetched again when older than this.
const LIST_MAX_AGE: Duration = Duration::from_secs(2 * 24 * 3600);
const LIST_MAX_SIZE: u64 = 96 * 1024 * 1024;
const CACHE_MAGIC: &[u8] = b"KITESF1\n";
/// The compiled filter's file.
const CACHE: &str = "filter.bin";

/// FNV-1a: the same on every run, as the compiled filter is kept in a file.
fn hash_name(name: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The site names of a list: a hosts file ("0.0.0.0 name"), names one to a line, or
/// blocker rules of the simplest kind ("||name^").
pub fn parse_list(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().filter_map(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
            return None;
        }
        let name = line.split_whitespace().last()?;
        let name = name.strip_prefix("||").unwrap_or(name).trim_end_matches('^').trim_start_matches("*.");
        // (A hosts file's own lines: "127.0.0.1 localhost" and such have no dot or are addresses.)
        if !name.contains('.') || name.parse::<std::net::IpAddr>().is_ok() || name == "localhost.localdomain" {
            return None;
        }
        site_name(name)
    })
}

/// Search engines that can't be held to safe search from outside: blocked in its place.
/// (With all under them; "yandex" stands for its many country addresses.)
const SEARCH_WITHOUT_SAFE_SEARCH: [&str; 16] = [
    "startpage.com",
    "search.brave.com",
    "qwant.com",
    "search.yahoo.com",
    "baidu.com",
    "yep.com",
    "mojeek.com",
    "metager.org",
    "metager.de",
    "presearch.com",
    "you.com",
    "dogpile.com",
    "ask.com",
    "search.aol.com",
    "kagi.com",
    "searx.space",
];

/// The addresses search engines keep for safe search that can't be switched off, as
/// Chromium's `--host-resolver-rules`: their own way of enforcing it on a network.
/// (Google's and YouTube's country addresses are covered by the patterns.)
pub fn safe_search_resolver_rules() -> String {
    const MAPS: [(&str, &str); 17] = [
        ("www.google.*", "forcesafesearch.google.com"),
        ("google.*", "forcesafesearch.google.com"),
        ("www.youtube.com", "restrict.youtube.com"),
        ("m.youtube.com", "restrict.youtube.com"),
        ("youtube.com", "restrict.youtube.com"),
        ("youtubei.googleapis.com", "restrict.youtube.com"),
        ("youtube.googleapis.com", "restrict.youtube.com"),
        ("www.youtube-nocookie.com", "restrict.youtube.com"),
        ("www.bing.com", "strict.bing.com"),
        ("bing.com", "strict.bing.com"),
        ("duckduckgo.com", "safe.duckduckgo.com"),
        ("www.duckduckgo.com", "safe.duckduckgo.com"),
        ("start.duckduckgo.com", "safe.duckduckgo.com"),
        ("html.duckduckgo.com", "safe.duckduckgo.com"),
        ("lite.duckduckgo.com", "safe.duckduckgo.com"),
        ("www.ecosia.org", "strict-safe-search.ecosia.org"),
        ("ecosia.org", "strict-safe-search.ecosia.org"),
    ];
    MAPS.iter().map(|(from, to)| format!("MAP {from} {to}")).collect::<Vec<_>>().join(", ")
}

/// Why a site doesn't open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Blocked {
    /// On a list of adult sites.
    Adult,
    /// Blocked by the parent.
    Listed,
    /// Not among the only sites allowed.
    NotAllowed,
    /// A search engine that can't be held to safe search.
    Search,
    /// The lists haven't been downloaded yet, so nothing can be told apart.
    NoLists,
}

impl Blocked {
    pub fn as_str(self) -> &'static str {
        match self {
            Blocked::Adult => "adult",
            Blocked::Listed => "listed",
            Blocked::NotAllowed => "not-allowed",
            Blocked::Search => "search",
            Blocked::NoLists => "no-lists",
        }
    }
}

#[derive(Default)]
struct Rules {
    on: bool,
    block_adult: bool,
    safe_search: bool,
    only_allowed: bool,
    blocked: HashSet<String>,
    allowed: HashSet<String>,
}

#[derive(Default)]
struct Compiled {
    /// The hashed names of the lists' sites, sorted.
    names: Vec<u64>,
    /// What they were built from (`SiteFilter::fingerprint`), None for none at all.
    built_from: Option<[u8; 32]>,
}

/// What's known of a list, for Settings.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListStatus {
    pub id: String,
    /// When its file was fetched (ms), 0 if it never was.
    pub updated: i64,
    pub error: String,
}

/// The filter of a child's profile: every request's host goes through `check`.
pub struct SiteFilter {
    dir: PathBuf,
    /// Where the files' state is noted (`Parental::files`), to tell when they were
    /// changed outside Kite.
    store: Option<Parental>,
    rules: RwLock<Rules>,
    lists: RwLock<Compiled>,
    errors: Mutex<BTreeMap<String, String>>,
}

/// `name` and each name above it down to two labels: "a.b.example.com" also stands
/// under "b.example.com" and "example.com".
fn with_parents(name: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(name);
    std::iter::from_fn(move || {
        let current = rest?;
        rest = current.split_once('.').map(|(_, parent)| parent).filter(|p| p.contains('.'));
        Some(current)
    })
}

impl SiteFilter {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, store: None, rules: RwLock::default(), lists: RwLock::default(), errors: Mutex::default() }
    }

    /// The filter of these parental controls: its files count only as Kite wrote them.
    pub fn of(store: Parental) -> Self {
        let dir = store.filter_dir();
        Self { store: Some(store), ..Self::new(dir) }
    }

    fn noted(&self) -> Option<BTreeMap<String, String>> {
        self.store.as_ref().and_then(|s| s.files())
    }

    /// What the profile is held to; None for a profile that isn't a child's.
    pub fn configure(&self, r: Option<&Restrictions>) {
        let mut rules = self.rules.write().unwrap_or_else(|e| e.into_inner());
        *rules = match r {
            Some(r) => Rules {
                on: true,
                block_adult: r.block_adult,
                safe_search: r.safe_search,
                only_allowed: r.only_allowed,
                blocked: clean_sites(&r.blocked).into_iter().collect(),
                allowed: clean_sites(&r.allowed).into_iter().collect(),
            },
            None => Rules::default(),
        };
    }

    pub fn is_on(&self) -> bool {
        self.rules.read().unwrap_or_else(|e| e.into_inner()).on
    }

    /// How many sites the lists in use name.
    pub fn len(&self) -> usize {
        self.lists.read().unwrap_or_else(|e| e.into_inner()).names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether a request to this host goes through. (A numeric address is on no list.)
    pub fn check(&self, host: &str) -> Option<Blocked> {
        let rules = self.rules.read().unwrap_or_else(|e| e.into_inner());
        if !rules.on {
            return None;
        }
        let host = host.trim_end_matches('.').to_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host);
        if with_parents(host).any(|n| rules.allowed.contains(n)) {
            return None;
        }
        if with_parents(host).any(|n| rules.blocked.contains(n)) {
            return Some(Blocked::Listed);
        }
        if rules.only_allowed {
            return Some(Blocked::NotAllowed);
        }
        if rules.safe_search && search_without_safe_search(host) {
            return Some(Blocked::Search);
        }
        if rules.block_adult {
            let lists = self.lists.read().unwrap_or_else(|e| e.into_inner());
            if lists.built_from.is_none() {
                return Some(Blocked::NoLists);
            }
            if with_parents(host).any(|n| lists.names.binary_search(&hash_name(n)).is_ok()) {
                return Some(Blocked::Adult);
            }
        }
        None
    }

    fn list_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.txt"))
    }

    /// Changes when what the filter is built from does: the lists in use and their files.
    fn fingerprint(&self, ids: &[String]) -> Option<[u8; 32]> {
        let mut input = String::from("1\n");
        let mut any = false;
        for id in ids {
            let file = std::fs::metadata(self.list_path(id)).ok();
            any |= file.is_some();
            let modified = file.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            input += &format!("{id}\0{:?}\0{modified:?}\n", file.map(|m| m.len()));
        }
        // (With no list fetched yet there's nothing to tell sites apart by.)
        any.then(|| crate::webext::amo::sha256(input.as_bytes()))
    }

    /// The filter compiled by an earlier run, if it's of these lists as they are now.
    pub fn load_cached(&self, ids: &[String]) -> bool {
        let ids = known(ids);
        let Some(now) = self.fingerprint(&ids) else { return false };
        let Ok(bytes) = std::fs::read(self.dir.join(CACHE)) else { return false };
        if self.noted().is_some_and(|noted| noted.get(CACHE) != Some(&crate::webext::amo::sha256_hex(&bytes))) {
            log::warn!("site filter: the compiled filter isn't as Kite wrote it; building it anew");
            return false;
        }
        let Some(rest) = bytes.strip_prefix(CACHE_MAGIC).filter(|r| r.len() >= 32 && (r.len() - 32) % 8 == 0) else { return false };
        let (fingerprint, data) = rest.split_at(32);
        if fingerprint != &now[..] {
            return false;
        }
        let names = data.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect();
        *self.lists.write().unwrap_or_else(|e| e.into_inner()) = Compiled { names, built_from: Some(now) };
        true
    }

    /// Builds the filter from the lists' files. Gives the compiled file's digest.
    fn build(&self, ids: &[String]) -> Option<String> {
        let fingerprint = self.fingerprint(ids);
        let mut digest = None;
        let mut names: Vec<u64> = vec![];
        for id in ids {
            if let Ok(text) = std::fs::read_to_string(self.list_path(id)) {
                names.extend(parse_list(&text).map(|n| hash_name(&n)));
            }
        }
        names.sort_unstable();
        names.dedup();
        if let Some(f) = fingerprint {
            let mut cache = Vec::with_capacity(CACHE_MAGIC.len() + 32 + names.len() * 8);
            cache.extend_from_slice(CACHE_MAGIC);
            cache.extend_from_slice(&f);
            for n in &names {
                cache.extend_from_slice(&n.to_le_bytes());
            }
            if crate::util::write_atomic(&self.dir.join(CACHE), &cache).is_ok() {
                digest = Some(crate::webext::amo::sha256_hex(&cache));
            }
        }
        log::info!("site filter: {} sites from {} lists", names.len(), ids.len());
        *self.lists.write().unwrap_or_else(|e| e.into_inner()) = Compiled { names, built_from: fingerprint };
        digest
    }

    /// Fetches the lists that are missing or old (all of them with `force`), and builds
    /// the filter anew if they changed. Blocking; call off the UI thread.
    pub fn refresh(&self, ids: &[String], force: bool) {
        self.refresh_with(ids, force, &fetch_list)
    }

    fn refresh_with(&self, ids: &[String], force: bool, fetch: &dyn Fn(&ListInfo) -> anyhow::Result<String>) {
        let ids = known(ids);
        let noted = self.noted();
        let mut notes: Vec<(String, Option<String>)> = vec![];
        for id in &ids {
            let Some(info) = LISTS.iter().find(|l| l.id == id) else { continue };
            let path = self.list_path(id);
            // A list that isn't as it was downloaded (emptied, say) is fetched anew.
            if let (Some(noted), Ok(there)) = (&noted, std::fs::read(&path)) {
                if noted.get(id) != Some(&crate::webext::amo::sha256_hex(&there)) {
                    log::warn!("site filter: {id} isn't as it was downloaded; fetching it anew");
                    let _ = std::fs::remove_file(&path);
                }
            }
            let stale = std::fs::metadata(&path).and_then(|m| m.modified()).map(|t| t.elapsed().unwrap_or_default() > LIST_MAX_AGE).unwrap_or(true);
            if !stale && !force {
                continue;
            }
            let mut errors = self.errors.lock().unwrap_or_else(|e| e.into_inner());
            match fetch(info) {
                // (An error page or an empty answer isn't a list.)
                Ok(text) if parse_list(&text).take(100).count() == 100 => {
                    errors.remove(id);
                    drop(errors);
                    match crate::util::write_atomic(&path, text.as_bytes()) {
                        Ok(()) => notes.push((id.clone(), Some(crate::webext::amo::sha256_hex(text.as_bytes())))),
                        Err(e) => log::warn!("site filter: can't keep {id}: {e}"),
                    }
                }
                Ok(_) => {
                    log::warn!("site filter: what came for {id} isn't a list");
                    errors.insert(id.clone(), "What was downloaded isn't a list.".into());
                }
                Err(e) => {
                    log::warn!("site filter: failed to fetch {id}: {e}");
                    errors.insert(id.clone(), format!("Couldn't be downloaded: {e}"));
                }
            }
        }
        let built = self.lists.read().unwrap_or_else(|e| e.into_inner()).built_from;
        if built.is_none() || built != self.fingerprint(&ids) {
            notes.push((CACHE.to_owned(), self.build(&ids)));
        }
        if let Some(store) = self.store.as_ref().filter(|_| !notes.is_empty()) {
            store.record_files(notes);
        }
    }

    pub fn status(&self) -> Vec<ListStatus> {
        let errors = self.errors.lock().unwrap_or_else(|e| e.into_inner());
        LISTS
            .iter()
            .map(|l| {
                let updated = std::fs::metadata(self.list_path(l.id))
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as i64);
                ListStatus { id: l.id.to_owned(), updated, error: errors.get(l.id).cloned().unwrap_or_default() }
            })
            .collect()
    }
}

/// The ids that are lists, in `LISTS`' order, each once.
fn known(ids: &[String]) -> Vec<String> {
    LISTS.iter().filter(|l| ids.iter().any(|id| id == l.id)).map(|l| l.id.to_owned()).collect()
}

fn search_without_safe_search(host: &str) -> bool {
    let mut labels = host.rsplit('.');
    let (_, second, third) = (labels.next(), labels.next(), labels.next());
    // (yandex.com, yandex.ru, yandex.com.tr, and what's under them.)
    if second == Some("yandex") || third == Some("yandex") || host == "ya.ru" {
        return true;
    }
    with_parents(host).any(|n| SEARCH_WITHOUT_SAFE_SEARCH.contains(&n))
}

fn fetch(url: &str) -> anyhow::Result<String> {
    let mut resp = crate::net::agent().get(url).header("User-Agent", crate::webext::amo::USER_AGENT).call()?;
    Ok(resp.body_mut().with_config().limit(LIST_MAX_SIZE).read_to_string()?)
}

fn fetch_list(info: &ListInfo) -> anyhow::Result<String> {
    if !info.index {
        return fetch(info.url);
    }
    // The list's file has a new name with every update: an index beside it says which.
    let index: serde_json::Value = serde_json::from_str(&fetch(info.url)?)?;
    let url = index["blocklist"]["raw_url"].as_str().unwrap_or_default();
    // (Only where the index itself is.)
    let folder = info.url.rsplit_once('/').map_or("", |(f, _)| f).rsplit_once('/').map_or("", |(f, _)| f);
    if folder.is_empty() || !url.starts_with(folder) {
        anyhow::bail!("its index names no list");
    }
    fetch(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_password(dir: &Path) -> Parental {
        let p = Parental::at(dir);
        p.set_password("", "sesame").unwrap();
        p
    }

    #[test]
    fn the_password_is_checked_and_changed_only_with_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let p = Parental::at(dir.path());
        assert!(!p.has_password());
        assert!(p.verify("anything").is_err(), "none set: nothing passes");
        assert!(p.set_password("", "abc").is_err(), "too short");
        p.set_password("", "sesame").unwrap();
        assert!(p.verify("sesame").is_ok());
        assert!(p.verify("Sesame").is_err());
        assert!(p.set_password("wrong", "another").is_err());
        assert!(p.verify("sesame").is_ok(), "still the old one");
        p.set_password("sesame", "another").unwrap();
        assert!(p.verify("another").is_ok());
        assert!(p.verify("sesame").is_err());
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(!text.contains("another"), "the password itself isn't kept");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dir.path().join(FILE)).unwrap().permissions().mode() & 0o777, 0o600, "and the file is the user's alone");
    }

    #[test]
    fn wrong_passwords_in_a_row_mean_waiting() {
        assert_eq!(wait_after(2), Duration::ZERO);
        assert_eq!(wait_after(3), Duration::from_secs(5));
        assert_eq!(wait_after(5), Duration::from_secs(20));
        assert_eq!(wait_after(40), Duration::from_secs(300));
        let dir = tempfile::tempdir().unwrap();
        let p = with_password(dir.path());
        for _ in 0..3 {
            assert!(p.verify("nope").is_err());
        }
        let refused = p.verify("sesame").unwrap_err().to_string();
        assert!(refused.contains("Too many"), "even the right one waits: {refused}");
        // Once the wait is over, the right one counts again and clears the count.
        let mut s = p.load();
        s.last_failure -= 6000;
        p.save(&s).unwrap();
        assert!(p.verify("sesame").is_ok());
        assert_eq!(p.load().failures, 0);
    }

    #[test]
    fn child_profiles_and_who_needs_the_password() {
        let dir = tempfile::tempdir().unwrap();
        let p = Parental::at(dir.path());
        assert!(!p.needs_password(Some("default")));
        assert!(p.set_child("Kid", Some(Restrictions::default())).is_err(), "not without a password");
        p.set_password("", "sesame").unwrap();
        assert!(p.set_child("default", Some(Restrictions::default())).is_err());
        assert!(p.set_child("../x", Some(Restrictions::default())).is_err());
        assert!(!p.guards_profiles(), "nothing to guard yet");
        p.set_child("Kid", Some(Restrictions { blocked: vec!["https://www.Example.com/page".into(), "example.com".into(), "not a site".into()], ..Default::default() })).unwrap();
        assert!(p.is_child("Kid") && !p.is_child("default"));
        assert_eq!(p.restrictions("Kid").unwrap().blocked, ["example.com"], "sites are kept as names, each once");
        assert!(p.guards_profiles());
        assert!(p.needs_password(Some("default")) && p.needs_password(Some("Work")) && p.needs_password(None));
        assert!(!p.needs_password(Some("Kid")));
        p.set_lock_unrestricted(false).unwrap();
        assert!(!p.needs_password(Some("default")));
        p.renamed("Kid", "Kiddo");
        assert!(p.is_child("Kiddo") && !p.is_child("Kid"));
        p.set_child("Kiddo", None).unwrap();
        assert!(p.children().is_empty());
    }

    #[test]
    fn a_pass_counts_once_and_only_if_real() {
        let dir = tempfile::tempdir().unwrap();
        let p = Parental::at(dir.path());
        let pass = p.grant_pass().unwrap();
        assert!(p.pass_is_good(&pass) && p.pass_is_good(&pass), "looking leaves it");
        assert!(p.redeem_pass(&pass));
        assert!(!p.redeem_pass(&pass) && !p.pass_is_good(&pass));
        for bad in ["", "../parental.json", "0123", &"g".repeat(32)] {
            assert!(!p.redeem_pass(bad) && !p.pass_is_good(bad), "{bad:?}");
        }
    }

    #[test]
    fn site_names() {
        assert_eq!(site_name(" https://WWW.Example.com:8080/a?b#c "), Some("example.com".into()));
        assert_eq!(site_name("user@sub.example.org"), Some("sub.example.org".into()));
        assert_eq!(site_name("example.com."), Some("example.com".into()));
        for bad in ["", "   ", "two words", "a..b", "exa<mple.com"] {
            assert_eq!(site_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn lists_of_every_kind_are_read() {
        let text = "# a hosts file\n127.0.0.1 localhost\n0.0.0.0 0.0.0.0\n0.0.0.0 www.one.example # note\n\ntwo.example\n||three.example^\n! comment\n[Adblock Plus]\n*.four.example\n::1 ip6-localhost\nnodot\n";
        assert_eq!(parse_list(text).collect::<Vec<_>>(), ["one.example", "two.example", "three.example", "four.example"]);
    }

    fn filter(dir: &Path, lists: &[(&str, &str)], r: &Restrictions) -> SiteFilter {
        let f = SiteFilter::new(dir.to_owned());
        std::fs::create_dir_all(dir).unwrap();
        for (id, text) in lists {
            std::fs::write(f.list_path(id), text).unwrap();
        }
        f.configure(Some(r));
        f.build(&known(&r.lists));
        f
    }

    #[test]
    fn listed_sites_are_blocked_with_all_under_them() {
        let dir = tempfile::tempdir().unwrap();
        let f = filter(dir.path(), &[("stevenblack", "0.0.0.0 adult.example\n0.0.0.0 pages.host.example\n")], &Restrictions::default());
        assert_eq!(f.len(), 2);
        assert_eq!(f.check("adult.example"), Some(Blocked::Adult));
        assert_eq!(f.check("WWW.Adult.Example."), Some(Blocked::Adult));
        assert_eq!(f.check("cdn.img.adult.example"), Some(Blocked::Adult));
        assert_eq!(f.check("pages.host.example"), Some(Blocked::Adult));
        assert_eq!(f.check("host.example"), None, "a listed name says nothing of the one above it");
        assert_eq!(f.check("notadult.example"), None);
        assert_eq!(f.check("example"), None);
        assert_eq!(f.check("192.168.1.1"), None);
        // A profile that isn't a child's is held to nothing.
        f.configure(None);
        assert_eq!(f.check("adult.example"), None);
    }

    #[test]
    fn the_parents_own_lists_come_first() {
        let dir = tempfile::tempdir().unwrap();
        let r = Restrictions { blocked: vec!["games.example".into()], allowed: vec!["fine.adult.example".into(), "startpage.com".into()], ..Default::default() };
        let f = filter(dir.path(), &[("stevenblack", "adult.example\n")], &r);
        assert_eq!(f.check("games.example"), Some(Blocked::Listed));
        assert_eq!(f.check("play.games.example"), Some(Blocked::Listed));
        assert_eq!(f.check("fine.adult.example"), None, "allowed, though under a listed one");
        assert_eq!(f.check("other.adult.example"), Some(Blocked::Adult));
        assert_eq!(f.check("www.startpage.com"), None, "an allowed search engine stays");
        // Only the allowed sites.
        f.configure(Some(&Restrictions { only_allowed: true, allowed: vec!["school.example".into()], ..Default::default() }));
        assert_eq!(f.check("lessons.school.example"), None);
        assert_eq!(f.check("anything.example"), Some(Blocked::NotAllowed));
    }

    #[test]
    fn search_engines_without_safe_search_are_blocked_with_it_on() {
        let dir = tempfile::tempdir().unwrap();
        let f = filter(dir.path(), &[("stevenblack", "adult.example\n")], &Restrictions::default());
        for host in ["www.startpage.com", "search.brave.com", "yandex.ru", "images.yandex.com.tr", "ya.ru", "images.search.yahoo.com"] {
            assert_eq!(f.check(host), Some(Blocked::Search), "{host}");
        }
        for host in ["www.google.com", "duckduckgo.com", "brave.com", "yahoo.com", "yandexx.example", "www.ecosia.org"] {
            assert_eq!(f.check(host), None, "{host}");
        }
        f.configure(Some(&Restrictions { safe_search: false, ..Default::default() }));
        assert_eq!(f.check("www.startpage.com"), None);
        let rules = safe_search_resolver_rules();
        assert!(rules.contains("MAP www.google.* forcesafesearch.google.com") && !rules.contains('"'), "{rules}");
    }

    #[test]
    fn without_any_list_nothing_opens_until_one_is_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let f = SiteFilter::new(dir.path().join("sitefilter"));
        let r = Restrictions { allowed: vec!["school.example".into()], ..Default::default() };
        f.configure(Some(&r));
        assert!(!f.load_cached(&r.lists));
        assert_eq!(f.check("anything.example"), Some(Blocked::NoLists));
        assert_eq!(f.check("school.example"), None, "but what the parent allowed does");
        // A download that fails changes nothing; one that isn't a list isn't kept.
        f.refresh_with(&r.lists, false, &|_| anyhow::bail!("offline"));
        assert_eq!(f.check("anything.example"), Some(Blocked::NoLists));
        assert!(f.status().iter().find(|s| s.id == "stevenblack").unwrap().error.contains("offline"));
        f.refresh_with(&r.lists, false, &|_| Ok("<html>Not found</html>".into()));
        assert_eq!(f.check("anything.example"), Some(Blocked::NoLists));
        // One list arriving is enough to tell sites apart.
        let list: String = (0..150).map(|i| format!("0.0.0.0 adult{i}.example\n")).collect();
        f.refresh_with(&r.lists, false, &|l| if l.id == "stevenblack" { Ok(list.clone()) } else { anyhow::bail!("offline") });
        assert_eq!(f.check("anything.example"), None);
        assert_eq!(f.check("adult7.example"), Some(Blocked::Adult));
        let status = f.status();
        assert!(status.iter().find(|s| s.id == "stevenblack").unwrap().updated > 0 && status.iter().find(|s| s.id == "stevenblack").unwrap().error.is_empty());
        // With the block on adult sites off, no list is needed.
        f.configure(Some(&Restrictions { block_adult: false, ..Default::default() }));
        assert_eq!(f.check("adult7.example"), None);
    }

    #[test]
    fn the_compiled_filter_is_kept_and_only_used_for_the_same_lists() {
        let dir = tempfile::tempdir().unwrap();
        let r = Restrictions::default();
        let f = filter(dir.path(), &[("stevenblack", "adult.example\n"), ("oisd-nsfw", "other.example\n")], &r);
        assert_eq!(f.len(), 2);
        let again = SiteFilter::new(dir.path().to_owned());
        again.configure(Some(&r));
        assert!(again.load_cached(&r.lists));
        assert_eq!(again.check("other.example"), Some(Blocked::Adult));
        // Fewer lists in use: the kept one isn't of these.
        let fewer = SiteFilter::new(dir.path().to_owned());
        assert!(!fewer.load_cached(&["stevenblack".to_owned()]));
        // A fresh list is left alone; nothing changed, so nothing is built.
        let fetched = std::cell::Cell::new(0);
        again.refresh_with(&r.lists, false, &|_| {
            fetched.set(fetched.get() + 1);
            anyhow::bail!("offline")
        });
        assert_eq!(fetched.get(), 1, "only the list that was never fetched");
        assert_eq!(again.check("adult.example"), Some(Blocked::Adult));
    }

    #[test]
    fn the_seal_is_the_state_hashed_like_the_password_with_its_salt_reversed() {
        let dir = tempfile::tempdir().unwrap();
        let p = with_password(dir.path());
        p.set_child("Kid", Some(Restrictions::default())).unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        let mut state: serde_json::Value = serde_json::from_str(&text).unwrap();
        let seal = state.as_object_mut().unwrap().shift_remove(SEAL).unwrap();
        let mut salt = unb64(state["password"]["salt"].as_str().unwrap()).unwrap();
        salt.reverse();
        assert_eq!(seal.as_str().unwrap(), hash_password(&serde_json::to_string(&state).unwrap(), &salt).unwrap());
        assert_eq!(std::fs::read_to_string(dir.path().join(COPY)).unwrap(), text, "and there's a second copy");
        assert_eq!(p.check(), Integrity::Fine);
        // Without a password there's nothing to seal, and nothing to find wrong.
        let none = tempfile::tempdir().unwrap();
        assert_eq!(Parental::at(none.path()).check(), Integrity::Fine);
        Parental::at(none.path()).set_lock_unrestricted(false).unwrap();
        assert!(!std::fs::read_to_string(none.path().join(FILE)).unwrap().contains(SEAL) && !none.path().join(COPY).exists());
        assert_eq!(Parental::at(none.path()).check(), Integrity::Fine);
    }

    #[test]
    fn a_file_changed_or_removed_outside_kite_is_put_back_from_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let p = with_password(dir.path());
        p.set_child("Kid", Some(Restrictions::default())).unwrap();
        let path = dir.path().join(FILE);
        let good = std::fs::read_to_string(&path).unwrap();
        // The child's profile made an ordinary one, by hand.
        let mut edited: serde_json::Value = serde_json::from_str(&good).unwrap();
        edited["children"] = serde_json::json!({});
        std::fs::write(&path, serde_json::to_string_pretty(&edited).unwrap()).unwrap();
        assert!(!p.is_child("Kid"));
        assert_eq!(p.check(), Integrity::Restored);
        assert!(p.is_child("Kid") && p.restored() > 0);
        assert_eq!(p.check(), Integrity::Fine, "it's sealed again");
        p.dismiss_restored().unwrap();
        assert_eq!((p.restored(), p.check()), (0, Integrity::Fine));
        // Removed, emptied, or without its password.
        for damage in [None, Some("{}"), Some("not json"), Some(r#"{"children": {}, "lockUnrestricted": false}"#)] {
            match damage {
                None => std::fs::remove_file(&path).unwrap(),
                Some(text) => std::fs::write(&path, text).unwrap(),
            }
            assert_eq!(p.check(), Integrity::Restored, "{damage:?}");
            assert!(p.is_child("Kid") && p.verify("sesame").is_ok() && p.needs_password(Some("default")), "{damage:?}");
        }
        // The copy alone gone: it's made again.
        std::fs::remove_file(dir.path().join(COPY)).unwrap();
        assert_eq!(p.check(), Integrity::Fine);
        assert!(dir.path().join(COPY).exists());
    }

    #[test]
    fn changed_with_nothing_to_put_back_from_it_takes_the_password() {
        let dir = tempfile::tempdir().unwrap();
        let p = with_password(dir.path());
        p.set_child("Kid", Some(Restrictions::default())).unwrap();
        let path = dir.path().join(FILE);
        let mut edited: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        edited["children"]["Kid"]["blockAdult"] = false.into();
        let edited = serde_json::to_string_pretty(&edited).unwrap();
        for copy in [None, Some(edited.as_str())] {
            std::fs::write(&path, &edited).unwrap();
            match copy {
                None => std::fs::remove_file(dir.path().join(COPY)).unwrap(),
                Some(text) => std::fs::write(dir.path().join(COPY), text).unwrap(),
            }
            assert_eq!(p.check(), Integrity::Changed, "{:?}", copy.is_some());
            // Nothing written meanwhile makes it count: a wrong password is noted, without a seal.
            assert!(p.verify("nope").is_err());
            assert_eq!(p.check(), Integrity::Changed);
            p.verify("sesame").unwrap();
            p.accept().unwrap();
            assert_eq!(p.check(), Integrity::Fine);
            assert!(!p.restrictions("Kid").unwrap().block_adult, "as it was found");
            assert_eq!(std::fs::read_to_string(dir.path().join(COPY)).unwrap(), std::fs::read_to_string(&path).unwrap());
            p.set_child("Kid", Some(Restrictions::default())).unwrap();
        }
        // A file from before there were seals has none: the same.
        let mut old: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        old.as_object_mut().unwrap().shift_remove(SEAL);
        std::fs::write(&path, old.to_string()).unwrap();
        std::fs::remove_file(dir.path().join(COPY)).unwrap();
        assert_eq!(p.check(), Integrity::Changed);
        // Both gone: parental controls start over (the way out of a forgotten password).
        std::fs::remove_file(&path).unwrap();
        assert_eq!(p.check(), Integrity::Fine);
        assert!(!p.has_password());
    }

    #[test]
    fn the_filters_files_count_only_as_kite_wrote_them() {
        let dir = tempfile::tempdir().unwrap();
        let p = with_password(dir.path());
        let r = Restrictions { lists: vec!["stevenblack".into()], ..Default::default() };
        p.set_child("Kid", Some(r.clone())).unwrap();
        let list: String = (0..150).map(|i| format!("0.0.0.0 adult{i}.example\n")).collect();
        let fetched = std::cell::Cell::new(0);
        let fetch = |_: &ListInfo| {
            fetched.set(fetched.get() + 1);
            Ok(list.clone())
        };
        let f = SiteFilter::of(Parental::at(dir.path()));
        f.configure(Some(&r));
        f.refresh_with(&r.lists, false, &fetch);
        assert_eq!((fetched.get(), f.check("adult7.example")), (1, Some(Blocked::Adult)));
        let noted = p.files().unwrap();
        assert!(noted.contains_key("stevenblack") && noted.contains_key(CACHE), "{noted:?}");
        assert_eq!(p.check(), Integrity::Fine, "noting them keeps the seal");
        let fresh = || {
            let f = SiteFilter::of(Parental::at(dir.path()));
            f.configure(Some(&r));
            f
        };
        assert!(fresh().load_cached(&r.lists));
        // The list emptied by hand: it isn't used, and is fetched anew.
        std::fs::write(f.list_path("stevenblack"), "0.0.0.0 nothing.example\n").unwrap();
        let again = fresh();
        assert!(!again.load_cached(&r.lists), "the compiled filter was of the list as it was");
        again.refresh_with(&r.lists, false, &fetch);
        assert_eq!((fetched.get(), again.check("adult7.example"), again.len()), (2, Some(Blocked::Adult), 150));
        // Emptied with no way to fetch it: nothing opens, rather than everything.
        std::fs::write(f.list_path("stevenblack"), "0.0.0.0 nothing.example\n").unwrap();
        let offline = fresh();
        offline.refresh_with(&r.lists, false, &|_| anyhow::bail!("offline"));
        assert_eq!(offline.check("adult7.example"), Some(Blocked::NoLists));
        offline.refresh_with(&r.lists, false, &fetch);
        assert_eq!(offline.check("adult7.example"), Some(Blocked::Adult));
        // The compiled filter emptied, its head kept: built anew from the list.
        let cache = dir.path().join("sitefilter").join(CACHE);
        let bytes = std::fs::read(&cache).unwrap();
        std::fs::write(&cache, &bytes[..CACHE_MAGIC.len() + 32]).unwrap();
        let rebuilt = fresh();
        assert!(!rebuilt.load_cached(&r.lists));
        rebuilt.refresh_with(&r.lists, false, &|_| anyhow::bail!("not needed"));
        assert_eq!((rebuilt.check("adult7.example"), rebuilt.len()), (Some(Blocked::Adult), 150));
        assert!(fresh().load_cached(&r.lists));
    }

    #[test]
    fn an_index_can_only_name_a_list_beside_it() {
        let info = LISTS.iter().find(|l| l.index).unwrap();
        let folder = info.url.rsplit_once('/').unwrap().0.rsplit_once('/').unwrap().0;
        assert_eq!(folder, "https://raw.githubusercontent.com/Bon-Appetit/porn-domains");
        assert!(LISTS.iter().all(|l| l.url.starts_with("https://")));
        let ids: HashSet<_> = LISTS.iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), LISTS.len());
    }
}
