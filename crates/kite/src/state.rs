//! Process-wide application state shared by the QML singletons and the
//! C++ network hooks.

use kite_core::adblock::AdBlock;
use kite_core::shared::Shared;
use kite_core::webext::runtime::Runtime;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub struct App {
    pub shared: Shared,
    pub runtime: Mutex<Runtime>,
    pub passwords: Mutex<kite_core::passwords::PasswordStore>,
    pub adblock: Arc<AdBlock>,
    /// "web.doNotTrack", read by the network interceptor on every request.
    pub do_not_track: std::sync::atomic::AtomicBool,
    /// "privacy.sendReferer", read by the network interceptor.
    pub send_referer: std::sync::atomic::AtomicBool,
    /// "privacy.cookies" as `cookie_policy()`, read by the cookie filter.
    pub cookie_policy: std::sync::atomic::AtomicU8,
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
    APP.set(App { shared, runtime: Mutex::new(runtime), passwords, adblock, do_not_track, send_referer, cookie_policy, qwebchannel_js: OnceLock::new(), initial_urls, private })
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
