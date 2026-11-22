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
        // Whether this profile has seen its welcome page.
        || key == "general.welcome"
        // A program on this machine, started with every download: like the folder they
        // go to, and nothing a file from someone else may name.
        || key.starts_with("downloads.external")
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
    // Addresses Kite goes to by itself are the web's or its own pages: a `javascript:`
    // one would run in whatever page is open when Home is pressed or a search is made.
    let goes_somewhere = |v: &Value, kite: bool| v.as_str().is_none_or(|u| is_web(u) || (kite && u.starts_with("kite:")));
    backup.settings.retain(|k, v| !matches!(k.as_str(), "general.homepage" | "general.newTabPage") || goes_somewhere(v, true));
    backup.search_engines.retain(|e| is_web(&e.url) && (e.suggestions_url.is_empty() || is_web(&e.suggestions_url)));
    Ok(backup)
}

fn is_web(address: &str) -> bool {
    url::Url::parse(address).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// What a file would do that its numbers don't say, for the question before it's
/// taken: what somebody else's file could use against the one importing it.
pub fn cautions(backup: &Backup) -> Vec<String> {
    let text = |key: &str| backup.settings.get(key).and_then(Value::as_str).unwrap_or_default();
    let mut out = vec![];
    if text("privacy.proxy") == "manual" && !text("privacy.proxyHost").is_empty() {
        let port = backup.settings.get("privacy.proxyPort").and_then(Value::as_i64).map(|p| format!(":{p}")).unwrap_or_default();
        out.push(format!("It sends everything you browse through the proxy {}{port}.", text("privacy.proxyHost")));
    }
    if matches!(text("privacy.dnsOverHttps"), "auto" | "secure") && !text("privacy.dnsServer").is_empty() {
        out.push(format!("It looks up the names of the sites you visit at {}.", text("privacy.dnsServer")));
    }
    let plugins: Vec<&str> = backup.settings.iter().filter(|(_, v)| **v == Value::Bool(true)).filter_map(|(k, _)| k.strip_prefix("plugins.")?.strip_suffix(".enabled")).collect();
    if !plugins.is_empty() {
        out.push(format!("It turns on these plugins, if they are installed: {}.", plugins.join(", ")));
    }
    let granted: Vec<&str> = backup.site_settings.iter().filter(|s| [s.settings.notifications, s.settings.geolocation, s.settings.midi, s.settings.midi_sysex].contains(&1) || !s.settings.handlers.is_empty()).map(|s| s.host.as_str()).collect();
    if !granted.is_empty() {
        out.push(format!("It lets these sites send notifications, see your location, use MIDI devices or open links of some kinds: {}.", granted.join(", ")));
    }
    out
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
    fn a_file_from_someone_else_names_no_program_and_no_script_address() {
        let b = parse(
            r#"{"kiteSettings": 1,
                "settings": {"downloads.external": true, "downloads.externalProgram": "sh", "downloads.externalArguments": "-c x",
                             "general.homepage": "javascript:alert(1)", "general.newTabPage": "kite:start", "appearance.defaultZoom": 110},
                "searchEngines": [{"name": "Bad", "url": "javascript:alert('%s')", "shortcut": "b", "suggestions_url": "", "post_data": ""},
                                  {"name": "Good", "url": "https://good.example/?q=%s", "shortcut": "g", "suggestions_url": "", "post_data": ""}]}"#,
        )
        .unwrap();
        let mut kept: Vec<&String> = b.settings.keys().collect();
        kept.sort();
        assert_eq!(kept, ["appearance.defaultZoom", "general.newTabPage"]);
        assert_eq!(b.search_engines.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Good"]);
        // And importing leaves this machine's program as it is.
        let dir = tempfile::tempdir().unwrap();
        let mut s = settings(&dir);
        s.set("downloads.externalProgram", json!("/usr/bin/mine")).unwrap();
        assert!(changes(s.all(), &b).iter().all(|(k, _)| !k.starts_with("downloads.external")));
    }

    #[test]
    fn says_what_a_file_would_do_beyond_its_numbers() {
        let quiet = parse(r#"{"kiteSettings": 1, "settings": {"appearance.defaultZoom": 110}, "siteSettings": [{"host": "a.example", "zoom": 150}]}"#).unwrap();
        assert!(cautions(&quiet).is_empty());
        let loud = parse(
            r#"{"kiteSettings": 1,
                "settings": {"privacy.proxy": "manual", "privacy.proxyHost": "proxy.example", "privacy.proxyPort": 9050,
                             "privacy.dnsOverHttps": "secure", "privacy.dnsServer": "https://dns.example/q", "plugins.some-plugin.enabled": true, "plugins.other.enabled": false},
                "siteSettings": [{"host": "push.example", "notifications": 1}, {"host": "calm.example", "notifications": 2}]}"#,
        )
        .unwrap();
        let c = cautions(&loud).join("\n");
        for said in ["proxy.example:9050", "https://dns.example/q", "some-plugin", "push.example"] {
            assert!(c.contains(said), "{said}");
        }
        assert!(!c.contains("calm.example") && !c.contains("other"));
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
