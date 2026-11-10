//! User preferences, stored as a flat JSON object with typed defaults.
//! Mirrors the most important groups of Falkon's preferences dialog.

use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct Settings {
    path: PathBuf,
    values: Map<String, Value>,
}

pub fn defaults() -> Map<String, Value> {
    let downloads = directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let v = json!({
        // General
        "general.homepage": "kite:start",
        "general.newTabPage": "kite:start",
        "general.startup": "restore",          // "homepage" | "blank" | "restore" | "speeddial"
        // Home and new tab page: an address ("url") or a local HTML file
        // ("file"), used in place or copied into the profile (startpage.rs).
        "general.homepageMode": "url",
        "general.newTabPageMode": "url",
        "startpage.home.file": "",
        "startpage.home.copy": false,
        "startpage.newtab.file": "",
        "startpage.newtab.copy": false,
        "general.searchEngine": "DuckDuckGo",
        // The first-launch page (kite:welcome): "pending" in a new profile until
        // it's finished or left; profiles from before it are "done".
        "general.welcome": "done",
        "general.searchFromLocationBar": true,
        "general.showSearchSuggestions": true,
        "general.crashReportNotice": true,     // the crash report dialog at the start after a crash
        // Appearance
        "appearance.showBookmarksToolbar": true,
        "appearance.showStatusBar": true,
        "appearance.showMenuBar": false,
        // Navigation toolbar contents, in order (Customize Toolbar…).
        "toolbar.items": ["back", "forward", "reload", "home", "urlbar", "extensions", "downloads", "menu"],
        "appearance.defaultZoom": 100,
        "appearance.forceDarkMode": false,
        // Thin page scrollbars that widen on hover and hide while idle (needs a restart).
        "appearance.overlayScrollbars": true,
        // Tabs
        "tabs.position": "top",                // "top" | "belowToolbar" | "bottom" | "vertical" (left) | "verticalRight"
        "tabs.hideWhenSingle": false,
        "tabs.openPopupsInTabs": false,
        "tabs.newTabAfterActive": true,       // new tabs (Ctrl+T, the + button) after the current tab
        // Tabs opened from a link: "afterRelated" (after its tab and the tabs it
        // opened before, as Firefox does) | "afterOpener" (right after its tab) | "end".
        "tabs.linkTabPosition": "afterRelated",
        "tabs.closeWindowWithLastTab": false,
        "tabs.switchToNewTabFromLink": false,
        // Closing a tab opened from another goes back to that one.
        "tabs.selectOpenerOnClose": true,
        // Browsing
        "web.javascript": true,
        "web.autoloadImages": true,
        "web.localStorage": true,
        "web.pointerLock": true,              // pages may lock the mouse pointer (games; cpp/pointerlock.cpp)
        "web.spatialNavigation": false,
        "web.smoothScrolling": true,
        "web.dnsPrefetch": false,             // looks up every link on a page (Settings → Privacy)
        "web.printBackgrounds": true,
        "web.userAgent": "",
        // Google's sign-in refuses Kite unless it appears as Firefox there.
        "web.googleSignIn": true,
        "devtools.place": "right",             // "right" | "bottom" | "window"
        "devtools.width": 520,
        "devtools.height": 320,
        "browsing.formFill": true,
        // Firefox: browser.tabs.min_inactive_duration_before_unload (10 min).
        "tabs.fillWidth": false,
        // Horizontal tabs drawn by the application style (Breeze, Oxygen, …) instead of Kite.
        "tabs.styleDrawn": false,
        // Links open as children of their tab, in vertical tab lists.
        "tabs.tree": false,
        // Speed dial: tiles the user added ([{url, title}], shown first) and
        // most-visited sites the user removed ([url]).
        "speeddial.showTiles": true,
        // In private windows, pages from the history: among the start page's
        // tiles, and among the address bar's suggestions.
        "privacy.protocolHandlers": true,
        "privacy.privateHistoryTiles": false,
        "privacy.privateHistorySuggestions": false,
        "privacy.privateNotifications": false,  // from pages and downloads in private windows
        "speeddial.pinned": [],
        // The tiles' order after dragging ([url]); others follow.
        "speeddial.order": [],
        "speeddial.hidden": [],
        // Scroll distance per mouse wheel notch on pages, in px; 0 = the system's.
        "web.wheelScrollPixels": 0,
        // Settings → Shortcuts: action id → key sequences, for the actions
        // whose keys the user changed (the defaults live in Main.qml).
        "shortcuts": {},
        "tabs.unloadInactive": true,
        "tabs.unloadAfterMinutes": 10,
        "tabs.unloadKeepPinned": true,
        "browsing.formFillSave": true,
        "browsing.fileFolder": "last",         // where a page's file picker opens: "home" | "last" | "custom"
        "browsing.fileFolderPath": "",         // the "custom" folder
        "browsing.safeBrowsing": "off",        // "off" | "google" | "open" (safebrowsing.rs)
        "browsing.safeBrowsingKey": "",        // Google Safe Browsing API key
        "web.unblockContextMenu": true,       // Kite's menu where a page cancels it without its own (js/context_menu.js)
        "web.unblockSelection": true,         // selecting and copying text where a page stops it (js/text_unblock.js)
        "web.doNotTrack": true,
        "web.spellcheck": true,
        "web.spellcheckLanguages": ["en-US"],
        // Privacy
        "privacy.saveHistory": true,
        "privacy.cookies": "noThirdParty",    // "all" | "noThirdParty" | "none"
        "privacy.clearOnExit": false,
        "privacy.sendReferer": true,
        // Tracking protection (Settings → Privacy)
        "privacy.globalPrivacyControl": true,  // Sec-GPC header, navigator.globalPrivacyControl
        "privacy.stripTrackingParams": true,   // fbclid, gclid, utm_* … removed from page URLs
        "privacy.blockPings": true,            // <a ping> click beacons
        "privacy.httpsOnly": true,             // upgrade page loads to https
        "privacy.webrtcPublicOnly": true,      // WebRTC doesn't reveal local network addresses
        "privacy.dnsOverHttps": "off",         // "off" | "auto" (fall back to system DNS) | "secure"
        "privacy.dnsServer": "https://cloudflare-dns.com/dns-query",
        "privacy.proxy": "system",             // "system" | "none" | "manual" (restart)
        "privacy.proxyType": "socks5",         // "socks5" | "http"
        "privacy.proxyHost": "",
        "privacy.proxyPort": 1080,
        "privacy.siteCookieRules": false,      // sites' own cookie rules (Site Settings)
        "privacy.containers": false,           // container tabs: separate cookies and storage
        "privacy.antiFingerprinting": false,   // fingerprint protection, turned on per site
        "privacy.containerList": [
            {"id": "personal", "name": "Personal", "color": "#3daee9"},
            {"id": "work", "name": "Work", "color": "#f67400"},
            {"id": "banking", "name": "Banking", "color": "#27ae60"},
            {"id": "shopping", "name": "Shopping", "color": "#da4453"}
        ],
        // Passwords
        "passwords.save": true,
        "passwords.autofill": true,
        "passwords.generator": true,
        "passwords.generatorLength": 20,
        "passwords.generatorSymbols": true,
        // Downloads
        "downloads.directory": downloads,
        "downloads.askWhereToSave": false,
        "downloads.notifications": true,       // "Download complete" (and Plasma's download jobs)
        "downloads.notificationTimeout": 0,    // seconds a notification stays; 0 = the desktop's default
        "downloads.external": false,           // hand downloads to another program (external.rs)
        "downloads.externalProgram": "",
        "downloads.externalArguments": "%d",   // %d the address, %f the file's name
        // Content blocking
        "adblock.enabled": true,
        "adblock.subscriptions": [
            {"title": "EasyList", "url": "https://easylist.to/easylist/easylist.txt", "enabled": true},
            {"title": "EasyPrivacy", "url": "https://easylist.to/easylist/easyprivacy.txt", "enabled": true},
            {"title": "uBlock filters", "url": "https://ublockorigin.github.io/uAssets/filters/filters.min.txt", "enabled": true},
            {"title": "uBlock filters: Quick fixes", "url": "https://ublockorigin.github.io/uAssets/filters/quick-fixes.min.txt", "enabled": true},
            {"title": "uBlock filters: Privacy", "url": "https://ublockorigin.github.io/uAssets/filters/privacy.min.txt", "enabled": true},
            {"title": "uBlock filters: Badware risks", "url": "https://ublockorigin.github.io/uAssets/filters/badware.min.txt", "enabled": true},
            {"title": "uBlock filters: Unbreak", "url": "https://ublockorigin.github.io/uAssets/filters/unbreak.min.txt", "enabled": true}
        ],
        "adblock.customRules": "",
        "adblock.allowlist": [],
        // Extensions
        "extensions.enabled": true,
        "extensions.amoSpoofFirefox": true,
        "extensions.siteInstalls": true,
        "extensions.unpinned": [],
        // Window
        "window.width": 1280,
        "window.height": 840,
        "window.maximized": false
    });
    v.as_object().cloned().unwrap()
}

