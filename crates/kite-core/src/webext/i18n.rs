//! `_locales/<locale>/messages.json` handling (browser.i18n, `__MSG_x__`).

use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Messages {
    /// Lower-cased message name -> message object, already merged with
    /// fallbacks (default_locale) so lookups are a single step.
    pub map: Map<String, Value>,
    pub locale: String,
}

/// The UI locale in the form used for `_locales` dirs ("en_US").
pub fn ui_locale() -> String {
    let raw = std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LC_MESSAGES").ok().filter(|v| !v.is_empty()))
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "en_US".into());
    let l = raw.split(['.', '@']).next().unwrap_or("en_US");
    if l == "C" || l == "POSIX" || l.is_empty() {
        "en_US".into()
    } else {
        l.replace('-', "_")
    }
}

fn read_locale(root: &Path, loc: &str) -> Option<Map<String, Value>> {
    let text = std::fs::read_to_string(root.join("_locales").join(loc).join("messages.json")).ok()?;
    // Some extensions ship messages.json with a BOM or comments.
    let text = text.trim_start_matches('\u{feff}');
    let v: Value = serde_json::from_str(text).ok().or_else(|| serde_json::from_str(&strip_comments(text)).ok())?;
    let mut out = Map::new();
    for (k, v) in v.as_object()? {
        out.insert(k.to_lowercase(), v.clone());
    }
    Some(out)
}

pub(crate) fn strip_comments(s: &str) -> String {
    // Minimal // and /* */ stripper that respects strings.
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let (mut i, mut in_str) = (0, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

impl Messages {
    pub fn load(root: &Path, default_locale: Option<&str>) -> Self {
        let ui = ui_locale();
        let lang = ui.split('_').next().unwrap_or("en").to_owned();
        let mut chain: Vec<String> = vec![ui.clone(), lang];
        if let Some(d) = default_locale {
            chain.push(d.to_owned());
        }
        chain.push("en".into());
        chain.push("en_US".into());
        let mut map = Map::new();
        let mut used = String::new();
        // Lowest priority first so higher priority overwrites.
        for loc in chain.iter().rev() {
            if let Some(m) = read_locale(root, loc) {
                used = loc.clone();
                map.extend(m);
            }
        }
        Messages { map, locale: if used.is_empty() { ui } else { used } }
    }

    /// browser.i18n.getMessage semantics.
    pub fn get(&self, name: &str, subs: &[String], ext_id: &str) -> String {
        let lname = name.to_lowercase();
        match lname.as_str() {
            "@@extension_id" => return ext_id.to_owned(),
            "@@ui_locale" => return self.locale.clone(),
            "@@bidi_dir" => return "ltr".into(),
            "@@bidi_reversed_dir" => return "rtl".into(),
            "@@bidi_start_edge" => return "left".into(),
            "@@bidi_end_edge" => return "right".into(),
            _ => {}
        }
        let Some(entry) = self.map.get(&lname) else { return String::new() };
        let msg = entry.get("message").and_then(Value::as_str).unwrap_or_default();
        let placeholders = entry.get("placeholders").and_then(Value::as_object);
        substitute(msg, placeholders, subs)
    }

    /// Replace `__MSG_name__` occurrences (manifest strings, CSS).
    pub fn localize(&self, s: &str, ext_id: &str) -> String {
        if !s.contains("__MSG_") {
            return s.to_owned();
        }
        let re = regex::Regex::new(r"__MSG_([A-Za-z0-9_@]+?)__").unwrap();
        re.replace_all(s, |c: &regex::Captures| self.get(&c[1], &[], ext_id)).into_owned()
    }
}

fn substitute(msg: &str, placeholders: Option<&Map<String, Value>>, subs: &[String]) -> String {
    let chars: Vec<char> = msg.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '$' {
            // $$ -> $
            if chars.get(i + 1) == Some(&'$') {
                out.push('$');
                i += 2;
                continue;
            }
            // $1..$9 -> substitution
            if let Some(d) = chars.get(i + 1).and_then(|c| c.to_digit(10)) {
                if d >= 1 {
                    out += subs.get(d as usize - 1).map(String::as_str).unwrap_or("");
                }
                i += 2;
                continue;
            }
            // $name$ -> placeholder content (which may contain $1)
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '$') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect::<String>().to_lowercase();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '@') {
                    let content = placeholders
                        .and_then(|p| p.iter().find(|(k, _)| k.to_lowercase() == name))
                        .and_then(|(_, v)| v.get("content")?.as_str())
                        .unwrap_or("");
                    out += &substitute(content, None, subs);
                    i += end + 2;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn messages() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("_locales/en");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("messages.json"),
            r#"{ // comment
              "extName": {"message": "My Ext"},
              "greet": {"message": "Hello $who$, you have $1 items ($$)", "placeholders": {"who": {"content": "$2"}}}}"#,
        )
        .unwrap();
        let m = Messages::load(dir.path(), Some("en"));
        assert_eq!(m.get("extname", &[], "x"), "My Ext");
        assert_eq!(m.get("greet", &["3".into(), "Bob".into()], "x"), "Hello Bob, you have 3 items ($)");
        assert_eq!(m.localize("__MSG_extName__!", "x"), "My Ext!");
        assert_eq!(m.get("@@extension_id", &[], "abc"), "abc");
    }
}
