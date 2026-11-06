//! The `kite:config` page: every setting and its value, to change directly, also
//! those Settings has no control for (like Firefox's about:config).
//!
//! Anything a user changes here is checked to be of the setting's kind, but not
//! whether it makes sense: the page warns about that, and `kite --factory` is the way
//! back.

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub key: String,
    pub value: Value,
    pub kind: &'static str,
    pub modified: bool,
    pub has_default: bool,
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "object",
        Value::Null => "unset",
    }
}

/// One setting. Its kind is that of its default, if it has one: that's the kind Kite
/// expects, whatever is stored now.
pub fn entry(key: &str, current: Option<&Value>, default: Option<&Value>) -> Entry {
    let value = current.or(default).cloned().unwrap_or(Value::Null);
    Entry {
        key: key.to_owned(),
        kind: kind_of(default.filter(|d| !d.is_null()).unwrap_or(&value)),
        modified: current.is_some_and(|c| Some(c) != default),
        has_default: default.is_some(),
        value,
    }
}

/// All of them by name: what has a default, what's stored, and what plugins declare.
pub fn entries(current: &Map<String, Value>, defaults: &Map<String, Value>) -> Vec<Entry> {
    let mut keys: Vec<&String> = current.keys().chain(defaults.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter().map(|k| entry(k, current.get(k), defaults.get(k))).collect()
}

/// What was typed for a setting of that kind, as its value.
pub fn parse_value(kind: &str, text: &str) -> Result<Value, String> {
    let json = || serde_json::from_str::<Value>(text.trim());
    match kind {
        "string" => Ok(Value::String(text.to_owned())),
        "boolean" => match text.trim() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err("This has to be true or false.".into()),
        },
        "integer" => match json() {
            Ok(v) if v.is_i64() || v.is_u64() => Ok(v),
            _ => Err("This has to be a whole number.".into()),
        },
        "number" => match json() {
            Ok(v) if v.is_number() => Ok(v),
            _ => Err("This has to be a number.".into()),
        },
        "list" => match json() {
            Ok(v) if v.is_array() => Ok(v),
            _ => Err("This has to be a JSON list, like [\"a\", \"b\"].".into()),
        },
        "object" => match json() {
            Ok(v) if v.is_object() => Ok(v),
            _ => Err("This has to be a JSON object, like {\"a\": 1}.".into()),
        },
        // No default says what it should be: any JSON, or else plain text.
        _ => Ok(json().unwrap_or_else(|_| Value::String(text.to_owned()))),
    }
}

