//! Generation of the JavaScript Kite injects to provide the WebExtension
//! platform:
//!
//! * `/__kite__/api.js` — the API for extension pages (served by the
//!   `moz-extension` scheme handler and inserted into every extension HTML page).
//! * the bridge script — one per tab, in world [`super::CONTENT_WORLD`], owning the
//!   QWebChannel connection and relaying content-script calls.
//! * content scripts — one user script per `content_scripts` entry, injected
//!   into the extension's own isolated world with Greasemonkey-style metadata
//!   so Chromium does the URL matching and `run_at` timing natively.

use super::manifest::{ContentScript, RunAt};
use super::registry::Extension;
use serde_json::{json, Value};

pub const API_CORE: &str = include_str!("js/api_core.js");
pub const PAGE_TRANSPORT: &str = include_str!("js/page_transport.js");
pub const CONTENT_TRANSPORT: &str = include_str!("js/content_transport.js");
pub const BRIDGE: &str = include_str!("js/bridge.js");
pub const CORS_SHIM: &str = include_str!("js/cors_shim.js");
/// Loaded right after Qt's qwebchannel.js (see the file).
pub const QWEBCHANNEL_PATCH: &str = include_str!("js/qwebchannel_patch.js");

/// Path (inside every extension origin) of the generated API script.
pub const API_PATH: &str = "__kite__/api.js";
/// Generated background page for `background.scripts`.
pub const BACKGROUND_PAGE: &str = "__kite__/background.html";

#[derive(Debug, Clone, PartialEq)]
pub struct UserScript {
    pub name: String,
    pub source: String,
    pub world: u32,
    pub all_frames: bool,
    /// 0 = DocumentCreation, 1 = DocumentReady, 2 = Deferred (QWebEngineScript::InjectionPoint)
    pub injection_point: u32,
}

pub fn ext_cfg(ext: &Extension, token: &str, kind: &str) -> Value {
    let messages: serde_json::Map<String, Value> = ext.messages.map.clone();
    json!({
        "id": ext.info.id,
        "uuid": ext.info.uuid,
        "token": token,
        "kind": kind,
        "manifest": ext.manifest.runtime_manifest(),
        "messages": messages,
        "locale": ext.messages.locale,
    })
}

fn with_cfg(transport: &str, cfg: &Value) -> String {
    // The cfg is JSON; escape `</script>` sequences defensively.
    let json = serde_json::to_string(cfg).unwrap().replace("</", "<\\/");
    transport.replace("__KITE_CFG__", &json)
}

/// `/__kite__/api.js` for extension pages. `blocking_port` is the
/// [`super::blocking::BlockingHub`] port (0: blocking webRequest unavailable).
pub fn page_api_js(ext: &Extension, token: &str, qwebchannel_js: &str, blocking_port: u16) -> String {
    let mut cfg = ext_cfg(ext, token, "page");
    cfg["blockingPort"] = blocking_port.into();
    format!("{qwebchannel_js}\n;{QWEBCHANNEL_PATCH}\n;{API_CORE}\n;{CORS_SHIM}\n;{}\n", with_cfg(PAGE_TRANSPORT, &cfg))
}

/// Code that makes sure `browser`/`chrome` exist in the extension's
/// content world (used as prelude for content scripts and executeScript).
pub fn content_prelude(ext: &Extension, token: &str) -> String {
    format!(
        "if (!globalThis.__kiteAPI) {{\n{API_CORE}\n;{}\n}}",
        with_cfg(CONTENT_TRANSPORT, &ext_cfg(ext, token, "content"))
    )
}

/// The bridge user script (world CONTENT_WORLD, main frame).
pub fn bridge_js(tokens: &[String], qwebchannel_js: &str) -> String {
    format!(
        "(function(){{\n{qwebchannel_js}\n;{QWEBCHANNEL_PATCH}\n;{}\n}})();",
        BRIDGE.replace("__KITE_TOKENS__", &serde_json::to_string(tokens).unwrap())
    )
}

/// Convert a match pattern into a Greasemonkey glob (for exclude_matches).
fn pattern_to_glob(p: &str) -> String {
    if p == "<all_urls>" {
        return "*".into();
    }
    p.replacen("*://", "http*://", 1)
}

fn metadata(name: &str, cs: &ContentScript) -> String {
    let mut h = String::from("// ==UserScript==\n");
    h += &format!("// @name {name}\n");
    for m in &cs.matches {
        h += &format!("// @match {m}\n");
    }
    for g in &cs.include_globs {
        h += &format!("// @include {g}\n");
    }
    for g in cs.exclude_globs.iter().cloned().chain(cs.exclude_matches.iter().map(|m| pattern_to_glob(m))) {
        h += &format!("// @exclude {g}\n");
    }
    h += match cs.run_at {
        RunAt::DocumentStart => "// @run-at document-start\n",
        RunAt::DocumentEnd => "// @run-at document-end\n",
        RunAt::DocumentIdle => "// @run-at document-idle\n",
    };
    h += "// ==/UserScript==\n";
    h
}

