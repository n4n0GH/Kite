//! Native WebExtension runtime.
//!
//! Kite does not rely on Chromium's (very partial) extension support in
//! QtWebEngine. Instead it implements the WebExtension platform itself, with
//! Firefox semantics, the way Firefox does:
//!
//! * `amo` / `xpi` / `registry` — find add-ons on addons.mozilla.org, download
//!   and unpack `.xpi` files, keep track of installed extensions.
//! * `manifest` / `i18n` / `matchpattern` — parse `manifest.json` (MV2 & MV3),
//!   `_locales` messages and URL match patterns / globs.
//! * `scripts` — generate the `browser.*` API shim injected into extension
//!   pages (served under `moz-extension://<uuid>/`) and into an isolated
//!   world of every web page for content scripts.
//! * `runtime` — the API backend: messaging and ports, tabs/windows mirror,
//!   storage, alarms, menus, browser actions, commands, permissions, cookies,
//!   history, bookmarks, webRequest events, declarativeNetRequest, ...
//!   UI-dependent calls (opening tabs, running scripts, popups) are forwarded
//!   to the GUI layer as [`runtime::Effect::Ui`].
//! * `blocking` — blocking webRequest listeners (off-UI-thread WebSocket hub).
//! * `dnr` — declarativeNetRequest rule matching for the network interceptor.

pub mod amo;
pub mod blocking;
pub mod dnr;
pub mod i18n;
pub mod manifest;
pub mod matchpattern;
pub mod registry;
pub mod runtime;
pub mod scripts;
pub mod storage;
pub mod xpi;

/// URL scheme used for extension resources, as in Firefox.
pub const SCHEME: &str = "moz-extension";
/// World id the content-script isolated world uses (QWebEngineScript::ApplicationWorld).
pub const CONTENT_WORLD: u32 = 1;
