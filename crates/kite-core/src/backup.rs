//! A file with the user's settings, to carry to another machine (Settings → General):
//! preferences, search engines and per-site settings. Not what belongs to this
//! machine (window size, download folder, start page files), nor the Safe Browsing
//! API key, as such a file may get passed around. Bookmarks and logins have their own
//! exports; history and cookies stay where they are.

use crate::search::{self, SearchEngine};
use crate::settings::{defaults, Settings};
use crate::sitesettings::{self, SiteSettings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// The format's version; also what tells this kind of file from others.
    pub kite_settings: u32,
    /// Those differing from the defaults.
    #[serde(default)]
    pub settings: Map<String, Value>,
    #[serde(default)]
    pub search_engines: Vec<SearchEngine>,
    #[serde(default)]
    pub site_settings: Vec<Site>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Site {
    pub host: String,
    #[serde(flatten)]
    pub settings: SiteSettings,
}

/// What stays out of such a file, going out and coming in.
fn belongs_here(key: &str, value: &Value) -> bool {
    key.starts_with("window.")
        || key.starts_with("startpage.")
        || key == "downloads.directory"
        || key == "browsing.safeBrowsingKey"
        // A page from a file: the file isn't coming along.
        || (matches!(key, "general.homepageMode" | "general.newTabPageMode") && value == "file")
}

pub fn export(settings: &Settings, db: &Connection) -> anyhow::Result<Backup> {
    let defs = defaults();
    let changed = settings.all().iter().filter(|(k, v)| !belongs_here(k, v) && !v.is_null() && defs.get(*k) != Some(*v));
    Ok(Backup {
        kite_settings: VERSION,
        settings: changed.map(|(k, v)| (k.clone(), v.clone())).collect(),
        search_engines: search::load_engines(db)?,
        site_settings: sitesettings::all(db)?.into_iter().filter(|(_, s)| !s.is_empty()).map(|(host, settings)| Site { host, settings }).collect(),
    })
}

/// A file's contents, if it's one of these. Settings of the wrong kind for what they
/// are (a text where a number belongs) are dropped, and so is what belongs elsewhere.
pub fn parse(text: &str) -> anyhow::Result<Backup> {
    let value: Value = serde_json::from_str(text).map_err(|_| anyhow::anyhow!("This isn't a Kite settings file."))?;
    let kind = value.get("kiteSettings").and_then(Value::as_u64).ok_or_else(|| anyhow::anyhow!("This isn't a Kite settings file."))?;
    anyhow::ensure!(kind <= VERSION as u64, "This settings file is from a newer version of Kite.");
    let mut backup: Backup = serde_json::from_value(value).map_err(|e| anyhow::anyhow!("This settings file is damaged: {e}"))?;
    let defs = defaults();
    backup.settings.retain(|k, v| {
        !belongs_here(k, v) && !v.is_null() && defs.get(k).is_none_or(|d| std::mem::discriminant(d) == std::mem::discriminant(v))
    });
    Ok(backup)
}

/// The settings to change so that they're as in the file: what isn't there goes back
/// to its default. What belongs to this machine stays.
pub fn changes(current: &Map<String, Value>, backup: &Backup) -> Vec<(String, Value)> {
    let defs = defaults();
    let mut keys: Vec<&String> = current.keys().chain(defs.keys()).chain(backup.settings.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|k| {
            let now = current.get(k).unwrap_or(&Value::Null);
            let new = backup.settings.get(k).or_else(|| defs.get(k)).unwrap_or(&Value::Null);
            (now != new && !belongs_here(k, now) && !belongs_here(k, new)).then(|| (k.clone(), new.clone()))
        })
        .collect()
}

/// The search engines and sites as in the file.
pub fn apply_to_db(db: &Connection, backup: &Backup) -> anyhow::Result<()> {
    if !backup.search_engines.is_empty() {
        search::save_engines(db, &backup.search_engines)?;
    }
    for (host, _) in sitesettings::all(db)? {
        sitesettings::clear(db, &host)?;
    }
    for site in &backup.site_settings {
        sitesettings::store(db, &site.host, &site.settings)?;
    }
    Ok(())
}

/// `kite --factory`: back to the default settings, after saving the current ones. Gives
/// the file to import to get them back (Settings → General), and an untouched copy of
/// the original beside it (what an import leaves out, or a file too broken to import),
/// or None if nothing had been changed. Only settings: bookmarks, logins, extensions and
/// the like stay. Nothing is touched if they can't be saved.
pub fn factory_reset(profile: &crate::Profile) -> anyhow::Result<Option<(std::path::PathBuf, std::path::PathBuf)>> {
    let current = profile.path("settings.json");
    if !current.exists() {
        return Ok(None);
    }
    let names = |n: u32| {
        let number = if n == 1 { String::new() } else { format!("-{n}") };
        (profile.path(&format!("settings-before-factory{number}.json")), profile.path(&format!("settings-before-factory{number}.original")))
    };
    let (import, original) = (1..).map(names).find(|(a, b)| !a.exists() && !b.exists()).expect("a free name");
    std::fs::copy(&current, &original)?;
    // (With the search engines and per-site settings, as importing replaces those too.)
    let db = crate::db::open(&profile.path("browsedata.db"))?;
    let backup = export(&Settings::load(current.clone()), &db)?;
    std::fs::write(&import, serde_json::to_string_pretty(&backup)?)?;
    std::fs::remove_file(&current)?;
    Ok(Some((import, original)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn factory_saves_and_starts_over() {
        let t = tempfile::tempdir().unwrap();
        let profile = crate::Profile::open_at(t.path().join("p"), "p").unwrap();
        assert_eq!(factory_reset(&profile).unwrap(), None, "nothing changed yet");
        let path = profile.path("settings.json");
        let mut s = Settings::load(path.clone());
        s.set("general.startup", json!("blank")).unwrap();
        s.set("plugins.vim-keys.enabled", json!(true)).unwrap();
        s.set("downloads.directory", json!("/somewhere")).unwrap();
        let (import, original) = factory_reset(&profile).unwrap().unwrap();
        assert!(!path.exists());
        assert_eq!(Settings::load(path.clone()).get("general.startup"), defaults()["general.startup"]);
        // The way back.
        let back = parse(&std::fs::read_to_string(&import).unwrap()).unwrap();
        assert_eq!(back.settings["general.startup"], json!("blank"));
        assert_eq!(back.settings["plugins.vim-keys.enabled"], json!(true));
        assert!(std::fs::read_to_string(&original).unwrap().contains("/somewhere"), "all of it");
        // A second time doesn't overwrite the first.
        Settings::load(path.clone()).set("general.startup", json!("homepage")).unwrap();
        let (second, _) = factory_reset(&profile).unwrap().unwrap();
        assert_ne!(second, import);
        assert_eq!(parse(&std::fs::read_to_string(&import).unwrap()).unwrap().settings["general.startup"], json!("blank"));
    }

    #[test]
    fn factory_copes_with_a_file_that_isnt_settings() {
        let t = tempfile::tempdir().unwrap();
        let profile = crate::Profile::open_at(t.path().join("p"), "p").unwrap();
        std::fs::write(profile.path("settings.json"), "{ this is broken").unwrap();
        let (_, original) = factory_reset(&profile).unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(original).unwrap(), "{ this is broken");
        assert!(!profile.path("settings.json").exists());
    }

    #[test]
    fn importing_what_factory_saved_drops_what_broke_it() {
        let t = tempfile::tempdir().unwrap();
        let profile = crate::Profile::open_at(t.path().join("p"), "p").unwrap();
        let mut s = Settings::load(profile.path("settings.json"));
        s.set("appearance.defaultZoom", json!("huge")).unwrap();
        s.set("general.startup", json!("blank")).unwrap();
        let (import, _) = factory_reset(&profile).unwrap().unwrap();
        let back = parse(&std::fs::read_to_string(import).unwrap()).unwrap();
        assert!(!back.settings.contains_key("appearance.defaultZoom"), "the wrong kind");
        assert_eq!(back.settings["general.startup"], json!("blank"));
    }

    fn settings(dir: &tempfile::TempDir) -> Settings {
        Settings::load(dir.path().join("settings.json"))
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut from = settings(&dir);
        from.set("appearance.defaultZoom", json!(125)).unwrap();
        from.set("shortcuts", json!({"newTab": ["Ctrl+Y"]})).unwrap();
        from.set("plugins.vim-keys.enabled", json!(true)).unwrap();
        let db = crate::db::open_in_memory().unwrap();
        search::save_engines(&db, &[SearchEngine { name: "Mine".into(), url: "https://mine.example/?q=%s".into(), shortcut: "m".into(), suggestions_url: String::new(), post_data: String::new() }]).unwrap();
        sitesettings::set(&db, "a.example", "zoom", 150).unwrap();
        sitesettings::set_text(&db, "b.example", "css", "body{}").unwrap();
        let text = serde_json::to_string(&export(&from, &db).unwrap()).unwrap();

        let back = parse(&text).unwrap();
        assert_eq!(back.settings["appearance.defaultZoom"], json!(125));
        assert_eq!(back.settings["plugins.vim-keys.enabled"], json!(true));
        assert!(!back.settings.contains_key("appearance.showStatusBar"), "only what differs");

        let other = crate::db::open_in_memory().unwrap();
        sitesettings::set(&other, "old.example", "zoom", 90).unwrap();
        apply_to_db(&other, &back).unwrap();
        assert_eq!(search::load_engines(&other).unwrap()[0].name, "Mine");
        assert_eq!(sitesettings::get(&other, "a.example").unwrap().zoom, 150);
        assert_eq!(sitesettings::get(&other, "b.example").unwrap().css, "body{}");
        assert_eq!(sitesettings::all(&other).unwrap().len(), 2, "what was there is replaced");
    }

    #[test]
    fn what_belongs_to_this_machine_stays_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(&dir);
        for (k, v) in [("window.width", json!(900)), ("downloads.directory", json!("/home/me/x")), ("browsing.safeBrowsingKey", json!("SECRET")), ("startpage.home.file", json!("/a.html")), ("general.homepageMode", json!("file"))] {
            s.set(k, v).unwrap();
        }
        let db = crate::db::open_in_memory().unwrap();
        let text = serde_json::to_string(&export(&s, &db).unwrap()).unwrap();
        for hidden in ["SECRET", "/home/me/x", "window.width", "/a.html", "homepageMode"] {
            assert!(!text.contains(hidden), "{hidden}");
        }
        // Nor do they get in from a file someone made.
        let b = parse(r#"{"kiteSettings": 1, "settings": {"browsing.safeBrowsingKey": "THEIRS", "window.width": 1, "downloads.directory": "/tmp/evil"}}"#).unwrap();
        assert!(b.settings.is_empty());
    }

    #[test]
    fn importing_leaves_this_machines_own_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(&dir);
        s.set("browsing.safeBrowsingKey", json!("MINE")).unwrap();
        s.set("window.width", json!(700)).unwrap();
        s.set("appearance.defaultZoom", json!(150)).unwrap();
        s.set("plugins.gestures.enabled", json!(true)).unwrap();
        let b = parse(r#"{"kiteSettings": 1, "settings": {"tabs.tree": true}}"#).unwrap();
        let c = changes(s.all(), &b);
        let of = |k: &str| c.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        assert_eq!(of("tabs.tree"), Some(json!(true)));
        assert_eq!(of("appearance.defaultZoom"), Some(json!(100)), "back to the default");
        assert_eq!(of("plugins.gestures.enabled"), Some(Value::Null), "no default: unset");
        assert_eq!(of("browsing.safeBrowsingKey"), None);
        assert_eq!(of("window.width"), None);
        assert_eq!(of("appearance.showStatusBar"), None, "unchanged");
    }

    #[test]
    fn other_files_and_newer_versions_are_refused() {
        for bad in ["", "not json", "[]", "{}", r#"{"root": {}}"#, r#"{"kiteSettings": "1"}"#] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse(r#"{"kiteSettings": 2}"#).unwrap_err().to_string().contains("newer"));
        assert!(parse(r#"{"kiteSettings": 1, "searchEngines": 5}"#).is_err());
    }

    #[test]
    fn settings_of_the_wrong_kind_are_dropped() {
        let b = parse(r#"{"kiteSettings": 1, "settings": {"appearance.defaultZoom": "huge", "tabs.tree": 1, "web.javascript": false, "plugins.x.enabled": true, "tabs.position": null}}"#).unwrap();
        let mut keys: Vec<_> = b.settings.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["plugins.x.enabled", "web.javascript"]);
    }
}
