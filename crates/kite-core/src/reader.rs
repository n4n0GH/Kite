//! Reader view: articles extracted from pages (Readability, in the tab) and
//! the `kite:reader` page showing them.
//!
//! An article's HTML comes from the web page, and `kite:` is a privileged
//! origin (its pages can change the speed dial, for one). So it is sanitised
//! here against an allow list, and the page's Content-Security-Policy forbids
//! scripts and everything else it doesn't need as well.
//!
//! Articles are kept in memory only, under random ids: a `kite:reader` address
//! says nothing about the article, and a web page linking to one can't pass
//! off its own text as another site's (the address bar shows the source kept
//! here, never one taken from the address).

use crate::settings::Settings;
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, LazyLock, Mutex};

const PREFIX: &str = "kite:reader?id=";
/// Articles kept for going back to them, the oldest forgotten first.
const MAX_ARTICLES: usize = 32;
/// Reading speed for the "N min read" estimate, in characters per minute.
const CHARS_PER_MINUTE: u64 = 1200;

/// The colours of the "auto" theme: the desktop's (`#rrggbb`).
#[derive(Debug, Clone, Default)]
pub struct Colors {
    pub background: String,
    pub text: String,
    pub link: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Extracted {
    title: String,
    byline: String,
    site_name: String,
    lang: String,
    dir: String,
    content: String,
    length: u64,
}

#[derive(Debug)]
struct Article {
    source: String,
    title: String,
    byline: String,
    site_name: String,
    lang: String,
    dir: String,
    html: String,
    minutes: u64,
    colors: Colors,
}

#[derive(Default)]
struct Store {
    order: VecDeque<String>,
    articles: HashMap<String, Arc<Article>>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(Default::default);

/// Keep an article extracted from the page at `source`; the address showing
/// it. None if it isn't usable or the page isn't a web page.
pub fn open(source: &str, json: &str, colors: Colors) -> Option<String> {
    let scheme = url::Url::parse(source).ok()?;
    if !matches!(scheme.scheme(), "http" | "https") {
        return None;
    }
    let e: Extracted = serde_json::from_str(json).ok()?;
    let html = sanitize(&e.content);
    if html.trim().is_empty() {
        return None;
    }
    let article = Article {
        source: source.to_owned(),
        title: e.title,
        byline: e.byline,
        site_name: e.site_name,
        lang: e.lang,
        dir: e.dir,
        html,
        minutes: (e.length / CHARS_PER_MINUTE).max(1),
        colors,
    };
    let id = uuid::Uuid::new_v4().to_string();
    let mut store = STORE.lock().unwrap();
    store.order.push_back(id.clone());
    store.articles.insert(id.clone(), Arc::new(article));
    while store.order.len() > MAX_ARTICLES {
        if let Some(old) = store.order.pop_front() {
            store.articles.remove(&old);
        }
    }
    Some(format!("{PREFIX}{id}"))
}

fn article(url: &str) -> Option<Arc<Article>> {
    let id = url.strip_prefix(PREFIX)?.split(['&', '#']).next()?;
    STORE.lock().unwrap().articles.get(id).cloned()
}

/// The page a `kite:reader` address was extracted from.
pub fn source_of(url: &str) -> Option<String> {
    article(url).map(|a| a.source.clone())
}

/// Only what an article needs: no scripts, styles, frames, forms or media
/// other than images, and links and images on the web only.
fn sanitize(html: &str) -> String {
    let mut b = ammonia::Builder::default();
    b.url_schemes(HashSet::from(["http", "https", "mailto"]))
        // Readability makes addresses absolute; what's left can't work here.
        .url_relative(ammonia::UrlRelative::Deny)
        .add_generic_attributes(["dir"])
        // Sized by the page's style.
        .rm_tag_attributes("img", ["width", "height", "align"]);
    b.clean(html).to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

/// The reader settings, limited to values safe to put in the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    pub font_size: i64,
    pub width: i64,
    pub font: &'static str,
    pub theme: &'static str,
}

impl Prefs {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            font_size: s.get_i64("reader.fontSize").clamp(12, 40),
            width: s.get_i64("reader.width").clamp(24, 70),
            font: if s.get_str("reader.font") == "sans" { "sans" } else { "serif" },
            theme: match s.get_str("reader.theme").as_str() {
                "light" => "light",
                "sepia" => "sepia",
                "dark" => "dark",
                _ => "auto",
            },
        }
    }

    /// Script for an open page, so a change keeps its scroll position.
    pub fn apply_script(&self) -> String {
        format!(
            r#"(() => {{ const r = document.documentElement; r.dataset.theme = "{}"; r.dataset.font = "{}"; r.style.setProperty("--size", "{}px"); r.style.setProperty("--width", "{}em"); }})()"#,
            self.theme, self.font, self.font_size, self.width
        )
    }
}

