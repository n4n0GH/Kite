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
        "general.searchFromLocationBar": true,
        "general.showSearchSuggestions": true,
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
        "tabs.newTabAfterActive": true,
        "tabs.closeWindowWithLastTab": false,
        "tabs.switchToNewTabFromLink": false,
        // Browsing
        "web.javascript": true,
        "web.autoloadImages": true,
        "web.localStorage": true,
        "web.pluginsEnabled": false,
        "web.spatialNavigation": false,
        "web.smoothScrolling": true,
        "web.dnsPrefetch": false,             // looks up every link on a page (Settings → Privacy)
        "web.printBackgrounds": true,
        "web.userAgent": "",
        "browsing.formFill": true,
        // Firefox: browser.tabs.min_inactive_duration_before_unload (10 min).
        "tabs.fillWidth": false,
        // Scroll distance per mouse wheel notch on pages, in px; 0 = the system's.
        "web.wheelScrollPixels": 0,
        // Settings → Shortcuts: action id → key sequences, for the actions
        // whose keys the user changed (the defaults live in Main.qml).
        "shortcuts": {},
        "tabs.unloadInactive": true,
        "tabs.unloadAfterMinutes": 10,
        "tabs.unloadKeepPinned": true,
        "browsing.formFillSave": true,
        "browsing.safeBrowsing": "off",        // "off" | "google" | "open" (safebrowsing.rs)
        "browsing.safeBrowsingKey": "",        // Google Safe Browsing API key
        "web.doNotTrack": false,
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
        "privacy.httpsOnly": false,            // upgrade page loads to https
        "privacy.genericUserAgent": true,      // no "QtWebEngine/x.y" in the user agent
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
        // Content blocking
        "adblock.enabled": true,
        "adblock.subscriptions": [
            {"title": "EasyList", "url": "https://easylist.to/easylist/easylist.txt", "enabled": true},
            {"title": "EasyPrivacy", "url": "https://easylist.to/easylist/easyprivacy.txt", "enabled": true}
        ],
        "adblock.customRules": "",
        "adblock.allowlist": [],
        // Extensions
        "extensions.enabled": true,
        "extensions.amoSpoofFirefox": true,
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
fn migrate(values: &mut Map<String, Value>) {
    // "Open links in background tabs" was the opposite of "Switch to tabs
    // opened from links"; only the latter is left.
    if let Some(Value::Bool(background)) = values.remove("tabs.openNewTabsInBackground") {
        if !background {
            values.insert("tabs.switchToNewTabFromLink".into(), Value::Bool(true));
        }
    }
    // Settings nothing reads.
    for old in ["tabs.showCloseButtons", "downloads.closeManagerOnFinish"] {
        values.remove(old);
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
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(stored)) = serde_json::from_str::<Value>(&text) {
                for (k, v) in stored {
                    values.insert(RENAMED.iter().find(|(old, _)| *old == k).map_or(k, |(_, new)| (*new).to_owned()), v);
                }
            }
        }
        migrate(&mut values);
        Self { path, values }
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
        self.values.insert(key.to_owned(), value);
        self.save()
    }

    pub fn reset(&mut self, key: &str) -> anyhow::Result<()> {
        if let Some(v) = defaults().get(key) {
            self.values.insert(key.to_owned(), v.clone());
        }
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
mod tests {
    use super::*;
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
        assert_eq!(stored.as_object().unwrap().len(), 1);
        assert_eq!(Settings::load(p).get_str("general.searchEngine"), "Wikipedia");
    }
}
