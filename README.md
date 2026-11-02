# Kite

A KDE web browser written in Rust, modelled on [Falkon](https://invent.kde.org/network/falkon),
with **native support for Firefox WebExtensions** installed straight from
[addons.mozilla.org](https://addons.mozilla.org).

## Building

Requirements: Rust (rustup), Qt ≥ 6.9 with QtWebEngine (Core + Quick runtime), Kirigami (KF6),
qqc2-desktop-style. QtWebEngineQuick *development* headers are optional.

```sh
cargo build --release
./target/release/kite [URL...]            # --profile NAME, --private
cargo test --workspace && node --test crates/kite-core/src/webext/js/api_core.test.js
```

Developer tools:

```sh
cargo run -p kite-core --example amo-install -- darkreader          # install an add-on headlessly
cargo run -p kite-core --example dnr-check -- <ext-dir> <url>       # evaluate DNR rulesets
```

## Architecture

```
crates/kite-core   pure Rust, unit-tested, no Qt
  history, bookmarks (Falkon bookmarks.json format), browsedata.db (Falkon schema),
  sessions, search engines + location-bar logic, site settings, downloads,
  adblock (Brave's engine, EasyList/EasyPrivacy, cached compiled engine)
  webext/  amo (AMO API client, sha256-verified downloads), xpi, registry,
           manifest (MV2+MV3), i18n, match patterns, storage, dnr,
           runtime (the WebExtension API backend), scripts + js/ (browser.* API)
crates/kite        the application
  src/  cxx-qt QML singletons `Browser` and `Extensions`, C ABI for the glue
  cpp/  thin C++: QtWebEngine init, moz-extension:/kite: scheme handlers,
        request interceptor, cookie store hooks
  qml/  UI: windows, tabs, location bar, menus, library, downloads,
        preferences, add-ons manager (Kirigami / Breeze)
```

### How WebExtensions work

QtWebEngine's built-in Chromium extension support only covers a handful of APIs
(no `tabs`, `windows`, `menus`, `browserAction`, …), so Kite implements the
platform itself, with Firefox semantics:

* Extensions are served from `moz-extension://<random-uuid>/`; HTML pages get the
  API injected; `web_accessible_resources` are enforced against the requesting origin.
* Background scripts run in a hidden DOM page (Firefox-style event page, also for MV3).
* Each extension's content scripts run in **their own isolated world**; Chromium does
  the `matches`/`run_at` scheduling natively. Files share one global scope as in Firefox.
* A privileged bridge world in each tab owns the QWebChannel and relays calls; each
  extension talks to it over DOM events named with a secret per-session token, so
  neither the page nor other extensions can see or forge its traffic. The token also
  authenticates every call in the Rust runtime.
* `browser.*` is promise-based; `chrome.*` additionally supports callbacks and `lastError`.
* Cross-origin `fetch`/XHR from extension pages to permitted hosts bypass CORS
  (performed natively), like Firefox.
* `declarativeNetRequest` rules are evaluated in the request interceptor.

Supported namespaces: runtime, extension, i18n, storage (local/sync/session/managed),
tabs, windows, browserAction/action/pageAction, sidebarAction, menus/contextMenus,
scripting, contentScripts, alarms, permissions, management, commands, cookies,
webNavigation, webRequest (observing), declarativeNetRequest, history, bookmarks,
downloads, notifications, topSites, sessions, search, idle, theme, omnibox,
browsingData, identity, dom, plus read-only privacy/proxy/browserSettings stubs.

Verified with unmodified AMO add-ons: **Dark Reader** (background, storage, tabs,
content scripts, popup UI), **uBlock Origin Lite** (DNR blocking incl. surrogate
redirects), **uBlock Origin** (loads, downloads and compiles its filter lists).

## Known limitations

* Blocking `webRequest` listeners are called but cannot cancel/modify requests (Qt's
  interceptor is synchronous). Classic uBlock Origin therefore doesn't block;
  use uBO Lite or Kite's built-in content blocker.
* Content scripts in sub-frames run, but only synchronous APIs work there.
* `webRequest.filterResponseData`, native messaging, `runtime.getBackgroundPage`,
  containers, `find`, `dns` and themes are not implemented.
* Extensions are disabled in private windows (Firefox default); no per-extension toggle yet.
* Falkon features not yet ported: password manager/autofill, named session manager,
  speed-dial editing, cookie/certificate managers, proxy settings, mouse gestures,
  vertical/tree tabs, KWallet/KIO integration, user stylesheets.
