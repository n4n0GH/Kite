//! Per-site settings (Falkon `site_settings` table). Kite uses the zoom level,
//! forced dark mode, the cookie / JavaScript / image overrides, and its own
//! custom CSS and JavaScript (`kite_css`, `kite_js`) and user agent (`kite_ua`;
//! see db.rs).
//!
//! Also here: what was answered when a site asked to open links of a kind
//! (`navigator.registerProtocolHandler`: mailto:, magnet:, web+…), in
//! `kite_handlers`. QtWebEngine keeps neither answer past the session (its
//! registry of handlers is built without a place to save to), so Kite keeps
//! them, and takes such links to the allowed site itself (`link_target`).

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a site was told when it asked for links of a kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LinkHandler {
    /// 1 = allowed, 2 = blocked.
    pub decision: i64,
    /// The site's address for such a link, `%s` where the link goes.
    pub url: String,
    /// When that was decided (seconds since 1970). Of the sites allowed the
    /// same kind of link, the one allowed last gets them.
    pub at: i64,
}

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
    /// The user agent the site is sent and its pages see, empty = the one
    /// other sites get (`util::QTWEBENGINE_USER_AGENT` stands for Kite's own).
    pub user_agent: String,
    /// Links the site asked to open, by their scheme ("mailto"), and the answer.
    pub handlers: BTreeMap<String, LinkHandler>,
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
                    wa_force_dark_mode, kite_css, kite_js, kite_fp, kite_midi, kite_midi_sysex, kite_ua, kite_handlers
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
                    user_agent: r.get::<_, Option<String>>(12)?.unwrap_or_default(),
                    handlers: r.get::<_, Option<String>>(13)?.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default(),
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

const TEXT_COLUMNS: &[(&str, &str)] = &[("css", "kite_css"), ("js", "kite_js"), ("userAgent", "kite_ua")];

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
    set_text(db, host, "js", &s.js)?;
    set_text(db, host, "userAgent", &s.user_agent)?;
    store_handlers(db, host, &s.handlers)
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

/// Back to the defaults, except for custom CSS and JavaScript, fingerprint
/// protection and the user agent (chosen by the user, not picked up while browsing).
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
    if !s.user_agent.is_empty() {
        set_text(db, host, "userAgent", &s.user_agent)?;
    }
    Ok(())
}

/// Forget a site that has nothing of its own left (all back to the defaults),
/// so it isn't listed, or marked in the address bar, for nothing.
pub fn clear_if_empty(db: &Connection, host: &str) -> anyhow::Result<()> {
    if get(db, host)?.is_empty() {
        clear(db, host)?;
    }
    Ok(())
}

/// A zoom equal to the default zoom (percent) isn't a site's own: such ones,
/// stored before resetting stopped storing them, are forgotten, and with them
/// sites with nothing else of their own. How many sites changed.
pub fn forget_default_zoom(db: &Connection, default_zoom: i64) -> anyhow::Result<usize> {
    let hosts: Vec<String> = {
        let mut stmt = db.prepare("SELECT server FROM site_settings WHERE zoom_level = ?1")?;
        let rows = stmt.query_map([default_zoom], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for host in &hosts {
        set(db, host, "zoom", -1)?;
        clear_if_empty(db, host)?;
    }
    Ok(hosts.len())
}

/// The sites with a user agent of their own: host → what was chosen.
pub fn user_agents(db: &Connection) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let mut stmt = db.prepare("SELECT server, kite_ua FROM site_settings WHERE kite_ua IS NOT NULL AND kite_ua != ''")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn store_handlers(db: &Connection, host: &str, handlers: &BTreeMap<String, LinkHandler>) -> anyhow::Result<()> {
    let json = if handlers.is_empty() { String::new() } else { serde_json::to_string(handlers)? };
    db.execute("INSERT OR IGNORE INTO site_settings (server) VALUES (?1)", [host])?;
    db.execute("UPDATE site_settings SET kite_handlers = ?2 WHERE server = ?1", params![host, json])?;
    Ok(())
}

/// The kinds of link a site may ask for: the ones the HTML standard and
/// Chromium let pages register, and anything starting with `web+`. Never an
/// address a tab shows itself.
pub fn link_scheme_allowed(scheme: &str) -> bool {
    const SAFE: &[&str] = &[
        "bitcoin", "cabal", "dat", "did", "dweb", "ethereum", "ftp", "ftps", "geo", "hyper", "im", "ipfs", "ipns", "irc", "ircs", "magnet", "mailto",
        "matrix", "mms", "news", "nntp", "openpgp4fpr", "sftp", "sip", "sms", "smsto", "ssb", "ssh", "tel", "urn", "webcal", "wtai", "xmpp",
    ];
    SAFE.contains(&scheme) || scheme.strip_prefix("web+").is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_lowercase()))
}

