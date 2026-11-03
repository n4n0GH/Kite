//! Site icons, cached locally for the speed dial so it never has to contact
//! sites (or trip over their certificates) just to draw a tile.
//!
//! Stored in Falkon's `icons` table, one PNG per site origin (url =
//! "https://host:port/"); Falkon looks icons up by domain, so the entries
//! work there too.

use rusqlite::{params, Connection, OptionalExtension};

/// Origin key for a page URL, e.g. "https://kde.org/". None for non-web URLs.
pub fn key(page_url: &str) -> Option<String> {
    let u = url::Url::parse(page_url).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| format!("{}/", u.origin().ascii_serialization()))
}

pub fn save(db: &Connection, page_url: &str, png: &[u8]) -> anyhow::Result<()> {
    let Some(k) = key(page_url) else { return Ok(()) };
    db.execute("INSERT INTO icons (url, icon) VALUES (?1, ?2) ON CONFLICT(url) DO UPDATE SET icon = excluded.icon", params![k, png])?;
    Ok(())
}

/// The icon for a page's site: its own origin first, else the same host
/// under another scheme or port.
pub fn for_url(db: &Connection, page_url: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(k) = key(page_url) else { return Ok(None) };
    if let Some(icon) = db.query_row("SELECT icon FROM icons WHERE url = ?1", [&k], |r| r.get(0)).optional()? {
        return Ok(Some(icon));
    }
    let host = url::Url::parse(page_url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
    Ok(db
        .query_row(
            "SELECT icon FROM icons WHERE url = ?1 OR url = ?2 OR url LIKE ?3 OR url LIKE ?4 ORDER BY id DESC LIMIT 1",
            params![format!("https://{host}/"), format!("http://{host}/"), format!("https://{host}:%"), format!("http://{host}:%")],
            |r| r.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_per_origin_and_falls_back_to_host() {
        let db = crate::db::open_in_memory().unwrap();
        assert_eq!(key("kite:start"), None);
        assert_eq!(key("https://kde.org/a/b?c#d").as_deref(), Some("https://kde.org/"));
        save(&db, "https://fritz.box/#/", b"one").unwrap();
        save(&db, "https://fritz.box/other", b"two").unwrap();
        assert_eq!(for_url(&db, "https://fritz.box/").unwrap().as_deref(), Some(&b"two"[..]));
        // http visit of a site whose icon was saved over https
        assert_eq!(for_url(&db, "http://fritz.box/x").unwrap().as_deref(), Some(&b"two"[..]));
        assert_eq!(for_url(&db, "https://kde.org/").unwrap(), None);
        assert_eq!(for_url(&db, "https://box/").unwrap(), None);
    }
}
