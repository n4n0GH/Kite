# Kite

A KDE web browser written in Rust, modelled on [Falkon](https://invent.kde.org/network/falkon),
with **native support for Firefox WebExtensions** installed straight from
[addons.mozilla.org](https://addons.mozilla.org).

How Kite compares with Chrome and Firefox: [FEATURE_COMPARISON.md](FEATURE_COMPARISON.md).

## Features

* **Tabs:** the tab bar above or below the address bar, below the page, or as a vertical
  list on either side (hidden with a single tab if wanted); pinned tabs, scroll arrows and
  wheel scrolling, drag to reorder, links/images/text dropped on the bar open in new tabs.
* **Tree-style tabs (optional, for vertical lists):** links open as children of their tab,
  shown nested with guide lines; trees collapse, move and close as a whole, and dragging a tab
  sideways changes its level (Settings or right-click the tab bar).
* **Tab search:** the tab bar's list button or Ctrl+Shift+E searches the open tabs of all
  windows by title and address (private and normal windows aren't mixed).
* **Tab groups:** named and coloured, collapsible; shown as a group tab on horizontal bars
  and as a file tree in vertical lists; dragged as a whole. Ctrl/Shift+click selects
  several tabs for the tab menu.
* **Split view:** two pages side by side, resizable, each with its own tabs (the tab bar
  splits with them); "Open Link in New Split Tab".
* **Toolbar and menus:** customizable toolbar, optional classic menu bar (Alt shows it),
  bookmarks toolbar, an extensions panel with pin/unpin.
* **Keyboard shortcuts:** every Kite action can get its own keys (Settings → Shortcuts),
  with conflict warnings and reset to the defaults. Extensions keep the keys they declare.
* **Picture-in-picture:** a page's video in a small window above others (address bar
  button, the video's context menu or Ctrl+Shift+]); QtWebEngine has none, so the page
  moves into the window and shows only the video.
* **Reader view:** an article without the rest of the page (address bar button, the View menu
  or Ctrl+Alt+R), with its own text size, line length, font and colours; see below.
* **Memory saver:** tabs not shown for 10 minutes (Firefox's threshold; adjustable in
  Settings → Browsing) are unloaded and reload when shown again; tabs playing sound, with
  unsent form input or keeping the screen on stay loaded. Also from the tab menu and
  `tabs.discard()`.
* **Home and new tab page:** an address or a local HTML file, used in place or kept as a copy
  in the profile (`kite:startpage/…`); saving the file reloads it.
* **Site settings:** per-site zoom, forced dark mode, custom CSS and JavaScript, cookie rules
  and fingerprint protection, from Settings or the address bar.
* **Privacy:** third-party cookies blocked by default, Global Privacy Control, tracking
  parameters removed from addresses, link pings dropped, a common user agent, WebRTC limited
  to public addresses, HTTPS-only mode, DNS over HTTPS, a proxy (SOCKS5/HTTP), container
  tabs, per-site cookie rules and fingerprint protection; each can be switched separately.
* **Dangerous-site warnings (opt-in, off by default):** pages and downloads checked against
  Google Safe Browsing (with your own API key; only hash prefixes leave the computer) or the
  open malware-filter phishing/URLhaus lists (checked entirely locally).
* **Content blocking:** built in (Brave's engine with EasyList/EasyPrivacy), paused while an
  extension such as uBlock Origin does the blocking.
* **Passwords:** see below.
* **Desktop:** Kirigami/Breeze UI following the KDE colour scheme and icon theme, KDE
  notifications, Chromium's overlay scrollbars shown when the pointer nears the edge, Screen
  Wake Lock through the desktop's screensaver inhibition.

## Building

Requirements: Rust (rustup), Qt ≥ 6.9 with QtWebEngine (Core + Quick runtime), Kirigami (KF6),
qqc2-desktop-style. QtWebEngineQuick *development* headers are optional.

```sh
cargo build --release
./target/release/kite [URL...]            # --profile NAME, --private
cargo test --workspace && node --test crates/kite-core/src/webext/js/api_core.test.js crates/kite/qml/tabtree.test.js
```

Developer tools:

```sh
cargo run -p kite-core --example amo-install -- darkreader          # install an add-on headlessly
cargo run -p kite-core --example dnr-check -- <ext-dir> <url>       # evaluate DNR rulesets
```

### AppImage

```sh
packaging/appimage/build-appimage.sh            # release build + bundle → dist/Kite-<version>-<arch>.AppImage
packaging/appimage/build-appimage.sh --no-build # bundle the existing target/release/kite
```

Bundles Qt, QtWebEngine and the KDE QML modules Kite uses (including the `org.kde.desktop` style,
Kirigami's desktop platform plugin and the Wayland client plugins) with
[linuxdeploy](https://github.com/linuxdeploy/linuxdeploy) and linuxdeploy-plugin-qt. Both are
downloaded into `target/appimage-tools` on first use; set `LINUXDEPLOY` / `LINUXDEPLOY_PLUGIN_QT`
to use your own copies, and `QMAKE` to pick the Qt to bundle. An AppImage only runs on systems whose
glibc is at least as new as the build system's, so build on an older distribution (e.g. in a
container) for wider compatibility.

## Architecture

```
crates/kite-core   pure Rust, unit-tested, no Qt
  history, bookmarks (Falkon bookmarks.json format), browsedata.db (Falkon schema),
  sessions, search engines + location-bar logic, site settings, downloads, settings,
  adblock (Brave's engine, EasyList/EasyPrivacy, cached compiled engine),
  privacy (tracking-parameter stripping, HTTPS-only upgrades), startpage (custom start pages),
  safebrowsing (Google Update API v4 + open lists)
  webext/  amo (AMO API client, sha256-verified downloads), xpi, registry,
           manifest (MV2+MV3), i18n, match patterns, storage, dnr,
           runtime (the WebExtension API backend), scripts + js/ (browser.* API)
crates/kite        the application
  src/  cxx-qt QML singletons `Browser` and `Extensions`, C ABI for the glue
  cpp/  thin C++: QtWebEngine init, moz-extension:/kite: scheme handlers,
        request interceptor, cookie store hooks and filter, DNS over HTTPS,
        input filter (menu bar on Alt, drags over the window, scrollbar reveal)
  src/js/  page scripts (Screen Wake Lock shim, scrollbar styling, fingerprint protection)
  icons/   Kite's own icons (a fallback icon theme in the resources)
  qml/  UI: windows, tabs and tab groups, split view, location bar, menus, library,
        downloads, preferences, add-ons manager (Kirigami / Breeze)
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
* `declarativeNetRequest` rules are evaluated in the request interceptor; blocking
  `webRequest.onBeforeRequest` listeners are asked too and can cancel or redirect.
* Content scripts in iframes reach the runtime through the extension's own origin
  (the bridge only lives in the top frame).

Supported namespaces: runtime, extension, i18n, storage (local/sync/session/managed),
tabs, windows, browserAction/action/pageAction, sidebarAction, menus/contextMenus,
scripting, contentScripts, alarms, permissions, management, commands, cookies,
webNavigation, webRequest (incl. blocking onBeforeRequest), declarativeNetRequest, history, bookmarks,
downloads, notifications, topSites, sessions, search, idle, theme, omnibox,
browsingData, identity, dom, plus read-only privacy/proxy/browserSettings stubs.

Verified with unmodified AMO add-ons: **Dark Reader** (background, storage, tabs,
content scripts, popup UI), **uBlock Origin** (blocking webRequest, scriptlets in the
page's main world, popup), **uBlock Origin Lite** (DNR blocking incl. surrogate
redirects), **Vimium** (content scripts in every frame).

## Password manager

Port of Falkon's AutoFill: login forms are detected (including script-driven
and late-rendered ones) in the privileged bridge world, Kite offers to save or
update, and fills saved logins automatically or from the key icon in the
location bar. HTTP authentication can be remembered too.

* **Storage:** Falkon's plain `autofill` table (Falkon-compatible), encrypted
  with a master password (Argon2id + XChaCha20-Poly1305; key only in memory), or
  **KDE Wallet** over D-Bus, using Falkon's `FalkonPasswords` folder so both
  browsers share logins. Switching storage moves saved logins along.
* **Password generator:** sign-up and change-password fields offer a strong random
  password (length and symbols configurable; look-alike characters left out), filled
  into the confirmation field too; also in the login editor.

## Form autofill

Addresses, names, emails and phone numbers (Settings → Browsing → Forms, on by
default). Fields are recognised by their `autocomplete` tokens or, failing that, by
name/label; focusing one shows the saved entries under it (arrow keys and Enter
work while the page keeps focus), and picking one fills the visible fields of that
form, including country drop-downs. Addresses typed into submitted forms can be
saved or update an existing entry; never from private windows.

* **Storage:** each entry sealed whole with XChaCha20-Poly1305 in `browsedata.db`;
  the random key is kept in KDE Wallet (folder `Kite`) or, without KDE Wallet, in an
  owner-only `formfill.key` file in the profile.
* **Safety:** credentials are only released for the host of the tab's actual URL
  (taken from the view, never from page JavaScript), only to http(s) pages, never
  to extension pages; nothing is saved from private windows.
* **Import/export:** Firefox/Chrome/Bitwarden CSV and Falkon XML; exports are 0600.

## Private windows

Their pages stay out of history, the saved session, closed tabs, the favicon cache and site
settings (zoom applies to the tab only). What is typed in the address bar isn't sent for search
suggestions. Downloads are listed until the last private window closes, never saved to the list
(the files themselves stay). Their start page and address bar show only the tiles and bookmarks
you added, unless Settings → Privacy says to show pages from the history there too. Only extensions allowed there run or see anything (below). Things
done on purpose are kept: bookmarks, site settings and
content blocker exceptions.

## Reader view

The page's article is extracted with Mozilla's
[Readability](https://github.com/mozilla/readability) (`crates/kite/src/js/readability/`,
Apache-2.0) in an isolated world, and shown at a `kite:reader` address, one step in the tab's
history. The address bar, bookmarks, the saved session and extensions go on seeing the page's
own address; a restored session reopens the page itself.

* **Safety:** `kite:` is a privileged origin, so the article is sanitised in Rust (ammonia, with
  an allow list: no scripts, styles, frames or forms; links and images on the web only), and the
  page's Content-Security-Policy forbids scripts as well. Articles stay in memory under random
  ids, so a web page can't link to one and pass it off as another site's.
* **Left out:** videos and other embedded content, and images given as `data:` addresses. Images
  are requested without a referrer, so sites refusing those won't show theirs.

## Known limitations

* Blocking `webRequest` listeners are only asked about `onBeforeRequest`, and must answer
  within 250 ms (Qt's request interceptor is synchronous); header changes from blocking
  `onBeforeSendHeaders`/`onHeadersReceived` listeners aren't applied.
* `webRequest.filterResponseData`, native messaging, `runtime.getBackgroundPage`, the
  containers (`contextualIdentities`) API, `find`, `dns`, `tabGroups` and themes are not
  implemented. Tabs report their container (`cookieStoreId`), but extensions can't list or
  manage containers, and the `cookies` API covers the main container only.
* Extensions are off in private windows by default (like Firefox): they don't run there and can't
  see their tabs or requests, and the built-in content blocker stands in for uBlock Origin. Each
  can be allowed in its details in the add-ons manager. Taking that back leaves its content
  scripts in open private pages until they are reloaded, cut off from the extension.
* Password manager and form autofill: fields inside iframes are not detected; no card autofill.
* Picture-in-picture: videos inside frames (embedded players) are not offered, and pages'
  own picture-in-picture buttons (`requestPictureInPicture()`) don't work. On Wayland the
  window can only stay on top with a KWin window rule ("Keep above" for the title
  "Picture-in-Picture").
* Not available: sync, translation, passkeys (WebAuthn),
  card autofill, browser themes, a profile picker (profiles are chosen with
  `--profile`).
* Proxy changes, overlay scrollbars and the Chromium flags behind them apply after a restart.
* Falkon features not yet ported: named session manager, speed-dial editing,
  cookie/certificate managers, mouse gestures, KIO integration.
