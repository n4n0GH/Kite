//! The first-launch page, kite:welcome: importing bookmarks from another browser,
//! choosing how private Kite should be, and a moment while that's set up.
//!
//! A level is only a set of values for settings that also stand on their own in
//! Settings, where each can be changed later. High is Kite's defaults.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Low,
    Medium,
    High,
    Extreme,
}

impl Level {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "extreme" => Self::Extreme,
            _ => return None,
        })
    }
}

/// The settings a level sets.
pub fn settings_for(level: Level) -> Vec<(&'static str, Value)> {
    use Level::*;
    let at = |l: Level| level as u8 >= l as u8;
    let extreme = level == Extreme;
    vec![
        ("privacy.saveHistory", json!(!extreme)),
        ("privacy.clearOnExit", json!(extreme)),
        // Private windows keep what's on the screen private from Medium up.
        ("privacy.privateHistoryTiles", json!(level == Low)),
        ("privacy.privateHistorySuggestions", json!(level == Low)),
        ("privacy.privateNotifications", json!(level == Low)),
        ("privacy.cookies", json!(match level {
            Low | Medium => "all",
            High => "noThirdParty",
            Extreme => "none",
        })),
        // With every cookie blocked, sites have to be let in one by one.
        ("privacy.siteCookieRules", json!(extreme)),
        ("privacy.globalPrivacyControl", json!(at(High))),
        ("web.doNotTrack", json!(at(High))),
        ("privacy.stripTrackingParams", json!(at(High))),
        ("privacy.blockPings", json!(at(High))),
        ("privacy.sendReferer", json!(!extreme)),
        // Hides that this is QtWebEngine at every level: telling it would only
        // make Kite's users easier to pick out.
        ("privacy.genericUserAgent", json!(true)),
        ("web.googleSignIn", json!(!extreme)),
        ("privacy.webrtcPublicOnly", json!(at(High))),
        ("privacy.antiFingerprinting", json!(extreme)),
        ("privacy.httpsOnly", json!(at(Medium))),
        ("privacy.dnsOverHttps", json!("off")),
        ("privacy.containers", json!(extreme)),
        ("extensions.enabled", json!(!extreme)),
        ("adblock.enabled", json!(at(Medium))),
    ]
}

const STYLE: &str = include_str!("welcome_page.css");

/// The page. `token` is what its messages have to come with; `nonce` lets only its
/// own script run. `sources` are the browsers found to import from.
pub fn page(token: &str, nonce: &str, sources: &Value) -> String {
    let data = json!({"token": token, "sources": sources}).to_string().replace('<', "\\u003c");
    let script = include_str!("welcome_page.js");
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Welcome to Kite</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-{nonce}'">
<meta name="color-scheme" content="light dark"><style>{STYLE}</style></head><body>
<main>
<ol class="steps" aria-label="Steps"><li class="on">Import</li><li>Privacy</li><li>Ready</li></ol>

<section id="import">
<h1>Welcome to Kite</h1>
<p class="lead">Would you like to bring your bookmarks along from another browser?</p>
<div id="sources" role="radiogroup" aria-label="Browsers to import from"></div>
<p id="none" class="note" hidden>No other browsers were found on this computer. If yours can export its bookmarks to a file, you can import that later from the Library.</p>
<p class="note" id="where" hidden>They go into a folder of their own in the Bookmarks Menu.</p>
<footer><button id="skip-import">Skip</button><button id="do-import" class="primary" disabled>Import</button></footer>
</section>

<section id="privacy" hidden>
<h1>How private should Kite be?</h1>
<p class="lead">Move the slider to choose a level. Each of these settings can be changed on its own later, in Kite's Settings.</p>
<div class="slider">
<input id="level" type="range" min="0" max="3" step="1" value="2" aria-describedby="about">
<div class="marks" aria-hidden="true"><span>Low</span><span>Medium</span><span>High</span><span>Extreme</span></div>
</div>
<div id="about" aria-live="polite">
<h2 id="level-name"></h2>
<p id="level-text"></p>
<ul id="level-points"></ul>
<p id="level-warning" class="warning" hidden></p>
<label id="ack" class="ack" hidden><input id="ack-box" type="checkbox"> <span id="ack-text"></span></label>
</div>
<footer><button id="back" class="back">Back</button><button id="skip-level">Skip</button><button id="use-level" class="primary">Use High Level</button></footer>
</section>

<section id="ready" hidden>
<div id="working">
<div class="spinner" aria-hidden="true"></div>
<h1>Setting up your new browser…</h1>
</div>
<div id="done" hidden>
<h1>All done. Enjoy!</h1>
</div>
</section>
</main>
<script id="data" type="application/json">{data}</script>
<script nonce="{nonce}">{script}</script>
</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_is_kites_defaults() {
        let defaults = crate::settings::defaults();
        for (key, value) in settings_for(Level::High) {
            assert_eq!(defaults.get(key), Some(&value), "{key}");
        }
    }

    #[test]
    fn every_level_sets_the_same_existing_settings() {
        let defaults = crate::settings::defaults();
        let keys = |l| settings_for(l).into_iter().map(|(k, _)| k).collect::<Vec<_>>();
        for level in [Level::Low, Level::Medium, Level::High, Level::Extreme] {
            assert_eq!(keys(level), keys(Level::High));
            for (key, value) in settings_for(level) {
                let default = defaults.get(key).unwrap_or_else(|| panic!("{key} isn't a setting"));
                assert_eq!(std::mem::discriminant(default), std::mem::discriminant(&value), "{key}");
            }
        }
    }

    #[test]
    fn levels_from_the_page() {
        assert_eq!(Level::parse("extreme"), Some(Level::Extreme));
        assert_eq!(Level::parse("High"), None);
        let low = settings_for(Level::Low);
        assert!(low.contains(&("adblock.enabled", json!(false))) && low.contains(&("privacy.httpsOnly", json!(false))));
        let extreme = settings_for(Level::Extreme);
        assert!(extreme.contains(&("privacy.cookies", json!("none"))) && extreme.contains(&("extensions.enabled", json!(false))));
        assert!(extreme.contains(&("privacy.genericUserAgent", json!(true))));
    }
}
