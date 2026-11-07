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
    matching(db, text, start, end, limit, "date DESC")
}

fn matching(db: &Connection, text: &str, start: i64, end: i64, limit: usize, order: &str) -> anyhow::Result<Vec<HistoryEntry>> {
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
    sql += &format!(" ORDER BY {order} LIMIT {}", limit.max(1));
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(terms.iter()), row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// What matters of an address when it's typed: without scheme and www., in lower case.
fn typed_form(url: &str) -> String {
    let u = url.trim().to_lowercase();
    let u = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(&u);
    u.strip_prefix("www.").unwrap_or(u).to_owned()
}

/// Splits an address in `typed_form` into its host and the rest.
fn host_and_rest(typed: &str) -> (&str, &str) {
    typed.split_at(typed.find(['/', '?', '#']).unwrap_or(typed.len()))
}

/// How well an address fits what's typed, the best first. It's what people mean
/// by typing part of an address that counts, not the words in a page's title.
pub fn match_rank(url: &str, text: &str) -> u8 {
    let needle = typed_form(text);
    let typed = typed_form(url);
    let (host, rest) = host_and_rest(&typed);
    if typed.starts_with(&needle) {
        // A site's main page before the pages on it.
        return if rest.is_empty() || rest == "/" { 0 } else { 1 };
    }
    if host.contains(&format!(".{needle}")) {
        return 2;
    }
    if needle.split_whitespace().all(|w| typed.contains(w)) {
        return 3;
    }
    4
}

/// A history search by how well the addresses fit, for someone looking for a page.
pub fn search_ranked(db: &Connection, text: &str, start: i64, end: i64, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    // (The shortest, as a recent limit could leave out a much used site's main page.)
    let mut found = matching(db, text, start, end, limit, "length(url)")?;
    found.sort_by_key(|e| (match_rank(&e.url, text), std::cmp::Reverse(e.count.min(1000) * 10 - e.url.len() as i64)));
    Ok(found)
}

/// Most visited sites, used for the speed dial / topSites API.
pub fn top_sites(db: &Connection, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    let mut stmt = db.prepare(
        "SELECT id, url, title, date, count FROM history ORDER BY count DESC, date DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The address of the main page of a web address.
fn main_page(url: &str) -> Option<String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return None;
    }
    let after_scheme = url.find("://")? + 3;
    let end = url[after_scheme..].find(['/', '?', '#']).map_or(url.len(), |i| after_scheme + i);
    Some(format!("{}/", &url[..end]))
}

/// How many of the main pages a site wasn't visited at are offered: they only help
/// for the few sites that fit best.
const MAX_ADDED_MAIN_PAGES: usize = 3;
/// And not for a site only passed through, above pages that are visited often.
const MIN_VISITS_FOR_ADDED_MAIN_PAGE: i64 = 2;

/// Completion candidates for the location bar: what fits the address typed
/// (`match_rank`), and among those what's visited most, sites by all their pages.
/// A site that fits has its main page in there, also if it was only ever left
/// through links to its other pages (those have an id of 0, and no title).
pub fn complete(db: &Connection, text: &str, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(vec![]);
    }
    let candidates = matching(db, text, 0, 0, 500, "length(url)")?;
    let mut sites = std::collections::HashMap::<String, (i64, Option<&HistoryEntry>)>::new();
    let mut main_pages = std::collections::HashSet::new();
    for e in &candidates {
        let typed = typed_form(&e.url);
        let (host, rest) = host_and_rest(&typed);
        if rest.is_empty() || rest == "/" {
            main_pages.insert(host.to_owned());
        }
        let site = sites.entry(host.to_owned()).or_insert((0, None));
        site.0 += e.count;
        if match_rank(&e.url, text) <= 1 && main_page(&e.url).is_some() {
            site.1.get_or_insert(e);
        }
    }
    let mut missing: Vec<HistoryEntry> = sites
        .iter()
        .filter(|(host, (visits, _))| !main_pages.contains(*host) && *visits >= MIN_VISITS_FOR_ADDED_MAIN_PAGE)
        .filter(|(host, _)| match_rank(&format!("https://{host}/"), text) == 0)
        .filter_map(|(_, (visits, page))| {
            let page = (*page)?;
            Some(HistoryEntry { id: 0, url: main_page(&page.url)?, title: String::new(), date: page.date, count: *visits })
        })
        .collect();
    missing.sort_by_key(|e| std::cmp::Reverse(e.count));
    missing.truncate(MAX_ADDED_MAIN_PAGES);
    let visits_to = |e: &HistoryEntry| {
        let typed = typed_form(&e.url);
        sites.get(host_and_rest(&typed).0).map_or(e.count, |s| s.0)
    };
    let key = |e: &HistoryEntry| {
        let rank = match_rank(&e.url, text);
        // Main pages by their site. Others by themselves, the shorter ones first.
        let weight = if rank == 0 { visits_to(e) } else { e.count.min(1000) * 10 - e.url.len() as i64 };
        (rank, std::cmp::Reverse(weight))
    };
    let mut ranked: Vec<HistoryEntry> = candidates.iter().cloned().chain(missing).collect();
    ranked.sort_by_key(key);
    ranked.truncate(limit);
    Ok(ranked)
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

    fn visit(db: &Connection, url: &str, title: &str, times: usize) {
        for _ in 0..times {
            add_visit(db, url, title).unwrap();
        }
    }
    fn urls(entries: Vec<HistoryEntry>) -> Vec<String> {
        entries.into_iter().map(|e| e.url).collect()
    }

    #[test]
    fn a_main_page_comes_before_pages_watched_more_often() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://www.youtube.com/watch?v=abc", "You Laugh You Reset - YouTube", 50);
        visit(&db, "https://www.youtube.com/", "YouTube", 2);
        assert_eq!(urls(complete(&db, "youtu", 5).unwrap())[0], "https://www.youtube.com/");
    }

    #[test]
    fn a_main_page_never_visited_is_offered() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://www.youtube.com/watch?v=abc", "You Laugh You Reset - YouTube", 3);
        let found = complete(&db, "youtu", 5).unwrap();
        assert_eq!(found[0].url, "https://www.youtube.com/");
        assert_eq!((found[0].id, found[0].title.as_str()), (0, ""));
        assert_eq!(found[1].url, "https://www.youtube.com/watch?v=abc");
        // It's not added to the history.
        assert!(get(&db, "https://www.youtube.com/").unwrap().is_none());
    }

    #[test]
    fn no_main_page_for_a_site_only_passed_through() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://youtu.be/xyz", "Short link", 1);
        visit(&db, "https://www.youtube.com/watch?v=abc", "A video", 50);
        let found = urls(complete(&db, "youtu", 5).unwrap());
        assert_eq!(found, ["https://www.youtube.com/", "https://www.youtube.com/watch?v=abc", "https://youtu.be/xyz"]);
    }

    #[test]
    fn no_main_page_for_a_site_that_only_fits_by_a_title_or_path() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://example.org/about", "All about youtube", 1);
        visit(&db, "https://example.net/youtube", "Clips", 1);
        let found = urls(complete(&db, "youtu", 5).unwrap());
        assert_eq!(found, ["https://example.net/youtube", "https://example.org/about"]);
    }

    #[test]
    fn sites_rank_by_the_visits_of_all_their_pages() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://youtu.be/", "short", 2);
        visit(&db, "https://www.youtube.com/", "YouTube", 1);
        visit(&db, "https://www.youtube.com/watch?v=abc", "A video", 20);
        assert_eq!(urls(complete(&db, "youtu", 5).unwrap())[0], "https://www.youtube.com/");
    }

    #[test]
    fn an_address_typed_further_fits_its_page() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://www.youtube.com/", "YouTube", 9);
        visit(&db, "https://www.youtube.com/feed/history", "History", 1);
        assert_eq!(urls(complete(&db, "youtube.com/feed", 5).unwrap())[0], "https://www.youtube.com/feed/history");
        assert_eq!(urls(complete(&db, "https://www.youtu", 5).unwrap())[0], "https://www.youtube.com/");
    }

    #[test]
    fn a_much_used_sites_main_page_is_not_lost_among_its_pages() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://www.youtube.com/", "YouTube", 1);
        for i in 0..600 {
            visit(&db, &format!("https://www.youtube.com/watch?v={i:04}"), "A video", 1);
        }
        assert_eq!(urls(complete(&db, "youtu", 5).unwrap())[0], "https://www.youtube.com/");
        assert_eq!(urls(search_ranked(&db, "youtu", 0, 0, 10).unwrap())[0], "https://www.youtube.com/");
    }

    #[test]
    fn subdomains_and_words_in_addresses_rank_after_what_starts_with_it() {
        assert!(match_rank("https://kde.org/", "kde") < match_rank("https://kde.org/a", "kde"));
        assert!(match_rank("https://kde.org/a", "kde") < match_rank("https://planet.kde.org/", "kde"));
        assert!(match_rank("https://planet.kde.org/", "kde") < match_rank("https://example.org/kde", "kde"));
        assert!(match_rank("https://example.org/kde", "kde") < match_rank("https://example.org/", "kde"));
    }

    #[test]
    fn a_ranked_search_only_has_real_visits_in_the_given_time() {
        let db = crate::db::open_in_memory().unwrap();
        visit(&db, "https://www.youtube.com/watch?v=abc", "A video", 3);
        assert_eq!(urls(search_ranked(&db, "youtu", 0, 0, 10).unwrap()), ["https://www.youtube.com/watch?v=abc"]);
        assert!(search_ranked(&db, "youtu", now_ms() + 60_000, 0, 10).unwrap().is_empty());
    }
}