/// Settings that moved to a new key: (old, new). Stored values are carried
/// over on load (and written under the new key on the next save).
const RENAMED: &[(&str, &str)] = &[("appearance.tabPosition", "tabs.position")];

/// Settings whose meaning moved elsewhere.
const UBLOCK_LISTS_KNOWN: &str = "adblock.ublockLists";

fn migrate(values: &mut Map<String, Value>) {
    // "Open links in background tabs" was the opposite of "Switch to tabs
    // opened from links"; only the latter is left.
    if let Some(Value::Bool(background)) = values.remove("tabs.openNewTabsInBackground") {
        if !background {
            values.insert("tabs.switchToNewTabFromLink".into(), Value::Bool(true));
        }
    }
    // Picture-in-picture and reader view became plugins: their actions have new names.
    if let Some(Value::Object(keys)) = values.get_mut("shortcuts") {
        if let Some(mine) = keys.remove("pictureInPicture") {
            keys.insert("pip.toggle".into(), mine);
        }
        if let Some(mine) = keys.remove("readerView") {
            keys.insert("reader-view.toggle".into(), mine);
        }
    }
    // And reader view's settings are the plugin's.
    for key in ["fontSize", "width", "font", "theme"] {
        if let Some(v) = values.remove(&format!("reader.{key}")) {
            values.entry(format!("{}{key}", crate::reader::PLUGIN_SETTINGS)).or_insert(v);
        }
    }
    // uBlock Origin's lists are in the defaults now, for profiles that chose their own lists too.
    // (Once: `set` marks a choice made since, and what's still the default has them anyway.)
    let wanted = defaults()["adblock.subscriptions"].clone();
    if !values.contains_key(UBLOCK_LISTS_KNOWN) && values.get("adblock.subscriptions") != Some(&wanted) {
        if let (Some(Value::Array(have)), Value::Array(wanted)) = (values.get_mut("adblock.subscriptions"), wanted) {
            for list in wanted {
                if !have.iter().any(|h| h["url"] == list["url"]) {
                    have.push(list);
                }
            }
        }
        values.insert(UBLOCK_LISTS_KNOWN.into(), Value::Bool(true));
    }
    // Settings nothing reads (web.pluginsEnabled: Pepper plugins, gone from Chromium).
    for old in ["tabs.showCloseButtons", "downloads.closeManagerOnFinish", "web.pluginsEnabled"] {
        values.remove(old);
    }
    // "Use a common user agent" (privacy.genericUserAgent) became the user agent
    // choice's default: off, it was Kite naming QtWebEngine, which is a choice there.
    if let Some(generic) = values.remove("privacy.genericUserAgent") {
        let own = values.get("web.userAgent").and_then(Value::as_str).is_some_and(|ua| !ua.trim().is_empty());
        if generic == Value::Bool(false) && !own {
            values.insert("web.userAgent".into(), Value::String(crate::util::QTWEBENGINE_USER_AGENT.into()));
        }
    }
    // "Open a page's file picker in the last folder used" became a choice of folder.
    if let Some(Value::Bool(remember)) = values.remove("browsing.rememberFileFolder") {
        if !remember {
            values.insert("browsing.fileFolder".into(), Value::String("home".into()));
        }
    }
    // "Show home button" became the home item of the customizable toolbar.
    if let Some(Value::Bool(show)) = values.remove("appearance.showHomeButton") {
        if !show {
            if let Some(Value::Array(items)) = values.get_mut("toolbar.items") {
                items.retain(|i| i != "home");
            }
        }
    }
}