/// Remember what the site at `url` (its address for such links, with `%s`)
/// was told about links of `scheme`: 1 = allowed, 2 = blocked, 0 = nothing,
/// it may ask again. `now`: seconds since 1970.
pub fn set_handler(db: &Connection, scheme: &str, url: &str, decision: i64, now: i64) -> anyhow::Result<()> {
    anyhow::ensure!(link_scheme_allowed(scheme), "no links a site may ask for: {scheme}");
    // (Qt hands the address over with every % in it encoded: to its mind the
    // placeholder is a mistake, and where there is one, no % means an encoding.)
    let url = if url.contains("%s") { url.to_owned() } else { url.replace("%25", "%") };
    let parsed = url::Url::parse(&url)?;
    anyhow::ensure!(matches!(parsed.scheme(), "https" | "http") && url.contains("%s"), "not an address for links: {url}");
    let host = parsed.host_str().ok_or_else(|| anyhow::anyhow!("no site in {url}"))?.to_owned();
    let mut handlers = get(db, &host)?.handlers;
    match decision {
        1 | 2 => {
            // The same answer again (the site asks on every visit, and is told
            // what it was told) isn't a newer one: only its address may change.
            let at = match handlers.get(scheme) {
                Some(known) if known.decision == decision && known.url == url => return Ok(()),
                Some(known) if known.decision == decision => known.at,
                _ => now,
            };
            handlers.insert(scheme.to_owned(), LinkHandler { decision, url, at })
        }
        _ => handlers.remove(scheme),
    };
    store_handlers(db, &host, &handlers)?;
    clear_if_empty(db, &host)
}

/// Where a link (`mailto:someone@example.org`) goes if a site was allowed
/// links of its kind: that site's address with the link in it.
pub fn link_target(db: &Connection, link: &str) -> anyhow::Result<Option<String>> {
    let Some((scheme, _)) = link.split_once(':') else { return Ok(None) };
    let scheme = scheme.to_ascii_lowercase();
    if !link_scheme_allowed(&scheme) {
        return Ok(None);
    }
    let mut stmt = db.prepare("SELECT kite_handlers FROM site_settings WHERE kite_handlers IS NOT NULL AND kite_handlers != ''")?;
    let sites: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let allowed = sites
        .iter()
        .filter_map(|json| serde_json::from_str::<BTreeMap<String, LinkHandler>>(json).ok()?.remove(&scheme))
        .filter(|h| h.decision == 1 && h.url.contains("%s"))
        .max_by_key(|h| h.at);
    Ok(allowed.map(|h| h.url.replacen("%s", &utf8_percent_encode(link, LINK_IN_ADDRESS).to_string(), 1)))
}

