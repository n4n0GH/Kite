//! Compatibility shims for extension code written against Firefox internals.
//!
//! uBlock Origin injects its scriptlets (the page-world code that strips
//! e.g. YouTube's video ads) from a content script by inserting an inline
//! `<script>` element, then checks it ran through `window.wrappedJSObject`.
//! Firefox and Chrome both exempt scripts inserted by extension content
//! scripts from the page's CSP and Trusted Types; QtWebEngine's isolated
//! worlds have no such exemption, so on sites like YouTube the insertion is
//! refused, and `wrappedJSObject` doesn't exist at all.
//!
//! Scripts Kite itself runs in the main world (user scripts and
//! `runJavaScript`) are not subject to CSP. So uBO's injector is recognised
//! and its scriptlets are run in the main world directly, while the
//! content-script side only records that they were injected.

use serde_json::Value;

/// uBO's Firefox scriptlet injector, split apart.
#[derive(Debug, Clone, PartialEq)]
pub struct MainWorldInjection {
    /// The code with the injector replaced by an isolated-world stub.
    pub isolated: String,
    /// Code to run in the page's main world.
    pub main: String,
}

const MARKER: &str = "self.uBO_scriptletsInjected !== undefined";
const ARG_START: &str = ")({\"hostname\":";

/// Find uBO's scriptlet injector in `code` (as passed to
/// `contentScripts.register` / `tabs.executeScript`).
pub fn split_main_world_injector(code: &str) -> Option<MainWorldInjection> {
    let marker = code.find(MARKER)?;
    if !code.contains("wrappedJSObject") {
        return None;
    }
    let start = code[..marker].rfind("(function(details)")?;
    let arg = marker + code[marker..].find(ARG_START)? + 2;
    let mut it = serde_json::Deserializer::from_str(&code[arg..]).into_iter::<Value>();
    let details = it.next()?.ok()?;
    let rest = &code[arg + it.byte_offset()..];
    let rest = rest.strip_prefix(");").or_else(|| rest.strip_prefix(')'))?;
    let end = code.len() - rest.len();

    let hostname = details["hostname"].as_str().unwrap_or_default();
    let scriptlets = details["scriptlets"].as_str().unwrap_or_default();
    let host = serde_json::to_string(hostname).unwrap();
    let filters = serde_json::to_string(&details["filters"]).unwrap();
    // Run once per document even if both the registered script and
    // executeScript deliver it.
    let guard = serde_json::to_string(&format!("__kiteScriptlets{:016x}", fnv1a(scriptlets))).unwrap();
    let main = format!(
        "(function() {{\n\
         if ({host} !== '' && location.hostname !== {host}) return;\n\
         if (Object.prototype.hasOwnProperty.call(self, {guard})) return;\n\
         Object.defineProperty(self, {guard}, {{ value: true }});\n\
         {scriptlets}\n\
         }})();"
    );
    let stub = format!("if (self.uBO_scriptletsInjected === undefined && ({host} === '' || location.hostname === {host})) self.uBO_scriptletsInjected = {filters};");
    Some(MainWorldInjection { isolated: format!("{}{stub}{}", &code[..start], &code[end..]), main })
}

fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Shaped like uBO's `vAPI.scriptletsInjector` output.
    fn injector(details: &Value) -> String {
        format!(
            "(function(details) {{\n            if ( self.uBO_scriptletsInjected !== undefined ) {{ return; }}\n            const s = document.createElement('script');\n            if ( self.wrappedJSObject[details.sentinel] ) {{ return 0; }}\n        }})({});",
            serde_json::to_string(details).unwrap()
        )
    }

    #[test]
    fn splits_ubo_injector() {
        let d = json!({"hostname": "www.youtube.com", "scriptlets": "try { setConstant('a', '}); ') } catch {}", "filters": "youtube.com##+js(x)", "sentinel": "abc"});
        let code = format!("// logger\n\n{}\n\n// isolated part", injector(&d));
        let r = split_main_world_injector(&code).unwrap();
        assert!(r.isolated.starts_with("// logger\n\nif (self.uBO_scriptletsInjected === undefined"));
        assert!(r.isolated.ends_with("self.uBO_scriptletsInjected = \"youtube.com##+js(x)\";\n\n// isolated part"));
        assert!(!r.isolated.contains("wrappedJSObject"));
        assert!(r.main.contains("try { setConstant('a', '}); ') } catch {}"));
        assert!(r.main.contains("location.hostname !== \"www.youtube.com\""));
    }

    #[test]
    fn leaves_other_code_alone() {
        assert_eq!(split_main_world_injector("console.log(1)"), None);
        assert_eq!(split_main_world_injector("if (self.uBO_scriptletsInjected !== undefined) {}"), None);
    }
}