impl Settings {
    pub fn load(path: PathBuf) -> Self {
        let mut values = defaults();
        // A new profile gets the welcome page: one with neither settings nor a
        // session (after --factory, the session is still there). It's written
        // down at once, so quitting halfway brings the page back next time.
        let new = !path.exists() && !path.with_file_name("session.json").exists();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(stored)) = serde_json::from_str::<Value>(&text) {
                for (k, v) in stored {
                    values.insert(RENAMED.iter().find(|(old, _)| *old == k).map_or(k, |(_, new)| (*new).to_owned()), v);
                }
            }
        }
        migrate(&mut values);
        let s = Self { path, values };
        if new {
            let mut s = s;
            s.values.insert("general.welcome".into(), Value::String("pending".into()));
            let _ = s.save();
            return s;
        }
        s
    }

    pub fn get(&self, key: &str) -> Value {
        self.values.get(key).cloned().unwrap_or(Value::Null)
    }
    pub fn get_bool(&self, key: &str) -> bool {
        self.get(key).as_bool().unwrap_or(false)
    }
    pub fn get_str(&self, key: &str) -> String {
        self.get(key).as_str().unwrap_or_default().to_owned()
    }
    pub fn get_i64(&self, key: &str) -> i64 {
        self.get(key).as_i64().unwrap_or(0)
    }

    pub fn set(&mut self, key: &str, value: Value) -> anyhow::Result<()> {
        if key == "adblock.subscriptions" {
            self.values.insert(UBLOCK_LISTS_KNOWN.into(), Value::Bool(true));
        }
        self.values.insert(key.to_owned(), value);
        self.save()
    }

    /// Back to its default, or forgotten if it has none.
    pub fn reset(&mut self, key: &str) -> anyhow::Result<()> {
        match defaults().get(key) {
            Some(v) => self.values.insert(key.to_owned(), v.clone()),
            None => self.values.remove(key),
        };
        self.save()
    }

    /// Forgets every setting starting with `prefix` (what a removed plugin stored;
    /// those have no defaults to go back to).
    pub fn remove_prefix(&mut self, prefix: &str) -> anyhow::Result<()> {
        self.values.retain(|k, _| !k.starts_with(prefix));
        self.save()
    }

    pub fn all(&self) -> &Map<String, Value> {
        &self.values
    }

    pub fn save(&self) -> anyhow::Result<()> {
        // Only persist values that differ from defaults so new defaults apply.
        let defs = defaults();
        let changed: Map<String, Value> = self
            .values
            .iter()
            .filter(|(k, v)| defs.get(*k) != Some(*v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        crate::util::write_atomic(&self.path, serde_json::to_string_pretty(&changed)?.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod pip_shortcut {
    use super::*;

    #[test]
    fn a_changed_key_for_picture_in_picture_survives_its_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"shortcuts": {"pictureInPicture": ["Ctrl+Y"], "newTab": ["Ctrl+U"]}}"#).unwrap();
        let s = Settings::load(p);
        assert_eq!(s.get("shortcuts"), json!({"pip.toggle": ["Ctrl+Y"], "newTab": ["Ctrl+U"]}));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn resets_to_the_default_or_forgets() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Settings::load(d.path().join("settings.json"));
        s.set("general.startup", serde_json::json!("blank")).unwrap();
        s.set("plugins.a.enabled", serde_json::json!(true)).unwrap();
        s.reset("general.startup").unwrap();
        s.reset("plugins.a.enabled").unwrap();
        assert_eq!(s.get("general.startup"), defaults()["general.startup"]);
        assert_eq!(s.get("plugins.a.enabled"), Value::Null);
        assert_eq!(Settings::load(d.path().join("settings.json")).get("plugins.a.enabled"), Value::Null);
    }

    #[test]
    fn forgets_what_a_plugin_stored() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Settings::load(d.path().join("settings.json"));
        s.set("plugins.a.enabled", serde_json::json!(true)).unwrap();
        s.set("plugins.a.undeclared", serde_json::json!(1)).unwrap();
        s.set("plugins.a-b.enabled", serde_json::json!(true)).unwrap();
        s.remove_prefix("plugins.a.").unwrap();
        assert_eq!(s.get("plugins.a.enabled"), Value::Null);
        assert_eq!(s.get("plugins.a.undeclared"), Value::Null);
        assert_eq!(s.get("plugins.a-b.enabled"), serde_json::json!(true), "another plugin's");
        let again = Settings::load(d.path().join("settings.json"));
        assert_eq!(again.get("plugins.a.enabled"), Value::Null, "for good");
    }

    use super::*;

    #[test]
    fn profiles_that_chose_their_lists_get_ublocks_too_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"adblock.subscriptions": [{"title": "Mine", "url": "https://mine.example/list.txt", "enabled": true}, {"title": "Off", "url": "https://easylist.to/easylist/easylist.txt", "enabled": false}]}"#).unwrap();
        let mut s = Settings::load(path.clone());
        let lists = s.get("adblock.subscriptions");
        let urls: Vec<&str> = lists.as_array().unwrap().iter().map(|l| l["url"].as_str().unwrap()).collect();
        assert_eq!(urls[0], "https://mine.example/list.txt", "theirs stay first");
        assert!(urls.iter().any(|u| u.contains("quick-fixes")));
        assert_eq!(urls.iter().filter(|u| u.contains("easylist.txt")).count(), 1, "not twice");
        assert_eq!(lists[1]["enabled"], false, "what they turned off stays off");
        // Removing one afterwards sticks.
        s.set("adblock.subscriptions", serde_json::json!([])).unwrap();
        s.save().unwrap();
        assert_eq!(Settings::load(path).get("adblock.subscriptions"), serde_json::json!([]));
    }

    #[test]
    fn a_new_profile_choosing_fewer_lists_keeps_that() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut s = Settings::load(path.clone());
        s.set("adblock.subscriptions", serde_json::json!([])).unwrap();
        s.save().unwrap();
        assert_eq!(Settings::load(path).get("adblock.subscriptions"), serde_json::json!([]));
    }
    #[test]
    fn background_tabs_setting_migrates() {
        let dir = std::env::temp_dir().join(format!("kite-settings-bg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, r#"{"tabs.openNewTabsInBackground": false, "tabs.showCloseButtons": false}"#).unwrap();
        let s = Settings::load(p.clone());
        assert!(s.get_bool("tabs.switchToNewTabFromLink"));
        assert_eq!(s.get("tabs.openNewTabsInBackground"), Value::Null);
        assert_eq!(s.get("tabs.showCloseButtons"), Value::Null);
        std::fs::write(&p, r#"{"tabs.openNewTabsInBackground": true}"#).unwrap();
        assert!(!Settings::load(p).get_bool("tabs.switchToNewTabFromLink"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_common_user_agent_switch_becomes_the_user_agent_choice() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"privacy.genericUserAgent": false}"#).unwrap();
        let s = Settings::load(p.clone());
        assert_eq!(s.get_str("web.userAgent"), "kite:qtwebengine", "off meant Kite naming QtWebEngine");
        assert_eq!(s.get("privacy.genericUserAgent"), Value::Null);
        std::fs::write(&p, r#"{"privacy.genericUserAgent": false, "web.userAgent": "Test/1.0"}"#).unwrap();
        assert_eq!(Settings::load(p.clone()).get_str("web.userAgent"), "Test/1.0", "a chosen one stays");
        std::fs::write(&p, r#"{"privacy.genericUserAgent": true}"#).unwrap();
        assert_eq!(Settings::load(p).get_str("web.userAgent"), "", "on was the default");
    }

    #[test]
    fn remembering_the_file_folder_becomes_a_choice() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        // Reader view became a plugin: its settings and its shortcut are the plugin's.
        std::fs::write(&p, r#"{"reader.fontSize": 26, "reader.theme": "sepia", "shortcuts": {"readerView": ["F9"]}}"#).unwrap();
        let s = Settings::load(p.clone());
        assert_eq!(s.get("plugins.reader-view.fontSize"), serde_json::json!(26));
        assert_eq!(s.get("plugins.reader-view.theme"), serde_json::json!("sepia"));
        assert_eq!(s.get("reader.fontSize"), Value::Null);
        assert_eq!(s.get("shortcuts")["reader-view.toggle"], serde_json::json!(["F9"]));
        std::fs::write(&p, r#"{"browsing.rememberFileFolder": false}"#).unwrap();
        let s = Settings::load(p.clone());
        assert_eq!(s.get_str("browsing.fileFolder"), "home");
        assert_eq!(s.get("browsing.rememberFileFolder"), Value::Null);
        std::fs::write(&p, r#"{"browsing.rememberFileFolder": true}"#).unwrap();
        assert_eq!(Settings::load(p).get_str("browsing.fileFolder"), "last", "on was the default");
    }

    #[test]
    fn hidden_home_button_moves_into_toolbar_items() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"appearance.showHomeButton": false}"#).unwrap();
        let s = Settings::load(p.clone());
        assert!(!s.get("toolbar.items").as_array().unwrap().iter().any(|i| i == "home"));
        assert_eq!(s.get("appearance.showHomeButton"), Value::Null);
        std::fs::write(&p, r#"{"appearance.showHomeButton": true}"#).unwrap();
        assert!(Settings::load(p).get("toolbar.items").as_array().unwrap().iter().any(|i| i == "home"));
    }

    #[test]
    fn renamed_settings_carry_over() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"appearance.tabPosition": "vertical"}"#).unwrap();
        let mut s = Settings::load(p.clone());
        assert_eq!(s.get_str("tabs.position"), "vertical");
        s.set("tabs.hideWhenSingle", true.into()).unwrap();
        let saved = std::fs::read_to_string(&p).unwrap();
        assert!(saved.contains("tabs.position") && !saved.contains("appearance.tabPosition"));
    }

    #[test]
    fn roundtrip_only_changed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        let mut s = Settings::load(p.clone());
        assert_eq!(s.get_str("general.searchEngine"), "DuckDuckGo");
        s.set("general.searchEngine", "Wikipedia".into()).unwrap();
        let stored: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        // (And that this new profile hasn't been through its welcome page.)
        let mut keys: Vec<_> = stored.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["general.searchEngine", "general.welcome"]);
        assert_eq!(Settings::load(p).get_str("general.searchEngine"), "Wikipedia");
    }

    #[test]
    fn only_new_profiles_get_the_welcome_page() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert_eq!(Settings::load(p.clone()).get_str("general.welcome"), "pending");
        // Quitting halfway: still pending.
        assert_eq!(Settings::load(p.clone()).get_str("general.welcome"), "pending");
        Settings::load(p.clone()).set("general.welcome", "done".into()).unwrap();
        assert_eq!(Settings::load(p.clone()).get_str("general.welcome"), "done");
        // A profile from before the welcome page, or after --factory (its session stays).
        let old = dir.path().join("old");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("settings.json"), "{}").unwrap();
        assert_eq!(Settings::load(old.join("settings.json")).get_str("general.welcome"), "done");
        let reset = dir.path().join("reset");
        std::fs::create_dir(&reset).unwrap();
        std::fs::write(reset.join("session.json"), "{}").unwrap();
        assert_eq!(Settings::load(reset.join("settings.json")).get_str("general.welcome"), "done");
    }
}
