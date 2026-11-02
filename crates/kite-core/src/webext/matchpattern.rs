//! WebExtension match patterns and globs.
//! <https://developer.mozilla.org/docs/Mozilla/Add-ons/WebExtensions/Match_patterns>
//!
//! Patterns are compiled to a regular expression that is valid both for the
//! Rust `regex` crate and JavaScript `RegExp`, so the same source string can be
//! shipped to the content-script shim.

use regex::Regex;

#[derive(Debug, Clone)]
pub struct MatchPattern {
    pub source: String,
    /// Regex source usable from Rust and JS.
    pub regex_src: String,
    regex: Regex,
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\.+*?()|[]{}^$/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl MatchPattern {
    pub fn parse(p: &str) -> Result<Self, String> {
        let regex_src = Self::to_regex(p)?;
        let regex = Regex::new(&regex_src).map_err(|e| e.to_string())?;
        Ok(Self { source: p.to_owned(), regex_src, regex })
    }

    fn to_regex(p: &str) -> Result<String, String> {
        if p == "<all_urls>" {
            return Ok(r"^(https?|wss?|ftp|file|data):".into());
        }
        let (scheme, rest) = p.split_once("://").ok_or_else(|| format!("invalid match pattern: {p}"))?;
        let scheme_re = match scheme {
            "*" => "(https?|wss?)".to_string(),
            "http" | "https" | "ws" | "wss" | "ftp" | "file" | "data" | "moz-extension" => escape(scheme),
            _ => return Err(format!("invalid scheme in match pattern: {p}")),
        };
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None if scheme == "file" => ("", "/*"),
            None => return Err(format!("match pattern without path: {p}")),
        };
        let host_re = if host == "*" {
            r"[^/]*".to_string()
        } else if let Some(h) = host.strip_prefix("*.") {
            format!(r"([^/@:]*\.)?{}(:[0-9]+)?", escape(h))
        } else if host.contains('*') {
            return Err(format!("invalid host in match pattern: {p}"));
        } else if host.is_empty() {
            String::new()
        } else {
            format!(r"{}(:[0-9]+)?", escape(host))
        };
        let path_re: String = path.split('*').map(escape).collect::<Vec<_>>().join(".*");
        Ok(format!("^{scheme_re}://{host_re}{path_re}$"))
    }

    pub fn matches(&self, url: &str) -> bool {
        // Patterns ignore the fragment.
        let url = url.split('#').next().unwrap_or(url);
        self.regex.is_match(url)
    }

    /// Does the pattern cover a whole host (used for host permissions)?
    pub fn matches_host(&self, url: &str) -> bool {
        match url::Url::parse(url) {
            Ok(u) => self.matches(&format!("{}://{}/", u.scheme(), u.host_str().unwrap_or(""))) || self.matches(url),
            Err(_) => false,
        }
    }
}

/// `include_globs` / `exclude_globs`: `*` any run, `?` one char, whole URL.
pub fn glob_to_regex(glob: &str) -> String {
    let mut out = String::from("^");
    for c in glob.chars() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            c => out.push_str(&escape(&c.to_string())),
        }
    }
    out.push('$');
    out
}

#[derive(Debug, Clone, Default)]
pub struct MatchSet(pub Vec<MatchPattern>);

impl MatchSet {
    pub fn parse<'a>(patterns: impl IntoIterator<Item = &'a str>) -> Self {
        MatchSet(patterns.into_iter().filter_map(|p| MatchPattern::parse(p).ok()).collect())
    }
    pub fn matches(&self, url: &str) -> bool {
        self.0.iter().any(|p| p.matches(url))
    }
    pub fn regex_sources(&self) -> Vec<String> {
        self.0.iter().map(|p| p.regex_src.clone()).collect()
    }
}

/// Is this a permission string that is actually a host pattern?
pub fn is_host_pattern(p: &str) -> bool {
    p == "<all_urls>" || p.contains("://")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns() {
        let m = |p: &str, u: &str| MatchPattern::parse(p).unwrap().matches(u);
        assert!(m("<all_urls>", "https://a.b/"));
        assert!(m("<all_urls>", "file:///etc/hosts"));
        assert!(!m("<all_urls>", "kite:start"));
        assert!(m("*://*.mozilla.org/*", "https://mozilla.org/"));
        assert!(m("*://*.mozilla.org/*", "http://a.b.mozilla.org/x?y#z"));
        assert!(!m("*://*.mozilla.org/*", "https://notmozilla.org/"));
        assert!(!m("*://*.mozilla.org/*", "ftp://mozilla.org/"));
        assert!(m("https://example.com/foo*", "https://example.com:8443/foobar"));
        assert!(!m("https://example.com/foo*", "https://example.com/bar"));
        assert!(m("*://*/*", "https://anything.test/path"));
        assert!(m("file:///home/*", "file:///home/user/a.html"));
        assert!(m("https://example.com/*.js", "https://example.com/a/b.js"));
        assert!(MatchPattern::parse("https://*foo.com/*").is_err());
        assert!(MatchPattern::parse("chrome://x/*").is_err());
        let g = Regex::new(&glob_to_regex("*://*.example.com/a?c*")).unwrap();
        assert!(g.is_match("https://www.example.com/abcdef"));
    }
}
