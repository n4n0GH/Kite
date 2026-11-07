//! Per-site settings (Falkon `site_settings` table). Kite uses the zoom level,
//! forced dark mode, the cookie / JavaScript / image overrides, and its own
//! custom CSS and JavaScript (`kite_css`, `kite_js`; see db.rs).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SiteSettings {
    /// Zoom in percent, -1 = default.
    pub zoom: i64,
    /// 0 = default, 1 = allow, 2 = deny (Falkon's convention).
    pub cookies: i64,
    pub javascript: i64,
    pub images: i64,
    pub notifications: i64,
    pub geolocation: i64,
    /// 0 = the global setting, 1 = force dark, 2 = force light.
    pub dark_mode: i64,
    /// Style sheet added to the site's pages.
    pub css: String,
    /// Script run in the site's pages (page world, once the DOM is ready).
    pub js: String,
    /// 1 = fingerprint protection on ("privacy.antiFingerprinting").
    pub fingerprint: i64,
    /// Web MIDI: 0 = ask, 1 = allow, 2 = block.
    pub midi: i64,
    /// Web MIDI with SysEx, which can change a device's settings and firmware.
    pub midi_sysex: i64,
}

impl SiteSettings {
    /// Whether anything differs from the defaults.
    pub fn is_empty(&self) -> bool {
        *self == SiteSettings { zoom: -1, ..Default::default() } || *self == SiteSettings::default()
    }
}

pub fn get(db: &Connection, host: &str) -> anyhow::Result<SiteSettings> {
    Ok(db
        .query_row(
            "SELECT zoom_level, allow_cookies, wa_js_enabled, wa_autoload_images, f_notifications, f_geolocation,
                    wa_force_dark_mode, kite_css, kite_js, kite_fp, kite_midi, kite_midi_sysex
             FROM site_settings WHERE server = ?1",
            [host],
            |r| {
                Ok(SiteSettings {
                    zoom: r.get(0)?,
                    cookies: r.get(1)?,
                    javascript: r.get(2)?,
                    images: r.get(3)?,
                    notifications: r.get(4)?,
                    geolocation: r.get(5)?,
                    dark_mode: r.get(6)?,
                    css: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    js: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                    fingerprint: r.get::<_, Option<i64>>(9)?.unwrap_or_default(),
                    midi: r.get::<_, Option<i64>>(10)?.unwrap_or_default(),
                    midi_sysex: r.get::<_, Option<i64>>(11)?.unwrap_or_default(),
                })
            },
        )
        .optional()?
        .unwrap_or(SiteSettings { zoom: -1, ..Default::default() }))
}

const COLUMNS: &[(&str, &str)] = &[
    ("zoom", "zoom_level"),
    ("cookies", "allow_cookies"),
    ("javascript", "wa_js_enabled"),
    ("images", "wa_autoload_images"),
    ("notifications", "f_notifications"),
    ("geolocation", "f_geolocation"),
    ("darkMode", "wa_force_dark_mode"),
    ("fingerprint", "kite_fp"),
    ("midi", "kite_midi"),
    ("midiSysex", "kite_midi_sysex"),
];

const TEXT_COLUMNS: &[(&str, &str)] = &[("css", "kite_css"), ("js", "kite_js")];

pub fn set(db: &Connection, host: &str, key: &str, value: i64) -> anyhow::Result<()> {
    let col = COLUMNS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c).ok_or_else(|| anyhow::anyhow!("unknown site setting {key}"))?;
    db.execute("INSERT OR IGNORE INTO site_settings (server) VALUES (?1)", [host])?;
    db.execute(&format!("UPDATE site_settings SET {col} = ?2 WHERE server = ?1"), params![host, value])?;
    Ok(())
}

pub fn set_text(db: &Connection, host: &str, key: &str, value: &str) -> anyhow::Result<()> {
    let col = TEXT_COLUMNS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c).ok_or_else(|| anyhow::anyhow!("unknown site setting {key}"))?;
    db.execute("INSERT OR IGNORE INTO site_settings (server) VALUES (?1)", [host])?;
    db.execute(&format!("UPDATE site_settings SET {col} = ?2 WHERE server = ?1"), params![host, value])?;
    Ok(())
}