const STYLE: &str = r#"
html { --bg: var(--auto-bg); --fg: var(--auto-fg); --link: var(--auto-link); background: var(--bg); color: var(--fg); }
html[data-theme="light"] { --bg: #ffffff; --fg: #232629; --link: #1d6fa5; color-scheme: light; }
html[data-theme="sepia"] { --bg: #f4ecd8; --fg: #5b4636; --link: #8a4f1d; color-scheme: light; }
html[data-theme="dark"] { --bg: #1b1e20; --fg: #e3e5e7; --link: #6cb6e8; color-scheme: dark; }
html[data-font="serif"] body { font-family: serif; }
html[data-font="sans"] body { font-family: sans-serif; }
body { margin: 0 auto; max-width: var(--width); padding: 3em 1.5em 5em; font-size: var(--size); line-height: 1.6; overflow-wrap: break-word; }
a { color: var(--link); }
header { margin-bottom: 2em; border-bottom: 1px solid color-mix(in srgb, var(--fg) 25%, transparent); padding-bottom: 1em; }
h1 { font-size: 1.8em; line-height: 1.2; margin: 0.3em 0; }
h2 { font-size: 1.4em; line-height: 1.25; } h3 { font-size: 1.2em; line-height: 1.3; }
.meta { font-size: 0.8em; opacity: 0.75; font-family: sans-serif; }
img { display: block; max-width: 100%; height: auto; margin: 1em auto; border-radius: 3px; }
figure { margin: 1.5em 0; } figcaption { font-size: 0.85em; opacity: 0.75; }
blockquote { margin: 1em 0; padding-left: 1em; border-left: 3px solid color-mix(in srgb, var(--fg) 30%, transparent); }
pre { overflow-x: auto; padding: 0.8em; font-size: 0.85em; background: color-mix(in srgb, var(--fg) 8%, transparent); border-radius: 3px; }
code { font-size: 0.9em; }
table { border-collapse: collapse; max-width: 100%; display: block; overflow-x: auto; }
th, td { border: 1px solid color-mix(in srgb, var(--fg) 25%, transparent); padding: 0.3em 0.6em; }
"#;

// `script-src 'none'` doesn't hinder Browser.readerApplyScript: scripts run by
// the browser in an isolated world aren't subject to the page's policy.
const POLICY: &str = "default-src 'none'; img-src http: https:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'";

fn shell(lang: &str, dir: &str, style: &str, title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html{lang}{dir} {style}><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{POLICY}\">\
         <meta name=\"referrer\" content=\"no-referrer\">\
         <title>{title}</title><style>{STYLE}</style></head><body>{body}</body></html>"
    )
}

/// The page for a `kite:reader` address.
pub fn page(url: &str, prefs: &Prefs) -> String {
    let look = |c: &Colors| {
        let color = |v: &str, fallback: &'static str| if is_hex_color(v) { v.to_owned() } else { fallback.to_owned() };
        format!(
            r#"data-theme="{}" data-font="{}" style="--size:{}px;--width:{}em;--auto-bg:{};--auto-fg:{};--auto-link:{}""#,
            prefs.theme,
            prefs.font,
            prefs.font_size,
            prefs.width,
            color(&c.background, "#ffffff"),
            color(&c.text, "#232629"),
            color(&c.link, "#1d6fa5"),
        )
    };
    let Some(a) = article(url) else {
        return shell("", "", &look(&Colors::default()), "Reader View", "<h1>This article is no longer available</h1><p>Kite keeps articles in reader view only for a while. Go back to the page to open it again.</p>");
    };
    let lang_ok = !a.lang.is_empty() && a.lang.len() <= 35 && a.lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let lang = if lang_ok { format!(" lang=\"{}\"", a.lang) } else { String::new() };
    let dir = if matches!(a.dir.as_str(), "ltr" | "rtl") { format!(" dir=\"{}\"", a.dir) } else { String::new() };
    let site = if a.site_name.is_empty() { url::Url::parse(&a.source).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default() } else { a.site_name.clone() };
    let byline = if a.byline.is_empty() { String::new() } else { format!("<div class=\"meta\">{}</div>", esc(&a.byline)) };
    let body = format!(
        "<header><div class=\"meta\"><a href=\"{}\">{}</a> · {} min read</div><h1>{}</h1>{byline}</header><main>{}</main>",
        esc(&a.source),
        esc(&site),
        a.minutes,
        esc(&a.title),
        a.html
    );
    shell(&lang, &dir, &look(&a.colors), &esc(&a.title), &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs() -> Prefs {
        Prefs { font_size: 20, width: 38, font: "serif", theme: "auto" }
    }

    fn json(title: &str, content: &str) -> String {
        serde_json::json!({"title": title, "byline": "A. Writer", "siteName": "Example", "lang": "en", "dir": "ltr", "content": content, "length": 2400}).to_string()
    }

    fn opened(title: &str, content: &str) -> String {
        let url = open("https://example.org/a", &json(title, content), Colors::default()).unwrap();
        page(&url, &prefs())
    }

    #[test]
    fn keeps_the_article() {
        let html = opened("Title", r#"<div><h2>Heading</h2><p>Some <em>text</em> <a href="https://example.org/b">link</a></p><img src="https://example.org/i.png" alt="pic" width="900"><pre><code>x</code></pre></div>"#);
        assert!(html.contains("<title>Title</title>"));
        assert!(html.contains("<h2>Heading</h2>"));
        assert!(html.contains("<em>text</em>"));
        assert!(html.contains(r#"href="https://example.org/b""#));
        assert!(html.contains(r#"src="https://example.org/i.png""#));
        assert!(!html.contains("width=\"900\""));
        assert!(html.contains("2 min read"));
        assert!(html.contains("A. Writer"));
    }

    #[test]
    fn removes_everything_active() {
        let html = opened(
            "T",
            r#"<script>bad1()</script><p onclick="bad2()">x</p><img src="https://e.org/a.png" onerror="bad3()"><a href="javascript:bad4()">a</a>
               <iframe src="https://e.org/bad5"></iframe><style>bad6{}</style><form action="https://e.org/bad7"><input></form><svg onload="bad8()"></svg>
               <object data="https://e.org/bad9"></object><p style="background:url(https://e.org/bad10)">y</p><base href="https://e.org/bad11">"#,
        );
        for i in 1..=11 {
            assert!(!html.contains(&format!("bad{i}")), "bad{i} in {html}");
        }
    }

    #[test]
    fn removes_addresses_of_kite_pages() {
        // Requests from kite: pages may change the speed dial.
        let html = opened("T", r#"<img src="kite:start?add=https://evil.example"><a href="kite:start?add=https://evil.example">a</a><img src="moz-extension://x/y"><img src="data:image/svg+xml,x"><a href="relative">r</a>"#);
        assert!(!html.contains("evil.example"));
        assert!(!html.contains("moz-extension"));
        assert!(!html.contains("data:image"));
        assert!(!html.contains("relative\""));
    }

    #[test]
    fn escapes_what_it_adds_itself() {
        let bad = r#"</title><script>bad()</script>""#;
        let e = serde_json::json!({"title": bad, "byline": bad, "siteName": bad, "lang": "en\"><script>bad()</script>", "dir": "x\"><script>bad()</script>", "content": "<p>x</p>"}).to_string();
        let colors = Colors { background: "red;}</style><script>bad()</script>".into(), text: "#123456".into(), link: String::new() };
        let html = page(&open("https://example.org/a", &e, colors).unwrap(), &prefs());
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("--auto-fg:#123456"));
        assert!(html.contains("--auto-bg:#ffffff"));
    }

    #[test]
    fn page_forbids_scripts() {
        let html = opened("T", "<p>x</p>");
        let policy = html.find("Content-Security-Policy").unwrap();
        // Before anything from the article.
        assert!(policy < html.find("<title>").unwrap());
        assert!(POLICY.contains("default-src 'none'") && !POLICY.contains("script-src"));
        assert!(!POLICY.contains("img-src kite") && !POLICY.contains("data:"));
    }

    #[test]
    fn only_web_pages_and_real_articles() {
        assert!(open("file:///etc/passwd", &json("T", "<p>x</p>"), Colors::default()).is_none());
        assert!(open("kite:start", &json("T", "<p>x</p>"), Colors::default()).is_none());
        assert!(open("https://example.org/", "not json", Colors::default()).is_none());
        assert!(open("https://example.org/", &json("T", "<script>only()</script>"), Colors::default()).is_none());
    }

    #[test]
    fn source_comes_from_the_store_not_the_address() {
        let url = open("https://example.org/a", &json("T", "<p>x</p>"), Colors::default()).unwrap();
        assert_eq!(source_of(&url).as_deref(), Some("https://example.org/a"));
        assert_eq!(source_of(&format!("{url}&url=https://bank.example/")).as_deref(), Some("https://example.org/a"));
        assert_eq!(source_of("kite:reader?id=unknown&url=https://bank.example/"), None);
        assert_eq!(source_of("https://example.org/"), None);
        assert!(page("kite:reader?id=unknown", &prefs()).contains("no longer available"));
    }

    #[test]
    fn prefs_are_limited_to_safe_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load(dir.path().join("settings.json"));
        assert_eq!(Prefs::from_settings(&s), prefs());
        s.set("reader.fontSize", 9000.into()).unwrap();
        s.set("reader.font", "x\"><script>".into()).unwrap();
        s.set("reader.theme", "x\"><script>".into()).unwrap();
        let p = Prefs::from_settings(&s);
        assert_eq!((p.font_size, p.font, p.theme), (40, "serif", "auto"));
        assert!(p.apply_script().contains("\"40px\""));
    }
}