const STYLE: &str = r#"
:root { color-scheme: light dark; --accent: #3daee9; --line: #8884; }
body { font: 14px/1.45 system-ui, sans-serif; margin: 0; padding: 1.5em; }
[hidden] { display: none !important; }
#warning { max-width: 38em; margin: 12vh auto; }
h1 { font-size: 1.5em; font-weight: 600; margin: 0 0 .5em; }
code { font-size: .95em; }
button { font: inherit; padding: .3em .9em; border: 1px solid #888; border-radius: 4px; background: transparent; color: inherit; cursor: pointer; }
button:hover { background: #8882; }
#accept { margin-top: 1.2em; background: var(--accent); border-color: var(--accent); color: #fff; }
#main { max-width: 76em; margin: 0 auto; }
#tools { display: flex; gap: 1em; align-items: center; position: sticky; top: 0; padding: .6em 0; background: Canvas; }
#search { flex: 1; font: inherit; padding: .4em .6em; }
#count { opacity: .7; }
.note { opacity: .7; margin: 0 0 .4em; }
table { border-collapse: collapse; width: 100%; }
td { padding: .3em .6em .3em 0; border-top: 1px solid var(--line); vertical-align: top; }
td.key { overflow-wrap: anywhere; width: 38%; }
tr.modified td.key { font-weight: 600; }
td.kind { opacity: .6; width: 5em; }
td:last-child { width: 5em; text-align: right; }
.value { font: 13px ui-monospace, monospace; width: 100%; box-sizing: border-box; padding: .2em .4em; }
.toggle { min-width: 5em; font-family: ui-monospace, monospace; font-size: 13px; }
.bad { outline: 2px solid #da4453; }
.error { color: #da4453; margin-top: .2em; }
"#;

/// Something unguessable, for a token or a nonce.
pub fn secret() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// The page. `token` is what its changes have to come with; `nonce` lets only its own
/// script run. All that's in it from settings is inside a JSON block.
pub fn page(entries: &[Entry], token: &str, nonce: &str) -> String {
    let data = serde_json::json!({"entries": entries, "token": token}).to_string().replace('<', "\\u003c");
    let script = include_str!("config_page.js");
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Advanced Configuration</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-{nonce}'">
<meta name="color-scheme" content="light dark"><style>{STYLE}</style></head><body>
<div id="warning">
<h1>Proceed with caution</h1>
<p>Changing these settings can break Kite, or weaken its security or your privacy. Some have no other way to be changed, and nothing checks that a value makes sense.</p>
<p>If Kite stops working, start it with <code>kite --factory</code>. That saves your settings to a file and goes back to the defaults.</p>
<button id="accept">Accept the Risk and Continue</button>
</div>
<div id="main" hidden>
<h1>Advanced Configuration</h1>
<p class="note">Changes are saved at once. Some only take effect after restarting Kite. Enter saves a field (Ctrl+Enter in lists and objects), Esc undoes typing.</p>
<div id="tools"><input id="search" type="search" placeholder="Search settings by name" aria-label="Search settings by name"><label><input id="changed" type="checkbox"> Only changed ones</label><span id="count"></span></div>
<table><tbody id="rows"></tbody></table>
</div>
<script id="data" type="application/json">{data}</script>
<script nonce="{nonce}">{script}</script>
</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn lists_what_has_a_default_and_what_is_stored() {
        let defaults = map(json!({"a.bool": true, "a.zero": 0, "a.same": "x"}));
        let current = map(json!({"a.bool": false, "a.same": "x", "plugins.p.enabled": true}));
        let e = entries(&current, &defaults);
        assert_eq!(e.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["a.bool", "a.same", "a.zero", "plugins.p.enabled"]);
        assert!(e[0].modified && e[0].value == json!(false) && e[0].kind == "boolean");
        assert!(!e[1].modified, "stored, but as the default");
        assert!(!e[2].modified && e[2].value == json!(0) && e[2].kind == "integer");
        assert!(e[3].modified && !e[3].has_default);
    }

    #[test]
    fn its_kind_is_that_of_the_default_whatever_is_stored() {
        // Someone broke it: it can be seen, and put right as what it should be.
        let broken = entry("a", Some(&json!("oops")), Some(&json!(5)));
        assert_eq!((broken.kind, broken.modified, broken.value), ("integer", true, json!("oops")));
    }

    #[test]
    fn a_value_has_to_be_of_its_kind() {
        assert_eq!(parse_value("boolean", "true"), Ok(json!(true)));
        assert_eq!(parse_value("boolean", " false "), Ok(json!(false)));
        assert!(parse_value("boolean", "yes").is_err());
        assert_eq!(parse_value("integer", "12"), Ok(json!(12)));
        assert_eq!(parse_value("integer", "-1"), Ok(json!(-1)));
        for bad in ["1.5", "x", "", "true", "\"1\""] {
            assert!(parse_value("integer", bad).is_err(), "{bad}");
        }
        assert_eq!(parse_value("number", "1.5"), Ok(json!(1.5)));
        assert!(parse_value("number", "x").is_err());
        assert_eq!(parse_value("string", " a \"b\" "), Ok(json!(" a \"b\" ")), "text stays as typed");
        assert_eq!(parse_value("list", "[1, \"a\"]"), Ok(json!([1, "a"])));
        assert!(parse_value("list", "{}").is_err());
        assert!(parse_value("list", "[1,").is_err());
        assert_eq!(parse_value("object", "{\"a\": 1}"), Ok(json!({"a": 1})));
        assert!(parse_value("object", "[]").is_err());
        assert_eq!(parse_value("unset", "3"), Ok(json!(3)));
        assert_eq!(parse_value("unset", "plain"), Ok(json!("plain")));
    }

    #[test]
    fn a_value_cant_become_markup_or_script() {
        let evil = "</script><script>alert(1)</script><img src=x onerror=alert(1)>";
        let html = page(&[entry(evil, Some(&json!(evil)), None)], "tok", "non");
        assert!(!html.contains("</script><script>alert"));
        assert!(!html.contains("<img"));
        // All of it is still there, for the page's script to show as text.
        assert!(html.contains("\\u003cimg src=x onerror=alert(1)>"));
        assert!(html.contains("script-src 'nonce-non'"));
        assert!(html.contains("default-src 'none'"));
    }

    #[test]
    fn warns_first() {
        let html = page(&[], "tok", "non");
        assert!(html.contains("Proceed with caution"));
        assert!(html.contains("kite --factory"));
        assert!(html.contains(r#"<div id="main" hidden>"#));
    }
}
