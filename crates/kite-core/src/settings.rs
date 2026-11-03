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
        "general.searchEngine": "DuckDuckGo",
        "general.searchFromLocationBar": true,
        "general.showSearchSuggestions": true,
        // Appearance
        "appearance.showBookmarksToolbar": true,
        "appearance.showStatusBar": true,
        "appearance.showHomeButton": true,
        "appearance.tabPosition": "top",       // "top" | "belowToolbar" | "bottom" | "vertical"
        "appearance.defaultZoom": 100,
        "appearance.forceDarkMode": false,
        // Tabs
        "tabs.openNewTabsInBackground": true,
        "tabs.openPopupsInTabs": false,
        "tabs.newTabAfterActive": true,
        "tabs.closeWindowWithLastTab": false,
        "tabs.switchToNewTabFromLink": false,
        "tabs.showCloseButtons": true,
        // Browsing
        "web.javascript": true,
        "web.autoloadImages": true,
        "web.localStorage": true,
        "web.pluginsEnabled": false,
        "web.spatialNavigation": false,
        "web.smoothScrolling": true,
        "web.dnsPrefetch": true,
        "web.printBackgrounds": true,
        "web.userAgent": "",
        "web.doNotTrack": false,
        "web.spellcheck": true,
        "web.spellcheckLanguages": ["en-US"],
        // Privacy
        "privacy.saveHistory": true,
        "privacy.cookies": "all",             // "all" | "noThirdParty" | "none"
        "privacy.clearOnExit": false,
        "privacy.sendReferer": true,
        // Passwords
        "passwords.save": true,
        "passwords.autofill": true,
        // Downloads
        "downloads.directory": downloads,
        "downloads.askWhereToSave": false,
        "downloads.closeManagerOnFinish": false,
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
        // Window
        "window.width": 1280,
        "window.height": 840,
        "window.maximized": false
    });
    v.as_object().cloned().unwrap()
}

impl Settings {
    pub fn load(path: PathBuf) -> Self {
        let mut values = defaults();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(stored)) = serde_json::from_str::<Value>(&text) {
                for (k, v) in stored {
                    values.insert(k, v);
                }
            }
        }
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