fn css_injector(css: &str) -> String {
    format!(
        "(function(){{const css={};const put=()=>{{const s=document.createElement('style');s.textContent=css;s.setAttribute('data-kite-ext','');(document.head||document.documentElement).appendChild(s);}};document.documentElement?put():new MutationObserver((m,o)=>{{if(document.documentElement){{o.disconnect();put();}}}}).observe(document,{{childList:true}});}})();\n",
        serde_json::to_string(css).unwrap()
    )
}

fn read(ext: &Extension, path: &str) -> String {
    let text = ext.resolve(path).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| {
        log::warn!("{}: missing content script file {path}", ext.info.id);
        String::new()
    });
    // CSS may contain __MSG_ placeholders and moz-extension://__MSG_@@extension_id__ URLs.
    if path.ends_with(".css") {
        ext.messages.localize(&text, &ext.info.uuid)
    } else {
        text
    }
}

/// Parse a dynamically registered content script (scripting.registerContentScripts
/// or contentScripts.register) into a ContentScript plus inline code.
fn dynamic_entry(ext: &Extension, v: &Value) -> (ContentScript, Vec<String>, Vec<String>) {
    let strs = |k: &str| -> Vec<String> { v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default() };
    let mut js_src = vec![];
    let mut css_src = vec![];
    for (key, out) in [("js", &mut js_src), ("css", &mut css_src)] {
        for item in v[key].as_array().into_iter().flatten() {
            match item {
                Value::String(f) => out.push(read(ext, f)),
                Value::Object(o) => {
                    if let Some(f) = o.get("file").and_then(Value::as_str) {
                        out.push(read(ext, f));
                    } else if let Some(c) = o.get("code").and_then(Value::as_str) {
                        out.push(c.to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    let run_at = match v["runAt"].as_str().or(v["run_at"].as_str()) {
        Some("document_start") => RunAt::DocumentStart,
        Some("document_end") => RunAt::DocumentEnd,
        _ => RunAt::DocumentIdle,
    };
    (
        ContentScript {
            matches: strs("matches"),
            exclude_matches: strs("excludeMatches"),
            include_globs: strs("includeGlobs"),
            exclude_globs: strs("excludeGlobs"),
            js: vec![],
            css: vec![],
            run_at,
            all_frames: v["allFrames"].as_bool().unwrap_or(false),
            match_about_blank: v["matchAboutBlank"].as_bool().unwrap_or(false),
            world: v["world"].as_str().unwrap_or("ISOLATED").to_owned(),
        },
        js_src,
        css_src,
    )
}

/// All user scripts for one enabled extension.
pub fn content_scripts(ext: &Extension, token: &str, world: u32, dynamic: &[Value]) -> Vec<UserScript> {
    let mut out = vec![];
    let prelude = content_prelude(ext, token);
    let mut entries: Vec<(String, ContentScript, Vec<String>, Vec<String>)> = ext
        .manifest
        .content_scripts
        .iter()
        .enumerate()
        .map(|(i, cs)| {
            let js = cs.js.iter().map(|f| read(ext, f)).collect();
            let css = cs.css.iter().map(|f| read(ext, f)).collect();
            (format!("kite:{}:{i}", ext.info.id), cs.clone(), js, css)
        })
        .collect();
    for (i, d) in dynamic.iter().enumerate() {
        let (cs, js, css) = dynamic_entry(ext, d);
        entries.push((format!("kite:{}:dyn{i}", ext.info.id), cs, js, css));
    }
    for (name, cs, js, css) in entries {
        if cs.matches.is_empty() {
            continue;
        }
        let main_world = cs.world.eq_ignore_ascii_case("MAIN");
        let mut src = metadata(&name, &cs);
        if !main_world {
            src += &prelude;
            src += "\n;";
        }
        if !css.is_empty() {
            src += &css_injector(&css.join("\n"));
        }
        for (i, f) in js.iter().enumerate() {
            // Each file is top-level script code so declarations are shared
            // across files and entries, like Firefox's per-extension sandbox.
            src += &format!("\n// ---- file {i} ----\n{f}\n;");
        }
        out.push(UserScript {
            name,
            source: src,
            world: if main_world { 0 } else { world },
            all_frames: cs.all_frames,
            injection_point: match cs.run_at {
                RunAt::DocumentStart => 0,
                RunAt::DocumentEnd => 1,
                RunAt::DocumentIdle => 2,
            },
        });
    }
    out
}

/// HTML for `background.scripts` extensions.
pub fn background_page(ext: &Extension) -> Option<String> {
    let bg = ext.manifest.background.as_ref()?;
    if bg.page.is_some() || bg.scripts.is_empty() {
        return None;
    }
    let ty = if bg.is_module { " type=\"module\"" } else { "" };
    let scripts: String = bg.scripts.iter().map(|s| format!("<script src=\"/{}\"{ty}></script>\n", html_escape(s))).collect();
    Some(format!("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><script src=\"/{API_PATH}\"></script>\n{scripts}</head><body></body></html>\n"))
}

/// URL of the background document, if any.
pub fn background_url(ext: &Extension) -> Option<String> {
    let bg = ext.manifest.background.as_ref()?;
    match &bg.page {
        Some(p) => Some(ext.url(p)),
        None if !bg.scripts.is_empty() => Some(ext.url(BACKGROUND_PAGE)),
        None => None,
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")
}

/// Insert the API script into an extension HTML document so `browser` exists
/// before any of the page's own scripts run.
pub fn inject_api_into_html(html: &str) -> String {
    let tag = format!("<script src=\"/{API_PATH}\"></script>");
    let lower = html.to_ascii_lowercase();
    // after <head ...>
    if let Some(i) = lower.find("<head") {
        if let Some(j) = lower[i..].find('>') {
            let at = i + j + 1;
            return format!("{}{tag}{}", &html[..at], &html[at..]);
        }
    }
    // after <html ...>
    if let Some(i) = lower.find("<html") {
        if let Some(j) = lower[i..].find('>') {
            let at = i + j + 1;
            return format!("{}{tag}{}", &html[..at], &html[at..]);
        }
    }
    // after the doctype (keep standards mode)
    if lower.trim_start().starts_with("<!doctype") {
        if let Some(j) = lower.find('>') {
            return format!("{}{tag}{}", &html[..=j], &html[j + 1..]);
        }
    }
    format!("{tag}{html}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webext::registry::Registry;
    use crate::webext::xpi::make_xpi;

    #[test]
    fn generate() {
        let d = tempfile::tempdir().unwrap();
        let mut r = Registry::open(d.path().into());
        r.install_xpi(
            &make_xpi(&[
                ("manifest.json", r#"{"manifest_version":2,"name":"G","version":"1","browser_specific_settings":{"gecko":{"id":"g@x"}},
                    "background":{"scripts":["a.js","b.js"]},
                    "content_scripts":[{"matches":["*://*.kde.org/*"],"exclude_matches":["*://bugs.kde.org/*"],"js":["c1.js","c2.js"],"css":["s.css"],"run_at":"document_start","all_frames":true}]}"#),
                ("c1.js", "const shared = 1;"),
                ("c2.js", "console.log(shared);"),
                ("s.css", "body{color:red}"),
            ]),
            "file",
            "",
        )
        .unwrap();
        let e = r.get("g@x").unwrap();
        let s = content_scripts(e, "tok", 11, &[]);
        assert_eq!(s.len(), 1);
        assert!(s[0].source.starts_with("// ==UserScript==\n// @name kite:g@x:0\n// @match *://*.kde.org/*\n// @exclude http*://bugs.kde.org/*\n// @run-at document-start\n"));
        assert!(s[0].source.contains("const shared = 1;") && s[0].source.contains("body{color:red}"));
        assert!(s[0].source.contains("\"token\":\"tok\""));
        assert_eq!((s[0].world, s[0].all_frames, s[0].injection_point), (11, true, 0));
        let bg = background_page(e).unwrap();
        assert!(bg.find("__kite__/api.js").unwrap() < bg.find("/a.js").unwrap());
        assert_eq!(background_url(e).unwrap(), e.url(BACKGROUND_PAGE));
        let dynamic = content_scripts(e, "tok", 11, &[serde_json::json!({"id": "x", "matches": ["<all_urls>"], "js": [{"code": "1+1"}], "runAt": "document_end"})]);
        assert_eq!(dynamic.len(), 2);
        assert!(dynamic[1].source.contains("1+1"));
        assert!(bridge_js(&["t1".into()], "/*qwc*/").contains("[\"t1\"]"));
    }

    #[test]
    fn html_injection() {
        assert_eq!(inject_api_into_html("<!DOCTYPE html><html><head><title>x</title></head></html>"), "<!DOCTYPE html><html><head><script src=\"/__kite__/api.js\"></script><title>x</title></head></html>");
        assert_eq!(inject_api_into_html("<!doctype html><p>hi"), "<!doctype html><script src=\"/__kite__/api.js\"></script><p>hi");
        assert!(inject_api_into_html("<body>x").starts_with("<script"));
    }
}
