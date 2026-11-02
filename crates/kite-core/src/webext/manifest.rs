//! `manifest.json` model covering Firefox MV2 and MV3 extensions.

use super::i18n::Messages;
use super::matchpattern::is_host_pattern;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAt {
    DocumentStart,
    DocumentEnd,
    DocumentIdle,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentScript {
    pub matches: Vec<String>,
    pub exclude_matches: Vec<String>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub js: Vec<String>,
    pub css: Vec<String>,
    pub run_at: RunAt,
    pub all_frames: bool,
    pub match_about_blank: bool,
    /// "ISOLATED" (default) or "MAIN".
    pub world: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Action {
    pub default_title: String,
    pub default_popup: String,
    /// size -> path
    pub default_icon: BTreeMap<u32, String>,
    /// "navbar" | "menupanel" | "tabstrip" | "personaltoolbar"
    pub default_area: String,
    pub show_matches: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub shortcut: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Background {
    pub scripts: Vec<String>,
    pub page: Option<String>,
    pub is_module: bool,
    pub persistent: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Manifest {
    #[serde(skip)]
    pub raw: Value,
    pub manifest_version: u32,
    pub name: String,
    pub short_name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub homepage_url: String,
    pub gecko_id: Option<String>,
    pub default_locale: Option<String>,
    pub icons: BTreeMap<u32, String>,
    pub background: Option<Background>,
    pub content_scripts: Vec<ContentScript>,
    /// MV3 `action` or MV2 `browser_action`.
    pub action: Option<Action>,
    pub page_action: Option<Action>,
    pub sidebar_action: Option<Action>,
    pub options_page: Option<String>,
    pub options_open_in_tab: bool,
    pub newtab_override: Option<String>,
    pub permissions: Vec<String>,
    pub optional_permissions: Vec<String>,
    pub host_permissions: Vec<String>,
    pub commands: Vec<Command>,
    pub web_accessible_resources: Vec<String>,
    pub omnibox_keyword: Option<String>,
    pub dnr_rulesets: Vec<(String, bool, String)>,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_owned()
}
fn strs(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}
fn icon_map(v: Option<&Value>) -> BTreeMap<u32, String> {
    let mut m = BTreeMap::new();
    match v {
        Some(Value::String(p)) => {
            m.insert(32, p.clone());
        }
        Some(Value::Object(o)) => {
            for (k, v) in o {
                if let (Ok(size), Some(p)) = (k.parse::<u32>(), v.as_str()) {
                    m.insert(size, p.to_owned());
                }
            }
        }
        _ => {}
    }
    m
}

fn action(v: &Value) -> Action {
    Action {
        default_title: s(v, "default_title"),
        default_popup: s(v, "default_popup"),
        default_icon: icon_map(v.get("default_icon")),
        default_area: s(v, "default_area"),
        show_matches: strs(v.get("show_matches")),
    }
}

/// Normalise a resource path from the manifest ("/popup.html", "./a.js").
pub fn norm_path(p: &str) -> String {
    p.trim_start_matches("./").trim_start_matches('/').to_owned()
}

impl Manifest {
    /// Parse raw manifest text. `messages` resolves `__MSG_*__` strings.
    pub fn parse(text: &str, messages: Option<&Messages>) -> anyhow::Result<Self> {
        let text = text.trim_start_matches('\u{feff}');
        let raw: Value = serde_json::from_str(text)
            .or_else(|_| serde_json::from_str(&super::i18n::strip_comments(text)))
            .map_err(|e| anyhow::anyhow!("manifest.json is not valid JSON: {e}"))?;
        if !raw.is_object() {
            anyhow::bail!("manifest.json must be an object");
        }
        let mv = raw.get("manifest_version").and_then(Value::as_u64).unwrap_or(2) as u32;
        if !(2..=3).contains(&mv) {
            anyhow::bail!("unsupported manifest_version {mv}");
        }
        let gecko = raw
            .get("browser_specific_settings")
            .or_else(|| raw.get("applications"))
            .and_then(|b| b.get("gecko"));
        let gecko_id = gecko.and_then(|g| g.get("id")).and_then(Value::as_str).map(str::to_owned);

        let loc = |x: String| match messages {
            Some(m) => m.localize(&x, gecko_id.as_deref().unwrap_or("")),
            None => x,
        };

        let name = loc(s(&raw, "name"));
        if name.trim().is_empty() {
            anyhow::bail!("manifest.json has no name");
        }
        let version = s(&raw, "version");
        if version.is_empty() {
            anyhow::bail!("manifest.json has no version");
        }

        let background = raw.get("background").map(|b| Background {
            scripts: strs(b.get("scripts")).into_iter().chain(strs(b.get("service_worker"))).map(|p| norm_path(&p)).collect(),
            page: b.get("page").and_then(Value::as_str).map(norm_path),
            is_module: s(b, "type") == "module",
            persistent: b.get("persistent").and_then(Value::as_bool).unwrap_or(mv == 2),
        });

        let content_scripts = match raw.get("content_scripts") {
            Some(Value::Array(a)) => a
                .iter()
                .map(|c| ContentScript {
                    matches: strs(c.get("matches")),
                    exclude_matches: strs(c.get("exclude_matches")),
                    include_globs: strs(c.get("include_globs")),
                    exclude_globs: strs(c.get("exclude_globs")),
                    js: strs(c.get("js")).iter().map(|p| norm_path(p)).collect(),
                    css: strs(c.get("css")).iter().map(|p| norm_path(p)).collect(),
                    run_at: match c.get("run_at").and_then(Value::as_str) {
                        Some("document_start") => RunAt::DocumentStart,
                        Some("document_end") => RunAt::DocumentEnd,
                        _ => RunAt::DocumentIdle,
                    },
                    all_frames: c.get("all_frames").and_then(Value::as_bool).unwrap_or(false),
                    match_about_blank: c.get("match_about_blank").and_then(Value::as_bool).unwrap_or(false),
                    world: c.get("world").and_then(Value::as_str).unwrap_or("ISOLATED").to_owned(),
                })
                .collect(),
            _ => vec![],
        };

        let mut act = raw.get("action").or_else(|| raw.get("browser_action")).map(action);
        if let Some(a) = act.as_mut() {
            a.default_title = loc(a.default_title.clone());
            a.default_popup = norm_path(&a.default_popup);
        }
        let mut page_action = raw.get("page_action").map(action);
        if let Some(a) = page_action.as_mut() {
            a.default_title = loc(a.default_title.clone());
            a.default_popup = norm_path(&a.default_popup);
        }
        let sidebar_action = raw.get("sidebar_action").map(|v| {
            let mut a = action(v);
            a.default_popup = norm_path(&s(v, "default_panel"));
            a.default_title = loc(a.default_title.clone());
            a
        });

        let (options_page, options_open_in_tab) = if let Some(o) = raw.get("options_ui") {
            (o.get("page").and_then(Value::as_str).map(norm_path), o.get("open_in_tab").and_then(Value::as_bool).unwrap_or(false))
        } else {
            (raw.get("options_page").and_then(Value::as_str).map(norm_path), true)
        };

        // MV2 mixes host patterns into "permissions"; split them out.
        let mut permissions = vec![];
        let mut host_permissions = strs(raw.get("host_permissions"));
        for p in strs(raw.get("permissions")) {
            if is_host_pattern(&p) {
                host_permissions.push(p);
            } else {
                permissions.push(p);
            }
        }
        let mut optional_permissions = strs(raw.get("optional_permissions"));
        optional_permissions.extend(strs(raw.get("optional_host_permissions")));

        let commands = match raw.get("commands") {
            Some(Value::Object(o)) => o
                .iter()
                .map(|(name, c)| {
                    let key = c.get("suggested_key");
                    let shortcut = key
                        .and_then(|k| k.get("linux").or_else(|| k.get("default")))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    Command { name: name.clone(), description: loc(s(c, "description")), shortcut }
                })
                .collect(),
            _ => vec![],
        };

        let web_accessible_resources = match raw.get("web_accessible_resources") {
            Some(Value::Array(a)) => a
                .iter()
                .flat_map(|e| match e {
                    Value::String(s) => vec![s.clone()],
                    Value::Object(_) => strs(e.get("resources")),
                    _ => vec![],
                })
                .collect(),
            _ => vec![],
        };

        let dnr_rulesets = raw
            .get("declarative_net_request")
            .and_then(|d| d.get("rule_resources"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|r| (s(r, "id"), r.get("enabled").and_then(Value::as_bool).unwrap_or(true), norm_path(&s(r, "path"))))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Manifest {
            manifest_version: mv,
            short_name: loc(s(&raw, "short_name")),
            description: loc(s(&raw, "description")),
            author: match raw.get("author") {
                Some(Value::String(a)) => a.clone(),
                Some(Value::Object(o)) => o.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
                _ => String::new(),
            },
            homepage_url: s(&raw, "homepage_url"),
            default_locale: raw.get("default_locale").and_then(Value::as_str).map(str::to_owned),
            icons: icon_map(raw.get("icons")),
            background,
            content_scripts,
            action: act,
            page_action,
            sidebar_action,
            options_page,
            options_open_in_tab,
            newtab_override: raw
                .get("chrome_url_overrides")
                .and_then(|o| o.get("newtab"))
                .and_then(Value::as_str)
                .map(norm_path),
            permissions,
            optional_permissions,
            host_permissions,
            commands,
            web_accessible_resources,
            omnibox_keyword: raw.get("omnibox").and_then(|o| o.get("keyword")).and_then(Value::as_str).map(str::to_owned),
            dnr_rulesets,
            gecko_id,
            name,
            version,
            raw,
        })
    }

    /// Best icon path for a requested pixel size.
    pub fn icon_for(icons: &BTreeMap<u32, String>, size: u32) -> Option<String> {
        icons
            .iter()
            .find(|(s, _)| **s >= size)
            .or_else(|| icons.iter().next_back())
            .map(|(_, p)| norm_path(p))
    }

    pub fn has_permission(&self, p: &str) -> bool {
        self.permissions.iter().any(|x| x == p)
    }

    /// The manifest as returned by runtime.getManifest(): raw JSON with
    /// localized top-level strings.
    pub fn runtime_manifest(&self) -> Value {
        let mut v = self.raw.clone();
        if let Some(o) = v.as_object_mut() {
            o.insert("name".into(), self.name.clone().into());
            o.insert("description".into(), self.description.clone().into());
        }
        v
    }

    /// Human readable permission warnings, shown before install (like Firefox).
    pub fn permission_warnings(&self) -> Vec<String> {
        permission_warnings(&self.permissions, &self.host_permissions)
    }
}

pub fn permission_warnings(perms: &[String], hosts: &[String]) -> Vec<String> {
    let mut out = vec![];
    let all_hosts = hosts.iter().any(|h| h == "<all_urls>" || h.starts_with("*://*/") || h == "http://*/*" || h == "https://*/*");
    if all_hosts {
        out.push("Access your data for all websites".to_owned());
    } else {
        let mut domains: Vec<String> = hosts
            .iter()
            .filter_map(|h| h.split("://").nth(1).map(|r| r.split('/').next().unwrap_or("").trim_start_matches("*.").to_owned()))
            .filter(|h| !h.is_empty())
            .collect();
        domains.sort();
        domains.dedup();
        if !domains.is_empty() {
            out.push(format!("Access your data for sites in: {}", domains.join(", ")));
        }
    }
    let table: &[(&str, &str)] = &[
        ("bookmarks", "Read and modify bookmarks"),
        ("browsingData", "Clear recent browsing history, cookies, and related data"),
        ("clipboardRead", "Get data from the clipboard"),
        ("clipboardWrite", "Input data to the clipboard"),
        ("declarativeNetRequest", "Block content on any page"),
        ("downloads", "Download files and read and modify the browser's download history"),
        ("downloads.open", "Open files downloaded to your computer"),
        ("geolocation", "Access your location"),
        ("history", "Access browsing history"),
        ("management", "Monitor extension usage and manage themes"),
        ("nativeMessaging", "Exchange messages with programs other than Kite"),
        ("notifications", "Display notifications to you"),
        ("privacy", "Read and modify privacy settings"),
        ("proxy", "Control browser proxy settings"),
        ("sessions", "Access recently closed tabs"),
        ("tabs", "Access browser tabs"),
        ("topSites", "Access browsing history"),
        ("webNavigation", "Access browser activity during navigation"),
        ("cookies", "Read and modify cookies"),
    ];
    for p in perms {
        if let Some((_, w)) = table.iter().find(|(k, _)| k == p) {
            if !out.iter().any(|o| o == w) {
                out.push((*w).to_owned());
            }
        }
    }
    out
}

/// Keys in the raw manifest object (for diagnostics in the add-on details page).
pub fn unsupported_keys(raw: &Map<String, Value>) -> Vec<String> {
    const KNOWN: &[&str] = &[
        "manifest_version", "name", "short_name", "version", "version_name", "description", "author", "homepage_url",
        "developer", "browser_specific_settings", "applications", "default_locale", "icons", "background",
        "content_scripts", "action", "browser_action", "page_action", "sidebar_action", "options_ui", "options_page",
        "chrome_url_overrides", "permissions", "optional_permissions", "host_permissions", "optional_host_permissions",
        "commands", "web_accessible_resources", "omnibox", "content_security_policy", "incognito",
        "declarative_net_request", "minimum_chrome_version", "offline_enabled", "key", "update_url", "storage",
        "browser_url_overrides", "short_name", "externally_connectable",
    ];
    raw.keys().filter(|k| !KNOWN.contains(&k.as_str())).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mv2_firefox() {
        let m = Manifest::parse(
            r#"{"manifest_version":2,"name":"X","version":"1.2",
                "browser_specific_settings":{"gecko":{"id":"x@example.org"}},
                "permissions":["tabs","storage","<all_urls>","*://*.kde.org/*"],
                "browser_action":{"default_popup":"/popup/p.html","default_icon":{"16":"i16.png","48":"i48.png"}},
                "background":{"scripts":["bg.js"]},
                "content_scripts":[{"matches":["<all_urls>"],"js":["./cs.js"],"run_at":"document_start"}],
                "commands":{"_execute_browser_action":{"suggested_key":{"default":"Ctrl+Shift+Y"}}}}"#,
            None,
        )
        .unwrap();
        assert_eq!(m.gecko_id.as_deref(), Some("x@example.org"));
        assert_eq!(m.permissions, vec!["tabs", "storage"]);
        assert_eq!(m.host_permissions.len(), 2);
        assert_eq!(m.action.as_ref().unwrap().default_popup, "popup/p.html");
        assert_eq!(Manifest::icon_for(&m.action.as_ref().unwrap().default_icon, 32).unwrap(), "i48.png");
        assert_eq!(m.content_scripts[0].js, vec!["cs.js"]);
        assert_eq!(m.content_scripts[0].run_at, RunAt::DocumentStart);
        assert!(m.background.as_ref().unwrap().persistent);
        assert_eq!(m.commands[0].shortcut, "Ctrl+Shift+Y");
        assert_eq!(m.permission_warnings()[0], "Access your data for all websites");
    }
    #[test]
    fn mv3_and_errors() {
        let m = Manifest::parse(r#"{"manifest_version":3,"name":"Y","version":"1","action":{},"background":{"service_worker":"sw.js","type":"module"},"host_permissions":["https://a.test/*"]}"#, None).unwrap();
        assert_eq!(m.background.as_ref().unwrap().scripts, vec!["sw.js"]);
        assert!(m.background.as_ref().unwrap().is_module);
        assert_eq!(m.permission_warnings(), vec!["Access your data for sites in: a.test"]);
        assert!(Manifest::parse(r#"{"manifest_version":2,"version":"1"}"#, None).is_err());
        assert!(Manifest::parse("nope", None).is_err());
    }
}