/// All of a site's settings at once, in place of what it had.
pub fn store(db: &Connection, host: &str, s: &SiteSettings) -> anyhow::Result<()> {
    clear(db, host)?;
    for (key, value) in [("zoom", s.zoom), ("cookies", s.cookies), ("javascript", s.javascript), ("images", s.images), ("notifications", s.notifications), ("geolocation", s.geolocation), ("darkMode", s.dark_mode), ("fingerprint", s.fingerprint), ("midi", s.midi), ("midiSysex", s.midi_sysex)] {
        set(db, host, key, value)?;
    }
    set_text(db, host, "css", &s.css)?;
    set_text(db, host, "js", &s.js)
}

pub fn all(db: &Connection) -> anyhow::Result<Vec<(String, SiteSettings)>> {
    let mut stmt = db.prepare("SELECT server FROM site_settings ORDER BY server")?;
    let hosts: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    hosts.into_iter().map(|h| get(db, &h).map(|s| (h, s))).collect()
}

pub fn clear(db: &Connection, host: &str) -> anyhow::Result<()> {
    db.execute("DELETE FROM site_settings WHERE server = ?1", [host])?;
    Ok(())
}

/// Back to the defaults, except for custom CSS and JavaScript and fingerprint
/// protection (chosen by the user, not picked up while browsing).
pub fn reset(db: &Connection, host: &str) -> anyhow::Result<()> {
    let s = get(db, host)?;
    clear(db, host)?;
    if !s.css.is_empty() {
        set_text(db, host, "css", &s.css)?;
    }
    if !s.js.is_empty() {
        set_text(db, host, "js", &s.js)?;
    }
    if s.fingerprint != 0 {
        set(db, host, "fingerprint", s.fingerprint)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_decisions_are_kept_and_reset() {
        let db = crate::db::open_in_memory().unwrap();
        assert_eq!(get(&db, "a.org").unwrap().midi, 0, "asks");
        set(&db, "a.org", "midi", 1).unwrap();
        set(&db, "a.org", "midiSysex", 2).unwrap();
        let s = get(&db, "a.org").unwrap();
        assert_eq!((s.midi, s.midi_sysex), (1, 2));
        assert_eq!(get(&db, "b.org").unwrap().midi, 0, "for that site only");
        assert!(!s.is_empty());
        store(&db, "c.org", &s).unwrap();
        assert_eq!(get(&db, "c.org").unwrap().midi_sysex, 2, "imports with the rest");
        reset(&db, "a.org").unwrap();
        let s = get(&db, "a.org").unwrap();
        assert_eq!((s.midi, s.midi_sysex), (0, 0), "cleared with the other permissions");
    }

    #[test]
    fn dark_mode_css_js() {
        let db = crate::db::open_in_memory().unwrap();
        assert_eq!(get(&db, "a.org").unwrap(), SiteSettings { zoom: -1, ..Default::default() });
        set(&db, "a.org", "darkMode", 1).unwrap();
        set_text(&db, "a.org", "css", "body { color: red }").unwrap();
        set_text(&db, "a.org", "js", "console.log(1)").unwrap();
        let s = get(&db, "a.org").unwrap();
        assert_eq!((s.dark_mode, s.css.as_str(), s.js.as_str(), s.zoom), (1, "body { color: red }", "console.log(1)", -1));
        assert!(!s.is_empty());
        assert!(set_text(&db, "a.org", "zoom", "x").is_err());
        assert_eq!(all(&db).unwrap().len(), 1);
        set(&db, "a.org", "zoom", 150).unwrap();
        reset(&db, "a.org").unwrap();
        let s = get(&db, "a.org").unwrap();
        assert_eq!((s.zoom, s.dark_mode, s.css.as_str()), (-1, 0, "body { color: red }"));
        clear(&db, "a.org").unwrap();
        assert!(all(&db).unwrap().is_empty());
    }
}
