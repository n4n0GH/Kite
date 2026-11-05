//! Search engines and location-bar input interpretation
//! (Falkon's SearchEnginesManager + LocationBar::loadAction).

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchEngine {
    pub name: String,
    /// Search URL with `%s` as the query placeholder.
    pub url: String,
    #[serde(default)]
    pub shortcut: String,
    #[serde(default)]
    pub suggestions_url: String,
    #[serde(default)]
    pub post_data: String,
}

pub fn builtin_engines() -> Vec<SearchEngine> {
    let e = |name: &str, url: &str, shortcut: &str, sugg: &str| SearchEngine {
        name: name.into(),
        url: url.into(),
        shortcut: shortcut.into(),
        suggestions_url: sugg.into(),
        post_data: String::new(),
    };
    vec![
        e("DuckDuckGo", "https://duckduckgo.com/?q=%s&t=kite", "d", "https://duckduckgo.com/ac/?q=%s&type=list"),
        e("Google", "https://www.google.com/search?client=kite&q=%s", "g", "https://suggestqueries.google.com/complete/search?output=firefox&q=%s"),
        e("Startpage", "https://www.startpage.com/do/search?query=%s", "sp", ""),
        e("Wikipedia (en)", "https://en.wikipedia.org/wiki/Special:Search?search=%s", "w", "https://en.wikipedia.org/w/api.php?action=opensearch&search=%s&namespace=0"),
        e("KDE Store", "https://store.kde.org/search/projectSearchText/%s", "kde", ""),
        e("Firefox Add-ons", "https://addons.mozilla.org/firefox/search/?q=%s", "amo", ""),
        e("YouTube", "https://www.youtube.com/results?search_query=%s", "yt", ""),
    ]
}

/// Engines from the database (user edited); seeded with built-ins on first use.
pub fn load_engines(db: &Connection) -> anyhow::Result<Vec<SearchEngine>> {
    let count: i64 = db.query_row("SELECT COUNT(*) FROM search_engines", [], |r| r.get(0))?;
    if count == 0 {
        save_engines(db, &builtin_engines())?;
    }
    let mut stmt = db.prepare("SELECT name, url, shortcut, suggestionsUrl, postData FROM search_engines ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(SearchEngine {
            name: r.get(0)?,
            // Falkon stores the placeholder as %s too.
            url: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            shortcut: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            suggestions_url: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            post_data: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn save_engines(db: &Connection, engines: &[SearchEngine]) -> anyhow::Result<()> {
    db.execute("DELETE FROM search_engines", [])?;
    for e in engines {
        db.execute(
            "INSERT INTO search_engines (name, url, shortcut, suggestionsUrl, postData) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![e.name, e.url, e.shortcut, e.suggestions_url, e.post_data],
        )?;
    }
    Ok(())
}

pub fn search_url(engine: &SearchEngine, query: &str) -> String {
    let q = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
    if engine.url.contains("%s") {
        engine.url.replace("%s", &q)
    } else {
        format!("{}{}", engine.url, q)
    }
}

/// Parse an OpenSearch / Firefox-style suggestions response
/// (`["query", ["s1", "s2", ...]]`) or DuckDuckGo's `[{"phrase": ..}]`.
pub fn parse_suggestions(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return vec![] };
    if let Some(list) = v.get(1).and_then(|l| l.as_array()) {
        return list.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect();
    }
    if let Some(arr) = v.as_array() {
        return arr.iter().filter_map(|o| o.get("phrase").and_then(|p| p.as_str()).map(str::to_owned)).collect();
    }
    vec![]
}

/// Schemes the browser itself deals with, besides those it can be told to load.
const BROWSER_SCHEMES: &[&str] = &["ws", "wss", "qrc", "filesystem", "kite-res", "cid"];

/// An address for another program (`smb://nas/share`, `magnet:?…`): not one a
/// tab can show. They go to whatever the desktop has for them, and file shares
/// are the file manager's job.
pub fn is_external_url(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else { return false };
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let s = scheme.to_ascii_lowercase();
    valid && !url.contains(char::is_whitespace) && !KNOWN_SCHEMES.contains(&s.as_str()) && !BROWSER_SCHEMES.contains(&s.as_str())
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadAction {
    Url(String),
    Search { engine: String, url: String },
    Invalid,
}

const KNOWN_SCHEMES: &[&str] = &[
    "http", "https", "file", "ftp", "data", "about", "kite", "view-source", "javascript",
    "moz-extension", "chrome", "devtools", "mailto", "blob",
];

fn looks_like_host(s: &str) -> bool {
    let host = s.split(['/', '?', '#']).next().unwrap_or("");
    let host_no_port = match host.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => h,
        _ => host,
    };
    if host_no_port.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host_no_port.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    if host_no_port.starts_with('[') && host_no_port.ends_with(']') {
        return true;
    }
    // A dot with a non-empty, alphabetic TLD, and no spaces.
    match host_no_port.rsplit_once('.') {
        Some((name, tld)) => {
            !name.is_empty()
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_alphanumeric() || c == '-')
                && !tld.chars().all(|c| c.is_ascii_digit())
                && host_no_port.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        }
        None => false,
    }
}

