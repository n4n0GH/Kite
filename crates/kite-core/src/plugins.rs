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
    /// The plugin's settings, with their defaults.
    #[serde(default)]
    pub settings: Map<String, Value>,
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
        Ok(p)
    }

    /// The files it names, relative to the kite crate.
    pub fn files(&self) -> impl Iterator<Item = &String> {
        self.window.iter().chain(self.settings_page.iter())
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
    }

    #[test]
    fn ids_are_unique() {
        let a = r#"{"id": "a", "name": "A", "description": "d", "icon": "i"}"#;
        let b = r#"{"id": "b", "name": "B", "description": "d", "icon": "i"}"#;
        assert_eq!(parse_all(&[a, b]).unwrap().len(), 2);
        assert!(parse_all(&[a, b, a]).is_err());
    }
}
