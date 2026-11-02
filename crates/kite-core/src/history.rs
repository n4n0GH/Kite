//! Browsing history (Falkon `history` table) and location-bar completion.

use crate::util::now_ms;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    /// Last visit, ms since epoch.
    pub date: i64,
    pub count: i64,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: r.get(0)?,
        url: r.get(1)?,
        title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        date: r.get(3)?,
        count: r.get(4)?,
    })
}

/// URLs that should never be recorded.
pub fn is_recordable(url: &str) -> bool {
    !(url.is_empty()
        || url.starts_with("kite:")
        || url.starts_with("about:")
        || url.starts_with("data:")
        || url.starts_with("view-source:")
        || url.starts_with("moz-extension:")
        || url.starts_with("devtools:")
        || url.starts_with("chrome:"))
}

pub fn add_visit(db: &Connection, url: &str, title: &str) -> anyhow::Result<Option<i64>> {
    if !is_recordable(url) {
        return Ok(None);
    }
    let now = now_ms();
    db.execute(
        "INSERT INTO history (url, title, date, count) VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(url) DO UPDATE SET count = count + 1, date = ?3,
            title = CASE WHEN ?2 = '' THEN title ELSE ?2 END",
        params![url, title, now],
    )?;
    Ok(db
        .query_row("SELECT id FROM history WHERE url = ?1", [url], |r| r.get(0))
        .optional()?)
}

pub fn update_title(db: &Connection, url: &str, title: &str) -> anyhow::Result<()> {
    if !title.is_empty() {
        db.execute("UPDATE history SET title = ?2 WHERE url = ?1", params![url, title])?;
    }
    Ok(())
}

pub fn get(db: &Connection, url: &str) -> anyhow::Result<Option<HistoryEntry>> {
    Ok(db
        .query_row("SELECT id, url, title, date, count FROM history WHERE url = ?1", [url], row)
        .optional()?)
}

/// Search history. Every whitespace separated term must match url or title.
/// `start`/`end` bound the visit time (ms); 0 means unbounded.
pub fn search(db: &Connection, text: &str, start: i64, end: i64, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    let terms: Vec<String> = text
        .split_whitespace()
        .map(|t| format!("%{}%", t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
        .collect();
    let mut sql = String::from("SELECT id, url, title, date, count FROM history WHERE 1=1");
    for i in 0..terms.len() {
        sql += &format!(" AND (url LIKE ?{0} ESCAPE '\\' OR title LIKE ?{0} ESCAPE '\\')", i + 1);
    }
    if start > 0 {
        sql += &format!(" AND date >= {start}");
    }
    if end > 0 {
        sql += &format!(" AND date <= {end}");
    }
    sql += &format!(" ORDER BY date DESC LIMIT {}", limit.max(1));
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(terms.iter()), row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Most visited sites, used for the speed dial / topSites API.
pub fn top_sites(db: &Connection, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    let mut stmt = db.prepare(
        "SELECT id, url, title, date, count FROM history ORDER BY count DESC, date DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Completion candidates for the location bar, ranked like Falkon: prefix
/// matches on the host first, then by visit count and recency.
pub fn complete(db: &Connection, text: &str, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(vec![]);
    }
    let mut candidates = search(db, text, 0, 0, 200)?;
    let needle = text.to_lowercase();
    let score = |e: &HistoryEntry| -> i64 {
        let u = e.url.to_lowercase();
        let stripped = u
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.");
        let mut s = e.count.min(1000) * 10;
        if stripped.starts_with(&needle) {
            s += 100_000;
        }
        if e.title.to_lowercase().starts_with(&needle) {
            s += 20_000;
        }
        // shorter URLs (site roots) first
        s -= stripped.len() as i64;
        s
    };
    candidates.sort_by_key(|e| std::cmp::Reverse(score(e)));
    candidates.truncate(limit);
    Ok(candidates)
}

pub fn delete_url(db: &Connection, url: &str) -> anyhow::Result<()> {
    db.execute("DELETE FROM history WHERE url = ?1", [url])?;
    Ok(())
}

pub fn delete_id(db: &Connection, id: i64) -> anyhow::Result<()> {
    db.execute("DELETE FROM history WHERE id = ?1", [id])?;
    Ok(())
}

pub fn delete_range(db: &Connection, start: i64, end: i64) -> anyhow::Result<usize> {
    Ok(db.execute("DELETE FROM history WHERE date >= ?1 AND date <= ?2", params![start, end])?)
}

pub fn clear(db: &Connection) -> anyhow::Result<()> {
    db.execute("DELETE FROM history", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visits_and_completion() {
        let db = crate::db::open_in_memory().unwrap();
        add_visit(&db, "https://kde.org/", "KDE").unwrap();
        add_visit(&db, "https://kde.org/", "").unwrap();
        add_visit(&db, "https://www.example.org/kde", "Example").unwrap();
        add_visit(&db, "kite:start", "Start").unwrap();
        let e = get(&db, "https://kde.org/").unwrap().unwrap();
        assert_eq!((e.count, e.title.as_str()), (2, "KDE"));
        assert!(get(&db, "kite:start").unwrap().is_none());
        let c = complete(&db, "kde", 5).unwrap();
        assert_eq!(c[0].url, "https://kde.org/");
        assert_eq!(c.len(), 2);
        assert_eq!(search(&db, "example kde", 0, 0, 10).unwrap().len(), 1);
        assert_eq!(search(&db, "100%", 0, 0, 10).unwrap().len(), 0);
    }
}