/// Domain endings that make a bare name in text an address. The common general ones
/// and every country's; a rarer one still counts when written with https:// or www.
const COMMON_DOMAINS: &str = "com org net edu gov mil int info biz name pro dev app ai io xyz online site store shop blog tech cloud wiki";
const COUNTRY_DOMAINS: &str = "ac ad ae af ag al am ao aq ar as at au aw ax az ba bb bd be bf bg bh bi bj bm bn bo br bs bt bw by bz ca cc cd cf cg ch ci ck cl cm cn co cr cu cv cw cx cy cz de dj dk dm do dz ec ee eg er es et eu fi fj fk fm fo fr ga gb gd ge gf gg gh gi gl gm gn gp gq gr gs gt gu gw gy hk hm hn hr ht hu id ie il im in iq ir is it je jm jo jp ke kg kh ki km kn kp kr kw ky kz la lb lc li lk lr ls lt lu lv ly ma mc md me mg mh mk ml mm mn mo mp mq mr ms mt mu mv mw mx my mz na nc ne nf ng ni nl no np nr nu nz om pa pe pf pg ph pk pl pm pn pr ps pt pw py qa re ro rs ru rw sa sb sc sd se sg sh si sj sk sl sm sn so sr ss st su sv sx sy sz tc td tf tg th tj tk tl tm tn to tr tt tv tw tz ua ug uk us uy uz va vc ve vg vi vn vu wf ws ye yt za zm zw";
/// Those that are mostly files, in text: `main.py`, `README.md`. With a path after
/// them they're addresses.
const FILE_LIKE: &[&str] = &["py", "rs", "md", "sh", "pl", "so", "ai", "app", "cc", "cd", "ps", "ms", "im", "st", "zip", "mov"];

/// The address selected text is written as, if it is one and nothing else, for
/// offering to open it. Pickier than the location bar, as nobody asked: `Node.js` or
/// `window.location` shouldn't offer a link. And the text is the page's, so only
/// what is plainly the web: no `javascript:` or `file:`, nor `https://bank.com@evil.com`.
pub fn selection_url(text: &str) -> Option<String> {
    let text = trim_around_url(text);
    if text.is_empty() || text.len() > 2048 || text.contains(char::is_whitespace) {
        return None;
    }
    let scheme = text.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
    let explicit = text.contains("://") && matches!(scheme.as_deref(), Some("http" | "https"));
    let u = url::Url::parse(&if explicit { text.to_owned() } else { format!("https://{text}") }).ok()?;
    let host = u.host_str()?.to_owned();
    if !u.username().is_empty() || u.password().is_some() {
        return None;
    }
    if explicit {
        return Some(u.to_string());
    }
    if text.contains("://") {
        return None;
    }
    if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
        // Without a port, `1.2.3.4` is more likely a version.
        let authority = text.split(['/', '?', '#']).next().unwrap_or("");
        let has_port = authority.rsplit_once(':').is_some_and(|(_, p)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        return has_port.then(|| url::Url::parse(&format!("http://{text}")).ok().map(|u| u.to_string())).flatten();
    }
    let tld = host.rsplit('.').next().unwrap_or("");
    let known = COMMON_DOMAINS.split(' ').chain(COUNTRY_DOMAINS.split(' ')).any(|d| d == tld);
    let has_path = text.contains('/');
    (host.starts_with("www.") || (host.contains('.') && known && (has_path || !FILE_LIKE.contains(&tld)))).then(|| u.to_string())
}

/// Without what writing puts around an address: quotes, brackets, the end of a sentence.
fn trim_around_url(text: &str) -> &str {
    let mut t = text.trim();
    loop {
        let before = t;
        t = t.trim_start_matches(['"', '\'', '“', '”', '‘', '’', '«', '»', '<', '(', '[', '{']);
        t = t.trim_end_matches(['"', '\'', '“', '”', '‘', '’', '«', '»', '>', '.', ',', ';', ':', '!', '?']);
        // A closing bracket stays if it belongs to the address: /wiki/Kite_(toy).
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            if t.ends_with(close) && t.matches(close).count() > t.matches(open).count() {
                t = &t[..t.len() - close.len_utf8()];
            }
        }
        if t == before {
            return t;
        }
    }
}

