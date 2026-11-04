//! Process-wide application state shared by the QML singletons and the
//! C++ network hooks.

use kite_core::adblock::AdBlock;
use kite_core::shared::Shared;
use kite_core::webext::runtime::Runtime;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub struct App {
    pub shared: Shared,
    pub runtime: Mutex<Runtime>,
    pub passwords: Mutex<kite_core::passwords::PasswordStore>,
    /// Screen kept on for tabs holding a Screen Wake Lock.
    pub screen: Mutex<kite_core::screensaver::ScreenInhibitor>,
    pub adblock: Arc<AdBlock>,
    /// "web.doNotTrack", read by the network interceptor on every request.
    pub do_not_track: std::sync::atomic::AtomicBool,
    /// "privacy.sendReferer", read by the network interceptor.
    pub send_referer: std::sync::atomic::AtomicBool,
    /// "privacy.cookies" as `cookie_policy()`, read by the cookie filter.
    pub cookie_policy: std::sync::atomic::AtomicU8,
    /// The privacy settings the network layer reads ("privacy.*").
    pub privacy: Privacy,
    pub qwebchannel_js: OnceLock<String>,
    /// URLs passed on the command line.
    pub initial_urls: Vec<String>,
    pub private: bool,
}

static APP: OnceLock<App> = OnceLock::new();

/// KNotification needs an event description file to show desktop notifications.
fn install_notifyrc() {
    let Some(base) = directories::BaseDirs::new() else { return };
    let path = base.data_dir().join("knotifications6").join("kite.notifyrc");
    if path.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(
        &path,
        "[Global]\nIconName=internet-web-browser\nComment=Kite Web Browser\nName=Kite\n\n[Event/notification]\nName=Web notification\nComment=A website or extension sent a notification\nAction=Popup\n",
    );
}

pub fn init(profile: &str, initial_urls: Vec<String>, private: bool) -> anyhow::Result<()> {
    install_notifyrc();
    let profile = kite_core::Profile::open(profile)?;
    let shared = Shared::open(profile.clone())?;
    let runtime = Runtime::new(shared.clone())?;
    if let Err(e) = runtime.blocking.start() {
        log::warn!("blocking webRequest unavailable: {e}");
    }
    let passwords = Mutex::new(kite_core::passwords::PasswordStore::open(&shared.db())?);
    let adblock = Arc::new(AdBlock::new(profile.path("adblock")));
    {
        let s = shared.settings();
        adblock.set_enabled(s.get_bool("adblock.enabled"));
        adblock.set_allowlist(serde_json::from_value(s.get("adblock.allowlist")).unwrap_or_default());
    }
    adblock.load_cached();
    let (do_not_track, send_referer, cookie_policy) = {
        let s = shared.settings();
        (s.get_bool("web.doNotTrack").into(), s.get_bool("privacy.sendReferer").into(), cookie_policy(&s.get_str("privacy.cookies")).into())
    };
    let privacy = Privacy::default();
    for key in Privacy::KEYS {
        privacy.apply(key, &shared.settings().get(key));
    }
    privacy.load_cookie_rules(&shared);
    APP.set(App { shared, runtime: Mutex::new(runtime), passwords, screen: Mutex::default(), adblock, do_not_track, send_referer, cookie_policy, privacy, qwebchannel_js: OnceLock::new(), initial_urls, private })
        .map_err(|_| anyhow::anyhow!("already initialised"))?;
    Ok(())
}

/// "privacy.cookies" value → policy for the cookie filter (glue.cpp).
pub fn cookie_policy(setting: &str) -> u8 {
    match setting {
        "none" => 2,
        "noThirdParty" => 1,
        _ => 0,
    }
}

pub fn app() -> &'static App {
    APP.get().expect("state::init must run first")
}

