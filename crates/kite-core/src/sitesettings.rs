//! Per-site settings (Falkon `site_settings` table). Kite currently uses the
//! zoom level and the cookie / JavaScript / image overrides.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SiteSettings {
    /// Zoom in percent, -1 = default.
    pub zoom: i64,
    /// 0 = default, 1 = allow, 2 = deny (Falkon's convention).
    pub cookies: i64,
    pub javascript: i64,
    pub images: i64,
    pub notifications: i64,
    pub geolocation: i64,
}

pub fn get(db: &Connection, host: &str) -> anyhow::Result<SiteSettings> {
    Ok(db
        .query_row(
            "SELECT zoom_level, allow_cookies, wa_js_enabled, wa_autoload_images, f_notifications, f_geolocation
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
];

pub fn set(db: &Connection, host: &str, key: &str, value: i64) -> anyhow::Result<()> {
    let col = COLUMNS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c).ok_or_else(|| anyhow::anyhow!("unknown site setting {key}"))?;
    db.execute("INSERT OR IGNORE INTO site_settings (server) VALUES (?1)", [host])?;
    db.execute(&format!("UPDATE site_settings SET {col} = ?2 WHERE server = ?1"), params![host, value])?;
    Ok(())
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