/// Decide what to do with text typed into the location bar.
///
/// * search-engine shortcut: `w rust` → Wikipedia search
/// * bookmark keyword: resolved by the caller via `keyword_lookup`
/// * things that look like URLs / hosts / paths → URL
/// * otherwise → default engine search
pub fn load_action(
    input: &str,
    engines: &[SearchEngine],
    default_engine: &str,
    search_enabled: bool,
    keyword_lookup: &dyn Fn(&str) -> Option<String>,
) -> LoadAction {
    let text = input.trim();
    if text.is_empty() {
        return LoadAction::Invalid;
    }

    // Engine shortcut or bookmark keyword as first word.
    let (first, rest) = text.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((text, ""));
    if !rest.is_empty() {
        if let Some(e) = engines.iter().find(|e| !e.shortcut.is_empty() && e.shortcut == first) {
            return LoadAction::Search { engine: e.name.clone(), url: search_url(e, rest) };
        }
    }
    if let Some(url) = keyword_lookup(text) {
        return LoadAction::Url(url);
    }

    let has_space = text.contains(char::is_whitespace);

    // Explicit scheme.
    if let Some((scheme, _)) = text.split_once(':') {
        let s = scheme.to_ascii_lowercase();
        if KNOWN_SCHEMES.contains(&s.as_str()) && !has_space {
            if let Ok(u) = url::Url::parse(text) {
                return LoadAction::Url(u.to_string());
            }
            return LoadAction::Url(text.to_owned());
        }
    }
    // Another program's kind of address: never a search, which would send it
    // (with names of machines on the network) to the search engine. "://"
    // because "note: hello" or "localhost:8080" are not such addresses.
    if text.contains("://") && is_external_url(text) {
        return LoadAction::Url(text.to_owned());
    }
    // Local paths.
    if text.starts_with('/') || text.starts_with("~/") {
        let p = if let Some(r) = text.strip_prefix("~/") {
            directories::BaseDirs::new().map(|b| b.home_dir().join(r)).unwrap_or_else(|| r.into())
        } else {
            text.into()
        };
        if let Ok(u) = url::Url::from_file_path(&p) {
            return LoadAction::Url(u.to_string());
        }
    }
    if !has_space && looks_like_host(text) {
        let scheme = if text.starts_with("localhost") || text.split(['/', ':']).next().map(|h| h.parse::<std::net::IpAddr>().is_ok()).unwrap_or(false) {
            "http"
        } else {
            "https"
        };
        if let Ok(u) = url::Url::parse(&format!("{scheme}://{text}")) {
            return LoadAction::Url(u.to_string());
        }
    }
    if !search_enabled {
        return LoadAction::Url(format!("https://{text}"));
    }
    let engine = engines.iter().find(|e| e.name == default_engine).or_else(|| engines.first());
    match engine {
        Some(e) => LoadAction::Search { engine: e.name.clone(), url: search_url(e, text) },
        None => LoadAction::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected(text: &str) -> Option<String> {
        selection_url(text)
    }

    #[test]
    fn selection_addresses() {
        assert_eq!(selected("https://kde.org/x?a=1#b").as_deref(), Some("https://kde.org/x?a=1#b"));
        assert_eq!(selected("HTTP://KDE.org").as_deref(), Some("http://kde.org/"));
        assert_eq!(selected("www.anything.example").as_deref(), Some("https://www.anything.example/"));
        assert_eq!(selected("kde.org").as_deref(), Some("https://kde.org/"));
        assert_eq!(selected("bbc.co.uk/news").as_deref(), Some("https://bbc.co.uk/news"));
        assert_eq!(selected("localhost:8080/a").as_deref(), Some("http://localhost:8080/a"));
        assert_eq!(selected("192.168.1.1:8765").as_deref(), Some("http://192.168.1.1:8765/"));
        assert_eq!(selected("192.168.1.1:80/a").as_deref(), Some("http://192.168.1.1/a"));
        assert_eq!(selected("http://192.168.1.1/x").as_deref(), Some("http://192.168.1.1/x"));
    }

    #[test]
    fn selection_without_what_writing_puts_around_it() {
        for text in ["  kde.org  ", "kde.org.", "kde.org,", "(kde.org)", "“kde.org”", "<kde.org>", "\"kde.org\"", "(kde.org).", "kde.org!\n"] {
            assert_eq!(selected(text).as_deref(), Some("https://kde.org/"), "{text:?}");
        }
        assert_eq!(selected("https://en.wikipedia.org/wiki/Kite_(toy)").as_deref(), Some("https://en.wikipedia.org/wiki/Kite_(toy)"));
        assert_eq!(selected("(https://en.wikipedia.org/wiki/Kite_(toy))").as_deref(), Some("https://en.wikipedia.org/wiki/Kite_(toy)"));
    }

    #[test]
    fn selection_that_is_just_text() {
        for text in ["", "hello", "two words", "see kde.org for more", "kde.org and bbc.co.uk", "Node.js", "window.location", "e.g.", "i.e.", "v1.2.3", "1.2.3.4", "localhost", "3.14",
            "me@kde.org", "hello.world", "foo.bar", "file.txt", "main.py", "README.md", "lib.rs", "line one\nkde.org"] {
            assert_eq!(selected(text), None, "{text:?}");
        }
    }

    #[test]
    fn selection_with_a_path_is_an_address_even_if_it_ends_like_a_file() {
        assert_eq!(selected("example.rs/docs").as_deref(), Some("https://example.rs/docs"));
    }

    #[test]
    fn selection_never_offers_what_a_page_could_abuse() {
        for text in ["javascript:alert(1)", "JAVASCRIPT://kde.org/%0aalert(1)", "file:///etc/passwd", "data:text/html,x", "ftp://kde.org/", "smb://nas/share", "chrome://settings",
            "https://kde.org@evil.com", "https://user:pw@kde.org/", "kde.org@evil.com", "kite:start", "about:blank"] {
            assert_eq!(selected(text), None, "{text:?}");
        }
    }

    fn act(s: &str) -> LoadAction {
        let kw = |k: &str| (k == "kde").then(|| "https://kde.org/".to_string());
        load_action(s, &builtin_engines(), "DuckDuckGo", true, &kw)
    }
    #[test]
    fn location_bar() {
        assert_eq!(act("kde.org"), LoadAction::Url("https://kde.org/".into()));
        assert_eq!(act("localhost:8080/x"), LoadAction::Url("http://localhost:8080/x".into()));
        assert_eq!(act("192.168.1.1"), LoadAction::Url("http://192.168.1.1/".into()));
        assert_eq!(act("https://a.b/c?d"), LoadAction::Url("https://a.b/c?d".into()));
        assert_eq!(act("kite:start"), LoadAction::Url("kite:start".into()));
        assert_eq!(act("kde"), LoadAction::Url("https://kde.org/".into()));
        assert!(matches!(act("rust borrow checker"), LoadAction::Search { ref engine, .. } if engine == "DuckDuckGo"));
        assert!(matches!(act("w Rust (language)"), LoadAction::Search { ref url, .. } if url.starts_with("https://en.wikipedia.org") && url.contains("Rust%20%28language%29")));
        assert!(matches!(act("hello"), LoadAction::Search { .. }));
        assert!(matches!(act("3.14"), LoadAction::Search { .. }));
        assert_eq!(act("/etc/hosts"), LoadAction::Url("file:///etc/hosts".into()));
    }
    #[test]
    fn addresses_for_other_programs_are_not_searches() {
        assert_eq!(act("smb://nas/share"), LoadAction::Url("smb://nas/share".into()));
        assert_eq!(act("sftp://user@host/x"), LoadAction::Url("sftp://user@host/x".into()));
        assert!(is_external_url("smb://nas/share"));
        assert!(is_external_url("SMB://nas"));
        assert!(is_external_url("magnet:?xt=urn:btih:abc"));
        assert!(is_external_url("steam://run/1"));
        for own in ["https://a.b/", "http://a", "file:///x", "kite:start", "about:blank", "data:text/plain,x", "blob:x", "javascript:0", "moz-extension://a/", "view-source:https://a", "wss://a", "mailto:a@b.c", "ftp://a"] {
            assert!(!is_external_url(own), "{own}");
        }
        for text in ["no scheme", "a b://c", "1abc://x", "kde.org", ""] {
            assert!(!is_external_url(text), "{text}");
        }
        // Not addresses: still what they were.
        assert_eq!(act("localhost:8080/x"), LoadAction::Url("http://localhost:8080/x".into()));
        assert!(matches!(act("note: hello"), LoadAction::Search { .. }));
        assert!(matches!(act("how does smb:// work"), LoadAction::Search { .. }));
    }
    #[test]
    fn suggestions() {
        assert_eq!(parse_suggestions(r#"["ru",["rust","ruby"]]"#), vec!["rust", "ruby"]);
        assert_eq!(parse_suggestions(r#"[{"phrase":"rust"}]"#), vec!["rust"]);
    }
}
