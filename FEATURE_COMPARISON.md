# Feature comparison: Kite, Chrome, Firefox

Desktop features, compared on Linux (Kite's only platform).

**Legend:** ✅ built in · ⚠️ partial, hidden or experimental, or only through an extension · ❌ no

The **Kite** column reflects Kite's code as of commit `86bf5ac` (2026-11-04).
The **Chrome** and **Firefox** columns reflect their stable releases as known in mid-2026. Both
ship changes every four weeks, so check the newest versions before relying on a single row.

## Platform

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Rendering engine | QtWebEngine (Chromium 140) | Blink (latest Chromium) | Gecko | Kite's engine trails Chrome by several releases. |
| Operating systems | Linux | Windows, macOS, Linux, ChromeOS | Windows, macOS, Linux | |
| Mobile apps | ❌ | ✅ | ✅ | |
| Open source | ✅ | ⚠️ | ✅ | Chromium is open source; Chrome adds closed parts. |
| UI toolkit | Qt/Kirigami (KDE) | Custom (Views) | Custom (XUL/HTML) | |

## Tabs and windows

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Tab bar above the address bar | ✅ | ✅ | ✅ | |
| Tab bar below the address bar | ✅ | ❌ | ⚠️ | Firefox: only with userChrome.css. |
| Tab bar below the page | ✅ | ❌ | ⚠️ | Firefox: only with userChrome.css. |
| Vertical tabs, left side | ✅ | ⚠️ | ✅ | Chrome's vertical tabs are experimental. |
| Vertical tabs, right side | ✅ | ❌ | ✅ | |
| Hide the tab bar with a single tab | ✅ | ❌ | ⚠️ | Firefox: only through userChrome.css. |
| Tab groups (name, colour, collapse) | ✅ | ✅ | ✅ | |
| Groups as a folder tree in vertical tabs | ✅ | ❌ | ❌ | IDE-style headers, indentation, guide lines. |
| Drag whole groups | ✅ | ✅ | ✅ | |
| Split view (two pages side by side) | ✅ | ✅ | ⚠️ | Recent in Chrome; Firefox's is new or in testing. |
| Split tab bar per pane | ✅ | ❌ | ❌ | Each pane gets its own strip of tabs. |
| "Open link in new split tab" | ✅ | ✅ | ⚠️ | |
| Select several tabs (Ctrl/Shift+click) | ✅ | ✅ | ✅ | |
| Pinned tabs | ✅ | ✅ | ✅ | |
| Mute tab | ✅ | ✅ | ✅ | |
| Container tabs (separate cookies) | ✅ | ❌ | ⚠️ | Firefox: through Mozilla's Multi-Account Containers add-on. Chrome uses profiles instead. |
| Reopen closed tabs and windows | ✅ | ✅ | ✅ | |
| List all tabs / tab search | ✅ | ✅ | ✅ | Kite has a list; Chrome and Firefox also search it. |
| Drop links, images or text on the tab bar | ✅ | ✅ | ✅ | Kite also shows a hidden tab bar while dragging. |
| Session restore | ✅ | ✅ | ✅ | Kite also keeps split view and tab groups. |
| Named sessions | ❌ | ❌ | ❌ | All need an extension. |
| Unload inactive tabs (memory saver) | ⚠️ | ✅ | ✅ | Kite only loads restored tabs when first shown. |
| Picture-in-picture video | ❌ | ✅ | ✅ | |
| Profiles | ⚠️ | ✅ | ✅ | Kite: `--profile NAME` on the command line, no profile picker. |
| Private windows | ✅ | ✅ | ✅ | |

## Address bar and navigation

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Suggestions from history and bookmarks | ✅ | ✅ | ✅ | |
| Search suggestions | ✅ | ✅ | ✅ | |
| Several search engines, shortcuts | ✅ | ✅ | ✅ | |
| Bookmark keywords | ✅ | ⚠️ | ✅ | Chrome uses site search shortcuts instead. |
| Site info popup with permissions | ✅ | ✅ | ✅ | |
| Per-site settings button in the address bar | ✅ | ❌ | ❌ | Kite's gear opens the site's own settings. |
| Per-site zoom | ✅ | ✅ | ✅ | |
| Smooth scrolling setting | ✅ | ⚠️ | ✅ | Chrome: behind a flag. |
| Overlay scrollbars | ✅ | ⚠️ | ✅ | Chrome: depends on the platform. Kite: Chromium's overlay style plus reveal near the edge. |

## Start page and customisation

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Start page with frequently visited sites | ✅ | ✅ | ✅ | |
| Local HTML file as home page | ✅ | ⚠️ | ✅ | Chrome accepts a file: URL as the home page. |
| Local HTML file as new tab page | ✅ | ❌ | ❌ | Chrome and Firefox need an extension. |
| Start page copy kept in the browser, live reload on save | ✅ | ❌ | ❌ | |
| Customisable toolbar | ✅ | ⚠️ | ✅ | Chrome: pinning a few buttons only. |
| Classic menu bar | ✅ | ❌ | ✅ | Kite and Firefox show it briefly on Alt. |
| Bookmarks toolbar | ✅ | ✅ | ✅ | |
| Follows the desktop theme (KDE/Breeze) | ✅ | ⚠️ | ⚠️ | Chrome has a "GTK/Qt" theme option; Firefox follows GTK. |
| Browser themes | ❌ | ✅ | ✅ | Kite follows the KDE colour scheme instead. |
| Per-site forced dark mode | ✅ | ❌ | ❌ | Chrome and Firefox: only global or through extensions. |
| Per-site custom CSS | ✅ | ❌ | ⚠️ | Firefox: userContent.css, which isn't per site. |
| Per-site custom JavaScript | ✅ | ❌ | ❌ | Chrome and Firefox need a userscript extension. |

## Privacy and security

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Third-party cookies blocked by default | ✅ | ❌ | ✅ | Firefox partitions them (Total Cookie Protection). |
| Per-site cookie rules (allow, block, until close) | ✅ | ✅ | ✅ | |
| Clear data on exit | ✅ | ✅ | ✅ | |
| Global Privacy Control | ✅ | ❌ | ✅ | |
| Do Not Track | ✅ | ✅ | ⚠️ | Firefox removed the setting in favour of GPC. |
| Remove tracking parameters from URLs | ✅ | ❌ | ✅ | Firefox: in strict tracking protection. |
| Block link click pings | ✅ | ❌ | ✅ | Firefox doesn't send them by default. |
| Generic user agent (no engine token) | ✅ | ✅ | ✅ | Kite drops QtWebEngine's rare token. |
| Fingerprinting protection | ✅ | ⚠️ | ✅ | Kite: canvas/WebGL/audio noise and generic hardware, turned on per site. Firefox: known fingerprinters blocked, more in strict mode. Chrome: limited. |
| WebRTC local IP protection | ✅ | ⚠️ | ✅ | Chrome hides local IPs with mDNS; no setting. |
| HTTPS-only mode | ✅ | ✅ | ✅ | |
| DNS over HTTPS | ✅ | ✅ | ✅ | |
| Proxy settings in the browser | ✅ | ❌ | ✅ | Chrome uses the system's. Kite needs a restart. |
| DNS prefetching off by default | ✅ | ❌ | ❌ | |
| Site permissions (camera, location, …) | ✅ | ✅ | ✅ | |
| Screen Wake Lock permission | ✅ | ✅ | ✅ | |
| Certificate error pages | ✅ | ✅ | ✅ | |
| Safe Browsing (malware/phishing lists) | ✅ | ✅ | ✅ | Kite: opt-in; Google (own API key) or open lists checked locally |
| Sandboxed site processes | ✅ | ✅ | ✅ | All three isolate sites in separate processes. |

## Content blocking

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Built-in ad and tracker blocking | ✅ | ❌ | ⚠️ | Kite uses EasyList/EasyPrivacy. Chrome only blocks "abusive" ads; Firefox blocks trackers, not ads. |
| Per-site blocking switch | ✅ | ❌ | ✅ | |
| Custom filter rules | ✅ | ❌ | ❌ | |
| Full uBlock Origin (blocking webRequest) | ✅ | ❌ | ✅ | Chrome only allows uBlock Origin Lite (Manifest V3). |

## Extensions

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Extension store | addons.mozilla.org | Chrome Web Store | addons.mozilla.org | Kite installs Firefox add-ons. |
| Manifest V2 | ✅ | ❌ | ✅ | |
| Manifest V3 | ✅ | ✅ | ✅ | |
| API coverage | ⚠️ | ✅ | ✅ | Kite has 30+ namespaces but no containers, `find`, `dns`, themes or `filterResponseData`, and no native messaging. |
| Extensions in private windows | ❌ | ⚠️ | ⚠️ | Chrome and Firefox: off by default, allowed per extension. |
| Unified extensions panel with pin/unpin | ✅ | ✅ | ✅ | |
| Extension sidebars | ✅ | ✅ | ✅ | |

## Bookmarks, history and downloads

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Bookmark manager | ✅ | ✅ | ✅ | Kite uses Falkon-compatible files. |
| History manager | ✅ | ✅ | ✅ | |
| Bookmarks and history sidebar | ✅ | ⚠️ | ✅ | Chrome's side panel has bookmarks and reading list. |
| Bookmark import and export | ✅ | ✅ | ✅ | Kite imports HTML (as Chrome and Firefox export it) or Falkon's bookmarks.json, and exports HTML. |
| Import straight from an installed browser | ❌ | ✅ | ✅ | Kite needs an exported file. |
| Download manager | ✅ | ✅ | ✅ | |
| Ask where to save | ✅ | ✅ | ✅ | |
| Reading list | ❌ | ✅ | ❌ | |

## Passwords and forms

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Password manager | ✅ | ✅ | ✅ | |
| KDE Wallet storage | ✅ | ✅ | ❌ | Kite shares logins with Falkon. |
| Master password / primary password | ✅ | ❌ | ✅ | |
| Password import/export (CSV) | ✅ | ✅ | ✅ | Kite also reads Bitwarden CSV and Falkon XML. |
| Password generator | ❌ | ✅ | ✅ | |
| Passkeys (WebAuthn) | ❌ | ✅ | ✅ | |
| Address and card autofill | ❌ | ✅ | ✅ | |

## Accounts and services

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Sync between devices | ❌ | ✅ | ✅ | |
| Built-in translation | ❌ | ✅ | ✅ | Firefox translates on the device. |
| Reader view | ❌ | ⚠️ | ✅ | Chrome has a reading mode side panel. |
| Screenshots | ❌ | ⚠️ | ✅ | |
| AI assistant | ❌ | ✅ | ⚠️ | Firefox: a sidebar for a chosen chatbot. |
| Casting to TVs | ❌ | ✅ | ❌ | |
| Install sites as apps (PWA) | ❌ | ✅ | ⚠️ | |

## Developers

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Developer tools | ✅ | ✅ | ✅ | Kite has Chromium's DevTools. |
| View page source | ✅ | ✅ | ✅ | |
| Console logging to a terminal | ✅ | ⚠️ | ⚠️ | Kite routes page and extension consoles to `RUST_LOG`. |

## Desktop integration (Linux / KDE)

| Feature | Kite | Chrome | Firefox | Notes |
|---|---|---|---|---|
| Native KDE look (Breeze, Kirigami) | ✅ | ❌ | ❌ | |
| KDE notifications | ✅ | ✅ | ✅ | |
| Screen kept on through the desktop's screensaver API | ✅ | ✅ | ✅ | |
| KDE file dialogs | ✅ | ⚠️ | ⚠️ | Chrome and Firefox: through the desktop portal. |

## Summary

**Where Kite goes further:**
- Tab bar anywhere: above, between, below, or vertical on either side.
- Split view with a tab strip for each side.
- Tab groups drawn as a file tree in vertical lists.
- Per-site dark mode, CSS, JavaScript and fingerprint protection.
- Built-in container tabs and built-in ad blocking.
- Full uBlock Origin on a Chromium engine, which Chrome itself no longer allows.
- Local start pages kept in the browser with live reload.
- Native KDE integration, including sharing KDE Wallet logins with Falkon.

**Where Kite lags:**
- Its engine trails Chrome, and it runs only on Linux.
- No sync, translation, reader view, picture-in-picture, passkeys or address/card autofill.
- No password generator or browser themes.
- No profile picker; profiles are chosen on the command line.
- Its extension API coverage is narrower than Firefox's.