impl App {
    pub fn runtime(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Rebuild the content blocker from settings on a worker thread.
    pub fn reload_adblock(&self, download: bool, done: impl FnOnce(anyhow::Result<usize>) + Send + 'static) {
        let (subs, rules) = {
            let s = self.shared.settings();
            (
                serde_json::from_value::<Vec<kite_core::adblock::Subscription>>(s.get("adblock.subscriptions")).unwrap_or_default(),
                s.get_str("adblock.customRules"),
            )
        };
        let ab = self.adblock.clone();
        std::thread::spawn(move || done(ab.rebuild(&subs, &rules, download)));
    }
}

/// Privacy settings read on every request (the interceptor runs on the UI
/// thread, so these are atomics), and HTTPS-only's session state.
#[derive(Default)]
pub struct Privacy {
    /// "privacy.globalPrivacyControl": the Sec-GPC header.
    pub gpc: AtomicBool,
    /// "privacy.stripTrackingParams": remove click/campaign IDs from page URLs.
    pub strip_params: AtomicBool,
    /// "privacy.httpsOnly": upgrade page loads to https.
    pub https_only: AtomicBool,
    /// Hosts the user chose to visit over plain http this session.
    pub http_allowed: Mutex<std::collections::HashSet<String>>,
    /// Hosts recently upgraded, so a failed https load can offer the http page.
    pub upgraded: Mutex<std::collections::HashMap<String, std::time::Instant>>,
    /// "privacy.siteCookieRules": the sites' cookie rules (Site Settings) apply.
    pub site_cookie_rules: AtomicBool,
    /// Host → rule (1 = allow, 2 = block, 3 = allow until Kite closes), read
    /// by the cookie filter on Chromium's network thread.
    pub cookie_rules: std::sync::RwLock<std::collections::HashMap<String, u8>>,
}

impl Privacy {
    pub const KEYS: [&'static str; 4] = ["privacy.globalPrivacyControl", "privacy.stripTrackingParams", "privacy.httpsOnly", "privacy.siteCookieRules"];

    /// Follow a setting change (unknown keys are ignored).
    pub fn apply(&self, key: &str, v: &serde_json::Value) {
        let on = v.as_bool().unwrap_or(false);
        match key {
            "privacy.globalPrivacyControl" => self.gpc.store(on, Relaxed),
            "privacy.stripTrackingParams" => self.strip_params.store(on, Relaxed),
            "privacy.httpsOnly" => self.https_only.store(on, Relaxed),
            "privacy.siteCookieRules" => self.site_cookie_rules.store(on, Relaxed),
            _ => {}
        }
    }

    /// The https URL a page load of `url` goes to instead, if HTTPS-only
    /// upgrades it.
    pub fn upgrade(&self, url: &str) -> Option<String> {
        if !self.https_only.load(Relaxed) {
            return None;
        }
        let host = kite_core::util::host_of(url);
        if self.http_allowed.lock().unwrap().contains(&host) {
            return None;
        }
        let to = kite_core::privacy::https_upgrade(url)?;
        self.upgraded.lock().unwrap().insert(host, std::time::Instant::now());
        Some(to)
    }

    /// Whether a failed load of `url` was an HTTPS-only upgrade (in the last
    /// minute), so the http page may be offered instead.
    pub fn was_upgraded(&self, url: &str) -> bool {
        let host = kite_core::util::host_of(url);
        self.upgraded.lock().unwrap().get(&host).is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(60))
    }

    /// Reload the sites' cookie rules (after a site setting changed).
    pub fn load_cookie_rules(&self, shared: &Shared) {
        let rules = kite_core::sitesettings::all(&shared.db())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, s)| (1..=3).contains(&s.cookies))
            .map(|(h, s)| (h, s.cookies as u8))
            .collect();
        *self.cookie_rules.write().unwrap() = rules;
    }

    /// The cookie rule for `host` (or the closest parent domain with one):
    /// 0 = none (the global policy applies).
    pub fn cookie_rule(&self, host: &str) -> u8 {
        if !self.site_cookie_rules.load(Relaxed) {
            return 0;
        }
        let rules = self.cookie_rules.read().unwrap();
        let mut h = host.trim_start_matches('.');
        loop {
            if let Some(r) = rules.get(h) {
                return *r;
            }
            match h.split_once('.') {
                Some((_, parent)) if parent.contains('.') => h = parent,
                _ => return 0,
            }
        }
    }

    /// Hosts whose cookies go when Kite closes (rule 3).
    pub fn session_cookie_hosts(&self) -> Vec<String> {
        if !self.site_cookie_rules.load(Relaxed) {
            return vec![];
        }
        self.cookie_rules.read().unwrap().iter().filter(|(_, r)| **r == 3).map(|(h, _)| h.clone()).collect()
    }

    pub fn allow_http(&self, host: &str) {
        self.http_allowed.lock().unwrap().insert(host.to_owned());
    }
}