/// What stays as it is when a link is put into the site's address
/// (JavaScript's encodeURIComponent, which is what Chromium does there).
const LINK_IN_ADDRESS: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')');

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_about_links_are_kept_and_links_go_to_the_site_allowed_last() {
        let db = crate::db::open_in_memory().unwrap();
        assert_eq!(link_target(&db, "mailto:a@b.org").unwrap(), None, "nobody was allowed");
        set_handler(&db, "mailto", "https://mail.example/compose?to=%s", 1, 100).unwrap();
        set_handler(&db, "magnet", "https://mail.example/m?%s", 2, 100).unwrap();
        let s = get(&db, "mail.example").unwrap();
        assert_eq!((s.handlers["mailto"].decision, s.handlers["magnet"].decision), (1, 2));
        assert!(!s.is_empty());
        assert_eq!(link_target(&db, "mailto:a@b.org?subject=Hi there&x=1").unwrap().as_deref(), Some("https://mail.example/compose?to=mailto%3Aa%40b.org%3Fsubject%3DHi%20there%26x%3D1"));
        assert_eq!(link_target(&db, "MAILTO:a@b.org").unwrap().as_deref(), Some("https://mail.example/compose?to=MAILTO%3Aa%40b.org"));
        assert_eq!(link_target(&db, "magnet:?xt=1").unwrap(), None, "blocked");
        assert_eq!(link_target(&db, "https://mail.example/").unwrap(), None);

        // Told the same again on a later visit, the site isn't the one allowed last.
        set_handler(&db, "mailto", "https://mail.example/compose?to=%s", 1, 150).unwrap();
        set_handler(&db, "mailto", "https://mail.example/new?to=%s", 1, 160).unwrap();
        let again = &get(&db, "mail.example").unwrap().handlers["mailto"];
        assert_eq!((again.at, again.url.as_str()), (100, "https://mail.example/new?to=%s"));
        set_handler(&db, "mailto", "https://mail.example/compose?to=%s", 1, 170).unwrap();

        // Another site allowed later gets them, until it is blocked or forgotten.
        set_handler(&db, "mailto", "https://other.example/?via=a%252Bb&u=%25s", 1, 200).unwrap();
        assert_eq!(get(&db, "other.example").unwrap().handlers["mailto"].url, "https://other.example/?via=a%2Bb&u=%s", "as the site gave it");
        assert_eq!(link_target(&db, "mailto:a@b.org").unwrap().as_deref(), Some("https://other.example/?via=a%2Bb&u=mailto%3Aa%40b.org"));
        set_handler(&db, "mailto", "https://other.example/?via=a%2Bb&u=%s", 2, 300).unwrap();
        assert_eq!(link_target(&db, "mailto:a@b.org").unwrap().as_deref(), Some("https://mail.example/compose?to=mailto%3Aa%40b.org"));
        set_handler(&db, "mailto", "https://other.example/?via=a%2Bb&u=%s", 0, 400).unwrap();
        assert!(all(&db).unwrap().iter().all(|(host, _)| host != "other.example"), "nothing left of it");

        // Kept with the site's other settings, and gone with its permissions.
        store(&db, "copy.example", &s).unwrap();
        assert_eq!(get(&db, "copy.example").unwrap().handlers, s.handlers);
        reset(&db, "mail.example").unwrap();
        assert!(get(&db, "mail.example").unwrap().handlers.is_empty());
    }

    #[test]
    fn only_links_a_site_may_ask_for_are_taken() {
        let db = crate::db::open_in_memory().unwrap();
        for scheme in ["https", "http", "file", "javascript", "kite", "data", "web+", "web+A1", "WEB+x"] {
            assert!(set_handler(&db, scheme, "https://a.example/?%s", 1, 1).is_err(), "{scheme}");
        }
        assert!(set_handler(&db, "web+thing", "https://a.example/?%s", 1, 1).is_ok());
        assert!(set_handler(&db, "mailto", "https://a.example/nowhere", 1, 1).is_err(), "no place for the link");
        assert!(set_handler(&db, "mailto", "javascript:alert('%s')", 1, 1).is_err());
        assert!(set_handler(&db, "mailto", "file:///etc/%s", 1, 1).is_err());
    }

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
    fn a_zoom_back_at_the_default_is_no_setting() {
        let db = crate::db::open_in_memory().unwrap();
        set(&db, "a.org", "zoom", 110).unwrap();
        set(&db, "b.org", "zoom", 100).unwrap();
        set(&db, "c.org", "zoom", 100).unwrap();
        set(&db, "c.org", "darkMode", 1).unwrap();
        assert_eq!(forget_default_zoom(&db, 100).unwrap(), 2);
        let hosts: Vec<String> = all(&db).unwrap().into_iter().map(|(h, _)| h).collect();
        assert_eq!(hosts, ["a.org", "c.org"], "b.org had nothing else");
        assert_eq!(get(&db, "c.org").unwrap().zoom, -1, "its dark mode stays");
        assert_eq!(get(&db, "a.org").unwrap().zoom, 110);
        set(&db, "a.org", "zoom", -1).unwrap();
        clear_if_empty(&db, "a.org").unwrap();
        assert_eq!(all(&db).unwrap().len(), 1);
        clear_if_empty(&db, "c.org").unwrap();
        assert_eq!(all(&db).unwrap().len(), 1, "not empty, kept");
    }

    #[test]
    fn user_agent_is_kept_through_reset_and_import() {
        let db = crate::db::open_in_memory().unwrap();
        assert!(user_agents(&db).unwrap().is_empty());
        set_text(&db, "a.org", "userAgent", "Test/1.0").unwrap();
        set(&db, "a.org", "zoom", 150).unwrap();
        set(&db, "b.org", "zoom", 120).unwrap();
        let s = get(&db, "a.org").unwrap();
        assert_eq!(s.user_agent, "Test/1.0");
        assert!(!s.is_empty());
        assert_eq!(user_agents(&db).unwrap().into_iter().collect::<Vec<_>>(), vec![("a.org".to_string(), "Test/1.0".to_string())], "only sites with one");
        reset(&db, "a.org").unwrap();
        let s = get(&db, "a.org").unwrap();
        assert_eq!((s.zoom, s.user_agent.as_str()), (-1, "Test/1.0"), "chosen by the user, so kept");
        store(&db, "c.org", &s).unwrap();
        assert_eq!(get(&db, "c.org").unwrap().user_agent, "Test/1.0");
        set_text(&db, "a.org", "userAgent", "").unwrap();
        assert_eq!(user_agents(&db).unwrap().len(), 1, "cleared");
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
