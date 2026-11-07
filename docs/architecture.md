# Architecture

Kite is a web browser with a Rust core, a QML interface and QtWebEngine (Chromium) rendering the
pages. It's modelled on Falkon, and several QML files still name the Falkon class they correspond
to. Extensions are Firefox WebExtensions, implemented by Kite itself rather than by Chromium.

## The layers

```
                     ┌──────────────────────────────────────────────┐
                     │ crates/kite/qml, crates/kite/plugins         │
                     │ The interface: windows, tabs, menus, settings│
                     └───────┬───────────────────────────┬──────────┘
          QML singletons     │                           │  WebEngineView,
          (cxx-qt)           ▼                           ▼  WebEngineProfile
┌────────────────────────────────────┐      ┌──────────────────────────────┐
│ crates/kite/src                    │      │ QtWebEngine / Chromium       │
│ Browser, Extensions, Passwords,    │      │ The pages themselves         │
│ FormFill, the App state            │      └──────────────▲───────────────┘
└───────┬──────────────────▲─────────┘                     │ schemes, request
        │ plain Rust calls │ C ABI (ffi.rs)                │ interceptor, cookies
        ▼                  ▼                               │
┌───────────────────┐  ┌───────────────────────────────────┴──┐
│ crates/kite-core  │  │ crates/kite/cpp                      │
│ Data and logic,   │  │ What only C++ can do: start-up,      │
│ no Qt, unit tested│  │ QtWebEngine hooks, printing          │
└───────────────────┘  └──────────────────────────────────────┘
```

**Where new code goes:** logic and data go in `kite-core`, where they can be tested without Qt. The
`src/` layer exposes them to QML. C++ is only for what Qt offers no other way to reach. QML holds the
interface and the state that belongs to it, such as which tabs a window has.

## Repository layout

