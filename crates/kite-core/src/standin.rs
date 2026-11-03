//! Stand-in responses for blocked requests (empty scripts, 1×1 images, ...)
//! that content blockers redirect to as `data:` URLs.
//!
//! Chromium refuses to redirect subresources to `data:` URLs, so Kite serves
//! them from its own `kite-res:` scheme instead. That scheme ignores the
//! page's CSP (like `moz-extension:`), so it must not serve content taken
//! from the URL — an injected script could then bypass a site's CSP. Only
//! stand-ins Kite registered itself are served, by content hash.

use base64::Engine;
use std::collections::HashMap;
use std::sync::Mutex;

pub const SCHEME: &str = "kite-res";
/// Registered stand-ins are few (blocker resources); this only bounds memory.
const MAX: usize = 512;

/// Parse `data:[<mime>][;base64],<data>`.
pub fn parse_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let (meta, base64) = match meta.strip_suffix(";base64") {
        Some(m) => (m, true),
        None => (meta, false),
    };
    let mime = if meta.is_empty() { "text/plain;charset=US-ASCII".to_owned() } else { meta.to_owned() };
    let bytes = if base64 {
        base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?
    } else {
        percent_encoding::percent_decode_str(data).collect()
    };
    Some((mime, bytes))
}

#[derive(Default)]
pub struct StandIns {
    map: Mutex<HashMap<String, (String, Vec<u8>)>>,
}

impl StandIns {
    /// Register a `data:` URL; returns the `kite-res:` URL serving it.
    pub fn register(&self, data_url: &str) -> Option<String> {
        let (mime, bytes) = parse_data_url(data_url)?;
        let id = format!("{:016x}", fnv1a(data_url.as_bytes()));
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX && !map.contains_key(&id) {
            map.clear();
        }
        map.insert(id.clone(), (mime, bytes));
        Some(format!("{SCHEME}:{id}"))
    }

    /// (mime, body) for a `kite-res:` URL Kite registered.
    pub fn get(&self, url: &str) -> Option<(String, Vec<u8>)> {
        let id = url.strip_prefix(SCHEME)?.strip_prefix(':')?;
        self.map.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
    }
}

fn fnv1a(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_data_urls() {
        assert_eq!(parse_data_url("data:text/javascript;base64,KGZ1bmN0aW9uKCl7fSkoKTs="), Some(("text/javascript".into(), b"(function(){})();".to_vec())));
        assert_eq!(parse_data_url("data:,a%20b"), Some(("text/plain;charset=US-ASCII".into(), b"a b".to_vec())));
        assert_eq!(parse_data_url("data:image/gif;base64,!!"), None);
        assert_eq!(parse_data_url("https://x/"), None);
    }

    #[test]
    fn serves_only_registered_stand_ins() {
        let s = StandIns::default();
        let url = s.register("data:text/plain;base64,Cg==").unwrap();
        assert!(url.starts_with("kite-res:"));
        assert_eq!(s.get(&url), Some(("text/plain".into(), b"\n".to_vec())));
        assert_eq!(s.register("data:text/plain;base64,Cg==").unwrap(), url);
        // Nothing else: content can't be smuggled in through the URL.
        assert_eq!(s.get("kite-res:0000000000000000"), None);
        assert_eq!(s.get("kite-res:text/javascript,alert(1)"), None);
    }
}
