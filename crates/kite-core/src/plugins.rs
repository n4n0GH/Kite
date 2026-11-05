//! Kite's own optional features ("plugins"): shipped in the binary, off or on
//! per user, with nothing of them running while off. Not an interface for
//! others, who have WebExtensions; so a plugin can use Kite's internals and
//! nothing here needs to stay compatible.
//!
//! Each one is a folder, crates/kite/plugins/<id>/, described by its
//! plugin.json. Its state is stored as the settings "plugins.<id>.enabled" and
//! "plugins.<id>.<key>"; those not set by the user come from here.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    #[serde(default)]
    pub default_enabled: bool,
    /// QML item made once per browser window while the plugin is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    /// QML item shown in Settings → Plugins while the plugin is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_page: Option<String>,
    /// Its entries among the shortcut actions. Its window item's runAction(id)
    /// does the work, so it needs one.
    #[serde(default)]
    pub actions: Vec<PluginAction>,
    /// Scripts for web pages, while the plugin is on.
    #[serde(default)]
    pub page_scripts: Vec<PageScript>,
    /// The plugin's settings, with their defaults. Page scripts see them, as they
    /// are when the page loads, as KITE_PLUGIN_SETTINGS.
    #[serde(default)]
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginAction {
    /// "<plugin id>.<name>".
    pub id: String,
    pub group: String,
    pub name: String,
    #[serde(default)]
    pub keys: Vec<String>,
    pub icon: String,
}

/// One script for pages, of one or more files put together in order. The other
/// fields are those of QWebEngineScript, as in Main.qml's scripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageScript {
    pub files: Vec<String>,
    pub world_id: u32,
    pub runs_on_sub_frames: bool,
    pub injection_point: u32,
}

impl Plugin {
    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let p: Self = serde_json::from_str(json)?;
        let good_id = !p.id.is_empty() && p.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        anyhow::ensure!(good_id, "plugin id {:?}: only a-z, 0-9 and -", p.id);
        anyhow::ensure!(!p.settings.contains_key("enabled"), "plugin {}: \"enabled\" is not a setting, see defaultEnabled", p.id);
        let folder = format!("plugins/{}/", p.id);
        for file in p.files() {
            anyhow::ensure!(file.starts_with(&folder) && !file.contains(".."), "plugin {}: {file} is outside its folder", p.id);
        }
        for action in &p.actions {
            anyhow::ensure!(action.id.starts_with(&format!("{}.", p.id)), "plugin {}: action {} must start with its id", p.id, action.id);
        }
        anyhow::ensure!(p.actions.is_empty() || p.window.is_some(), "plugin {}: actions need a window item to run them", p.id);
        for script in &p.page_scripts {
            anyhow::ensure!(!script.files.is_empty(), "plugin {}: a page script without files", p.id);
            // The page's own world can be reached by the page, and would see the
            // secret its messages to the window are marked with.
            anyhow::ensure!(script.world_id != 0, "plugin {}: page scripts run in their own world", p.id);
        }
        Ok(p)
    }

    /// The files it names, relative to the kite crate.
    pub fn files(&self) -> impl Iterator<Item = &String> {
        self.window.iter().chain(self.settings_page.iter()).chain(self.page_scripts.iter().flat_map(|s| s.files.iter()))
    }
}

/// All the plugins, from their plugin.json texts; each must be valid, and be
/// the only one with its id.
pub fn parse_all(manifests: &[&str]) -> anyhow::Result<Vec<Plugin>> {
    let mut out: Vec<Plugin> = Vec::new();
    for text in manifests {
        let p = Plugin::parse(text)?;
        anyhow::ensure!(out.iter().all(|o| o.id != p.id), "two plugins are called {}", p.id);
        out.push(p);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal() {
        let p = Plugin::parse(r#"{"id": "a-1", "name": "A", "description": "d", "icon": "i"}"#).unwrap();
        assert!(!p.default_enabled);
        assert_eq!(p.files().count(), 0);
    }

    #[test]
    fn refuses_what_would_go_wrong_later() {
        let with = |rest: &str| Plugin::parse(&format!(r#"{{"name": "A", "description": "d", "icon": "i", {rest}}}"#));
        assert!(with(r#""id": "Bad.id""#).is_err());
        assert!(with(r#""id": """#).is_err());
        assert!(with(r#""id": "a", "typo": 1"#).is_err());
        assert!(with(r#""id": "a", "settings": {"enabled": true}"#).is_err());
        assert!(with(r#""id": "a", "window": "plugins/b/W.qml""#).is_err());
        assert!(with(r#""id": "a", "window": "plugins/a/../b/W.qml""#).is_err());
        assert!(with(r#""id": "a", "window": "plugins/a/W.qml""#).is_ok());
        let action = r#""actions": [{"id": "a.go", "group": "G", "name": "N", "icon": "i"}]"#;
        assert!(with(&format!(r#""id": "a", {action}"#)).is_err(), "no window to run it");
        assert!(with(&format!(r#""id": "a", "window": "plugins/a/W.qml", {action}"#)).is_ok());
        assert!(with(&format!(r#""id": "b", "window": "plugins/b/W.qml", {action}"#)).is_err(), "another's id");
        let script = |world: u32, file: &str| format!(r#""id": "a", "pageScripts": [{{"files": ["{file}"], "worldId": {world}, "runsOnSubFrames": true, "injectionPoint": 0}}]"#);
        assert!(with(&script(1, "plugins/a/p.js")).is_ok());
        assert!(with(&script(0, "plugins/a/p.js")).is_err(), "the page's world");
        assert!(with(&script(1, "plugins/b/p.js")).is_err(), "another's file");
    }

    #[test]
    fn ids_are_unique() {
        let a = r#"{"id": "a", "name": "A", "description": "d", "icon": "i"}"#;
        let b = r#"{"id": "b", "name": "B", "description": "d", "icon": "i"}"#;
        assert_eq!(parse_all(&[a, b]).unwrap().len(), 2);
        assert!(parse_all(&[a, b, a]).is_err());
    }
}