| Path | Contents |
|---|---|
| `crates/kite-core/src/` | The core library, see [below](#kite-core-data-and-logic) |
| `crates/kite-core/src/webext/` | The WebExtension platform |
| `crates/kite-core/src/webext/js/` | The `browser.*` API and the scripts injected into pages |
| `crates/kite-core/examples/` | Developer tools: `amo-install`, `dnr-check`, `kwallet-check` |
| `crates/kite/src/` | The QML singletons and the C ABI |
| `crates/kite/src/js/` | Page scripts: reader extraction, fingerprint protection, wake lock, scrollbars, Google sign-in identity |
| `crates/kite/cpp/` | The C++ layer |
| `crates/kite/qml/` | The interface, see [ui.md](ui.md) |
| `crates/kite/plugins/` | Plugins that ship with Kite, see [plugins.md](plugins.md) |
| `crates/kite/icons/` | Kite's own icons, used as a fallback icon theme |
| `crates/kite/build.rs` | Compiles the QML module and resources, including every plugin file |
| `data/` | Desktop file and application icon |
| `packaging/appimage/` | `build-appimage.sh` |

## Start-up

1. **`main()` in `src/main.rs`** reads the arguments (`--profile`, `--private`, URLs) and, when
   several profiles exist and the user asked to choose, runs the picker as a separate process
   (`kite --pick-profile`, `cpp/profiles.cpp`). The profile has to be known before QtWebEngine starts.
2. **`state::init`** opens the profile and builds the global `App` (`src/state.rs`), reached from
   anywhere through `app()`.
3. **Chromium flags** are set from settings, because they can't change later. That's why proxy and
   overlay scrollbar settings need a restart.
4. **`kite_bootstrap` in `cpp/bootstrap.cpp`** registers the URL schemes, initialises QtWebEngine,
   creates the `QApplication` and loads the `Main` QML component.
5. **Profile hooks:** when `Main` is created, the C++ layer installs the scheme handlers, request
   interceptor and cookie hooks on its profiles (`kite_install_profile_hooks` in `cpp/glue.cpp`), then
   calls `Main.start()`.
6. **`Main.start()` in `qml/Main.qml`** installs the page scripts, then restores the session or opens
   the first window.

## How the layers talk

### QML to Rust: the singletons

`cxx-qt` turns Rust structs into QML singletons, imported with `import org.kde.kite`:

| Singleton | File | Covers |
|---|---|---|
| `Browser` | `src/browser.rs` | Settings, history, bookmarks, sessions, search, site settings, downloads, plugins, printing |
| `Extensions` | `src/extensions.rs` | Installing extensions, the API calls from their pages, effects back to QML |
| `Passwords` | `src/passwords.rs` | The password store and its backends |
| `FormFill` | `src/formfill.rs` | Saved addresses and form autofill |

Rust gets called with strings and JSON, and answers the same way or through a signal. Slow work runs
on a thread and comes back through `qt_thread().queue(...)`, because QObjects may only be touched on
the Qt thread.

### C++ and Rust: the C ABI

`src/ffi.rs` holds both directions: `kite_cpp_*` functions are implemented in C++ and called from
Rust, and `kite_*` functions are implemented in Rust and called from C++. Buffers crossing from Rust
are a `KiteBuf`, released with `kite_buf_free`.

| C++ file | Does |
|---|---|
| `cpp/bootstrap.cpp` | Start-up, the application-wide input filter `KiteInputFilter` (Alt menu bar, mouse gestures), favicon tracking |
| `cpp/glue.cpp` | `KiteSchemeHandler` (`kite:`, `moz-extension:`), `KiteInterceptor` (every request), cookie store hooks, DNS over HTTPS, the certificate lookup for `kite:certificate` |
| `cpp/print.cpp` | Qt's print dialog, then CUPS through `lp` |
| `cpp/profiles.cpp` | The profile picker |

### Web pages and the browser

Scripts run in Chromium's isolated worlds, which the page can't reach:

| World | Used by |
|---|---|
| `0`, the page's own | Only what pages must see, such as `navigator.wakeLock`, and what's only there, such as `navigator.mediaSession`. Never anything secret |
| `1`, Kite's | Password and form scripts, plugin page scripts, the extension bridge |
| Others | One each per extension, for its content scripts |

A page script tells the browser something by printing a console message that starts with
`Main.frameSecret`. The page can neither see nor forge that secret. `WebTab.onJavaScriptConsoleMessage`
recognises it and routes the rest. Extension pages and the bridge world use a `QWebChannel` object
instead (`qml/ExtBridge.qml`).

## Core components

### kite-core: data and logic

| Module | Responsibility |
|---|---|
| `paths`, `profiles` | Profile folders, the profile list, locks |
| `shared` | `Shared`: the data both the interface and the extension runtime use |
| `settings` | Preferences as a flat JSON object. `defaults()` lists every setting |
| `db` | Opens `browsedata.db` (Falkon's schema) |
| `history`, `bookmarks`, `favicons`, `downloads` | What their names say. Bookmarks use Falkon's format |
| `session` | The last session, closed tabs and windows, named sessions |
| `search` | Search engines and address bar input (address or search?) |
| `sitesettings`, `privacy` | Per-site settings; tracking parameters and HTTPS upgrades |
| `adblock`, `safebrowsing` | The built-in content blocker; dangerous-site protection |
| `passwords`, `kwallet`, `formfill` | Saved logins and their backends; saved addresses |
| `certs`, `certview` | Accepted certificate exceptions; the `kite:certificate` page |
| `reader`, `startpage`, `standin` | Reader view pages; custom start pages; generated stand-in files |
| `backup`, `browser_import` | The settings file; importing bookmarks from other browsers |
| `plugins` | Parses and checks plugin manifests; installs, lists and removes plugin files |
| `kwin`, `screensaver` | KWin scripting (picture-in-picture staying on top); screen inhibition |

### kite-core: the WebExtension platform

| Module | Responsibility |
|---|---|
| `webext/amo`, `xpi`, `registry` | Finding, downloading, unpacking and tracking extensions |
| `webext/manifest`, `i18n`, `matchpattern` | `manifest.json`, `_locales`, URL patterns |
| `webext/scripts`, `js/` | The `browser.*` API injected into extension pages and content script worlds |
| `webext/runtime` | The API backend: messaging, tabs and windows mirror, storage, alarms, menus and more |
| `webext/storage`, `dnr`, `blocking`, `compat` | `browser.storage`; `declarativeNetRequest`; blocking `webRequest`; shims for Firefox internals |
| `webext/native` | Native messaging: finding hosts, framing, running them |

`Runtime` knows nothing about Qt. A call goes in and a list of `Effect`s comes out, so it can be
tested without a browser. See [An extension API call](#an-extension-api-call).

### crates/kite/src

| File | Responsibility |
|---|---|
| `main.rs` | The entry point, arguments, Chromium flags |
| `state.rs` | `App`, the process-wide state, and background updaters |
| `browser.rs`, `extensions.rs`, `passwords.rs`, `formfill.rs` | The QML singletons |
| `ffi.rs` | The C ABI, serving `kite:` pages (`serve`) and intercepting requests (`kite_intercept`) |
| `rpc.rs` | The API transport for extension pages in sub-frames, where the `QWebChannel` isn't available |
| `native_hosts.rs` | The running native messaging hosts |
| `startpage.rs` | The `kite:start` page |

### The main QML objects

| Object | File | Owns |
|---|---|---|
| `Main` (one) | `qml/Main.qml` | Web profiles, the windows, the action list, session saving, plugin helpers, page scripts, answering extensions' UI requests |
| `BrowserWindow` (one per window) | `qml/BrowserWindow.qml` | `tabModel`, tab groups and trees, split view, `runAction`, the window's menus |
| `WebTab` (one per tab) | `qml/WebTab.qml` | One `WebEngineView` and everything per-page: loading, permissions, certificate errors, unloading |
| `ExtBridge` (one per view) | `qml/ExtBridge.qml` | The `QWebChannel` object for extension traffic |

**A `WebTab` must not be the delegate of the `Repeater` over `tabModel`.** The delegate is a small
item that creates the tab with `webTabComponent` and retires it. A removed delegate loses its QML
context at once but is deleted a moment later, and a script result arriving in that gap crashes
inside Qt. Keep this structure.

## How things flow

### Changing a setting

1. Settings and the palette call `Browser.setSetting(key, json)`.
2. `set_setting` in `browser.rs` stores it, applies anything Rust acts on itself, and emits
   `settingChanged`.
3. The `Connections` on `Browser` in `Main.qml` reacts. Keys listed in `liveKeys` update
   `Main.liveSettings`, which the interface binds to. Keys in `profileKeys` are reapplied to every
   profile, and some keys rebuild the page scripts.

### A request

Every request from a page goes through `KiteInterceptor` to `kite_intercept` in `ffi.rs`, **on the UI
thread, so anything slow there stutters the interface and video**. In order, it handles:
dangerous-site protection for page loads, tracking parameters and HTTPS upgrades, `webRequest`
listeners and `declarativeNetRequest`, then the content blocker.

### A kite: page

`KiteSchemeHandler::requestStarted` in `glue.cpp` receives it. Almost all go to `serve` in `ffi.rs`,
which matches on the page name:

`kite:start` (with `speeddial`, `newtab` and `home`), `kite:search`, `kite:favicon`, `kite:startpage/…`,
`kite:reader`, `kite:dangerous`, `kite:https-only` and `kite:config`.

`kite:certificate` is the exception. It answers later from C++, after its own TLS connection. Any web
page can link to a `kite:` address, so pages that act on their parameters check the request's
initiator first.

`kite:config` (`kite_core::config`, with `config_page.js`) lists every setting. It's refused to web
pages, as it can change settings. Its changes come back as console messages with a secret token only
that page has: `WebTab.onJavaScriptConsoleMessage` passes them to `Browser.configCommand`, which goes
through the same steps as `setSetting`.

### An extension API call

```
extension code: browser.tabs.create(...)
  ▼  js/api_core.js                the promise-based API
  ▼  the bridge world (js/bridge.js), the WebChannel, or rpc.rs in sub-frames
Extensions.call(...)               src/extensions.rs
  ▼
Runtime::call(...)                 webext/runtime.rs: checks the token and permissions
  ▼  returns Vec<Effect>
Extensions::apply(...)
  ├─ Reply / Deliver   → back to the extension's pages
  ├─ Ui                → Main.onUiRequest, which does it and answers through ui_reply
  ├─ Native            → native_hosts.rs
  └─ Changed           → QML refreshes its toolbar buttons, menus and so on
```

Effects from other threads go through `apply_effects_async`.

### Native messaging

`runtime.connectNative` produces `Effect::Native(NativeOp::Connect)`. `native_hosts.rs` finds and
starts the host with `webext/native.rs`, whose threads hand messages to
`Runtime::native_message` and `native_closed`. Hosts get none of Kite's open files, and none of the
AppImage's environment.

### Saving a session

`Main.saveSession()` collects each window's tabs with `WebTab.saveState()` and passes them to
`Browser`, which writes them through `kite_core::session`. On start-up, `Main.restoreWindow` recreates
tabs *lazily*: a restored tab has only a `pendingUrl` until it's on view (`WebTab.wake()`).

### Profiles, private windows and containers

`Main` holds `profile`, `privateProfile` and one profile per container (`containerProfile`). Each gets
the same hooks and scripts (`allProfiles`, `installUserScripts`). Extensions only run in private windows
if allowed. `kite --private` uses a throwaway profile folder, deleted at exit.

## Getting out of trouble

| Command | Does |
|---|---|
| `kite --no-plugins` | Runs no plugin in this session. Nothing stored changes |
| `kite --factory` | Saves the settings to `settings-before-factory.json` in the profile, plus an untouched copy in `settings-before-factory.original`, then starts with the defaults. Run before anything reads the settings, since they may be what's broken. Bookmarks, logins, extensions and sessions stay |

The saved `.json` can be imported in Settings → General. It leaves out what an import always does, such
as window positions and the download folder, and drops values of the wrong type. `.original` has
everything, byte for byte.

## Things that will bite you

| Trap | Why |
|---|---|
| Never put a `WebTab` directly in a `Repeater` | Script results then crash Qt, see [above](#the-main-qml-objects) |
| A page woken from being unloaded has no favicon helper | Reading its icon crashes, so `WebTab.faviconOut` keeps it off the favicon provider |
| `frame.runJavaScript` needs a callback | Qt doesn't run it otherwise; pass `() => {}` |
| `Array.prototype.flatMap` doesn't exist in Qt's JavaScript | Use `reduce` and `concat` |
| A `.js` import named `Keys` hides the attached `Keys` | Pick another name for the import |
| QML type names are global to the module | Files of plugins shipped with Kite included, so a duplicate name clashes |
| A plugin's text renders as markup in Qt labels and dialog titles | `kite_core::plugins` refuses `<` in everything shown, because installed plugins come from anyone |
| A property can't be called `left`, `right`, `top` and so on | They're `FINAL` on `Item`, and the file fails to load |
| `kite_intercept` runs on the UI thread | Keep it fast |
| Blocking third-party cookies also denies those frames all storage | Chromium has one check for both, and Qt's cookie filter can't tell them apart. `src/js/third_party_storage.js` gives such frames `localStorage` and `sessionStorage` in memory only, so embedded players don't crash. IndexedDB and the Cache API stay refused |
| A redirect from the request interceptor never finishes on a page a service worker answers for | The tab stays blank. So page loads are decided first, in `WebTab.onNavigationRequested` through `ffi::page_load_redirect`, which rejects them and loads the new address. The interceptor keeps redirecting only what can't be loaded again, like form posts |
| The AppImage only has the KDE plugins it was given | `build-appimage.sh` lists them, as it doesn't use the system's. Without `kio_file`, KDE's file dialog stays empty, and the scanner can't see what's only loaded at runtime |
| Only a shown tab may hold a certificate warning open | Chromium holds the site's other requests meanwhile (`WebTab.shownInPane`) |

## On disk

Everything is under `~/.local/share/kite/`, or `$KITE_DATA_DIR` if set:

```
profiles/<name>/
    settings.json     preferences
    browsedata.db     history, icons, search engines, site settings
    bookmarks.json    bookmark tree
    session.json      last session, closed tabs and windows
    downloads.json    download history
    adblock/          filter subscriptions
    extensions/       registry.json, storage.db and the unpacked extensions
    webengine/        Chromium's storage: cookies, cache, local storage
native-messaging-hosts/   Kite's own native messaging manifests
plugins/<id>/             plugins installed from a file
```

## Building and testing

```sh
cargo build
cargo test --workspace
node --test crates/kite-core/src/webext/js/*.test.js crates/kite/qml/*.test.js crates/kite/plugins/*/*.test.js
```

Keep logic testable: the plain `.js` files next to the QML (`fuzzy.js`, `palette.js`, `tabtree.js`)
and Rust in `kite-core` are tested directly. Some tests also compare files with each other. For
example, `palette.test.js` fails if a checkbox in Settings is missing from the command palette, and
a test in `browser.rs` checks that every plugin is registered.

## Common changes

**A new setting**
1. Add its default in `defaults()` in `kite-core/src/settings.rs`.
2. Add its control to the right page in `qml/PreferencesWindow.qml`.
3. If it's a plain checkbox, add it to `TOGGLES` in `qml/palette.js` (a test enforces this). Other
   settings pages can go in `CHOICES`.
4. To react without a restart, add it to `liveKeys` in `Main.qml` and bind to
   `app.liveSettings["your.key"]`.

**A new action with a shortcut**
1. Add it to `coreActions` in `Main.qml`. Users can then rebind it, and it's in the palette.
2. Add a `case` to `BrowserWindow.runAction`.
3. Give it an icon in `ACTION_ICONS` in `palette.js` (a test enforces this).
4. Optionally, add it to the menus listed in [ui.md](ui.md). Menus don't pick up actions by themselves.

**A new `kite:` page**
Add a branch to `serve` in `ffi.rs`, with the page in `kite-core` where possible. Escape everything
that comes from outside, and check the initiator if loading it does anything.

**A new extension API**
Add the call to the `match` in `Runtime` in `webext/runtime.rs`, and to `js/api_core.js`. If it
needs the interface, return a `Ui` effect through `self.ui(...)` and add a `case` to the handler in
`Main.qml`.

**A new toolbar button**
Add it to `catalog` in `qml/ToolbarItems.qml`, and a `Component` for it in
`qml/NavigationToolbar.qml`.
