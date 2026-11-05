//! Certificate exceptions: sites whose certificate problems the user has decided to
//! accept for good, in the profile's database (and so not in settings files: a file
//! from someone else must not be able to make Kite trust a certificate).
//!
//! An exception is normally for one certificate, by its fingerprint, so a different
//! one on that site still gets the warning. One without a fingerprint accepts any
//! certificate of the site, for devices making themselves new ones.

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Exception {
    pub host: String,
    /// SHA-256 of the certificate, in hex; "" for any certificate.
    pub fingerprint: String,
    /// What was wrong with it when it was accepted.
    pub description: String,
    /// Seconds since the epoch.
    pub added: i64,
}

/// A site's name as typed: without what else an address has. None if it isn't one.
pub fn host_of_input(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    let with_scheme = if t.contains("://") { t.to_owned() } else { format!("https://{t}") };
    let u = url::Url::parse(&with_scheme).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    u.host_str().map(str::to_owned)
}

pub fn add(db: &Connection, host: &str, fingerprint: &str, description: &str, now: i64) -> anyhow::Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO kite_cert_exceptions (host, fingerprint, description, added) VALUES (?1, ?2, ?3, ?4)",
        params![host.to_ascii_lowercase(), fingerprint, description, now],
    )?;
    Ok(())
}

pub fn remove(db: &Connection, host: &str, fingerprint: &str) -> anyhow::Result<()> {
    db.execute("DELETE FROM kite_cert_exceptions WHERE host = ?1 AND fingerprint = ?2", params![host, fingerprint])?;
    Ok(())
}

pub fn list(db: &Connection) -> anyhow::Result<Vec<Exception>> {
    let mut stmt = db.prepare("SELECT host, fingerprint, description, added FROM kite_cert_exceptions ORDER BY host, added")?;
    let rows = stmt.query_map([], |r| Ok(Exception { host: r.get(0)?, fingerprint: r.get(1)?, description: r.get(2)?, added: r.get(3)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Has this certificate of this site been accepted? Only for its exact host: an
/// exception for a site says nothing about the names under it.
pub fn accepted(db: &Connection, host: &str, fingerprint: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM kite_cert_exceptions WHERE host = ?1 AND (fingerprint = '' OR (fingerprint = ?2 AND ?2 <> ''))",
        params![host.to_ascii_lowercase(), fingerprint],
        |_| Ok(()),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        crate::db::open_in_memory().unwrap()
    }

    #[test]
    fn only_that_certificate_of_that_site() {
        let d = db();
        add(&d, "Router.Local", "aaa", "self-signed", 5).unwrap();
        assert!(accepted(&d, "router.local", "aaa"));
        assert!(accepted(&d, "ROUTER.local", "aaa"));
        assert!(!accepted(&d, "router.local", "bbb"), "another certificate");
        assert!(!accepted(&d, "other.local", "aaa"), "another site");
        assert!(!accepted(&d, "sub.router.local", "aaa"), "a name under it");
    }

    #[test]
    fn a_site_wide_exception_takes_any_certificate() {
        let d = db();
        add(&d, "nas.local", "", "", 5).unwrap();
        assert!(accepted(&d, "nas.local", "whatever"));
        assert!(accepted(&d, "nas.local", ""));
        assert!(!accepted(&d, "sub.nas.local", "whatever"));
    }

    #[test]
    fn no_fingerprint_doesnt_match_a_pinned_one() {
        let d = db();
        add(&d, "a.local", "aaa", "", 5).unwrap();
        assert!(!accepted(&d, "a.local", ""), "if the certificate couldn't be read, that's no match");
    }

    #[test]
    fn listing_adding_again_and_removing() {
        let d = db();
        add(&d, "b.local", "x", "one", 1).unwrap();
        add(&d, "a.local", "y", "two", 2).unwrap();
        add(&d, "b.local", "x", "newer", 3).unwrap();
        let all = list(&d).unwrap();
        assert_eq!(all.iter().map(|e| e.host.as_str()).collect::<Vec<_>>(), ["a.local", "b.local"]);
        assert_eq!(all[1].description, "newer", "the same exception, not a second one");
        remove(&d, "b.local", "x").unwrap();
        assert!(!accepted(&d, "b.local", "x"));
        assert_eq!(list(&d).unwrap().len(), 1);
    }

    #[test]
    fn what_can_be_typed_as_a_site() {
        assert_eq!(host_of_input(" Router.Local ").as_deref(), Some("router.local"));
        assert_eq!(host_of_input("https://nas.local:8443/x?y").as_deref(), Some("nas.local"));
        assert_eq!(host_of_input("192.168.1.1").as_deref(), Some("192.168.1.1"));
        for bad in ["", "  ", "two words", "ftp://x.local/", "file:///etc/passwd", "javascript:alert(1)"] {
            assert_eq!(host_of_input(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn stays_out_of_the_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let settings = crate::settings::Settings::load(dir.path().join("settings.json"));
        let d = db();
        add(&d, "router.local", "SECRETPRINT", "", 1).unwrap();
        let text = serde_json::to_string(&crate::backup::export(&settings, &d).unwrap()).unwrap();
        assert!(!text.contains("router.local") && !text.contains("SECRETPRINT"));
    }
}
