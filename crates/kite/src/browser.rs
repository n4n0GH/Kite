//! `Browser` — QML singleton exposing browsing data and preferences
//! (history, bookmarks, sessions, downloads, search, site settings,
//! content blocking). Complex values cross the boundary as JSON strings.

use crate::state::app;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use kite_core::bookmarks::{Kind, Node};
use kite_core::search::{self, LoadAction};
use serde_json::{json, Value};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Browser = super::BrowserRust;

        // settings
        #[qinvokable]
        fn setting(self: &Browser, key: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "setSetting"]
        fn set_setting(self: Pin<&mut Browser>, key: &QString, json: &QString);
        #[qinvokable]
        #[cxx_name = "resetSetting"]
        fn reset_setting(self: Pin<&mut Browser>, key: &QString);
        /// A setting changed (set or reset); `json` is its new value.
        #[qsignal]
        #[cxx_name = "settingChanged"]
        fn setting_changed(self: Pin<&mut Browser>, key: QString, json: QString);
        #[qinvokable]
        #[cxx_name = "webEnginePath"]
        fn web_engine_path(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "profileName"]
        fn profile_name(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "initialUrls"]
        fn initial_urls(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "isPrivate"]
        fn is_private(self: &Browser) -> bool;

        // location bar
        #[qinvokable]
        #[cxx_name = "resolveInput"]
        fn resolve_input_q(self: &Browser, text: &QString) -> QString;
        #[qinvokable]
        fn complete(self: &Browser, text: &QString, in_private: bool) -> QString;
        #[qinvokable]
        #[cxx_name = "requestSuggestions"]
        fn request_suggestions(self: Pin<&mut Browser>, text: &QString);
        #[qsignal]
        #[cxx_name = "suggestionsReady"]
        fn suggestions_ready(self: Pin<&mut Browser>, text: QString, json: QString);

        // history
        #[qinvokable]
        #[cxx_name = "recordVisit"]
        fn record_visit(self: Pin<&mut Browser>, url: &QString, title: &QString, in_private: bool);
        #[qinvokable]
        #[cxx_name = "updateTitle"]
        fn update_title(self: Pin<&mut Browser>, url: &QString, title: &QString, in_private: bool);
        #[qinvokable]
        fn history(self: &Browser, query: &QString, start: f64, end: f64, limit: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "deleteHistory"]
        fn delete_history(self: Pin<&mut Browser>, ids_json: &QString);
        #[qinvokable]
        #[cxx_name = "deleteHistoryRange"]
        fn delete_history_range(self: Pin<&mut Browser>, start: f64, end: f64);
        #[qinvokable]
        #[cxx_name = "clearHistory"]
        fn clear_history(self: Pin<&mut Browser>);
        #[qsignal]
        #[cxx_name = "historyChanged"]
        fn history_changed(self: Pin<&mut Browser>);

        // bookmarks
        #[qinvokable]
        #[cxx_name = "bookmarkTree"]
        fn bookmark_tree(self: &Browser, folder: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "bookmarkRoots"]
        fn bookmark_roots(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "bookmarkFolders"]
        fn bookmark_folders(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "searchBookmarks"]
        fn search_bookmarks(self: &Browser, text: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "bookmarkForUrl"]
        fn bookmark_for_url(self: &Browser, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "addBookmark"]
        fn add_bookmark(self: Pin<&mut Browser>, url: &QString, title: &QString, folder: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "addFolder"]
        fn add_folder(self: Pin<&mut Browser>, parent: &QString, name: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "addSeparator"]
        fn add_separator(self: Pin<&mut Browser>, parent: &QString);
        #[qinvokable]
        #[cxx_name = "updateBookmark"]
        fn update_bookmark(self: Pin<&mut Browser>, id: &QString, json: &QString);
        #[qinvokable]
        #[cxx_name = "moveBookmark"]
        fn move_bookmark(self: Pin<&mut Browser>, id: &QString, parent: &QString, index: i32);
        #[qinvokable]
        #[cxx_name = "removeBookmark"]
        fn remove_bookmark(self: Pin<&mut Browser>, id: &QString);
        #[qinvokable]
        #[cxx_name = "importBookmarks"]
        fn import_bookmarks(self: Pin<&mut Browser>, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "exportBookmarks"]
        fn export_bookmarks(self: &Browser, path: &QString) -> QString;
        #[qsignal]
        #[cxx_name = "bookmarksChanged"]
        fn bookmarks_changed(self: Pin<&mut Browser>);

        // session
        #[qinvokable]
        #[cxx_name = "saveSession"]
        fn save_session(self: Pin<&mut Browser>, json: &QString);
        #[qinvokable]
        #[cxx_name = "lastSession"]
        fn last_session(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "setRunning"]
        fn set_running(self: Pin<&mut Browser>, running: bool);
        #[qinvokable]
        #[cxx_name = "crashedLastTime"]
        fn crashed_last_time(self: &Browser) -> bool;
        #[qinvokable]
        #[cxx_name = "pushClosedTab"]
        fn push_closed_tab(self: Pin<&mut Browser>, json: &QString);
        #[qinvokable]
        #[cxx_name = "popClosedTab"]
        fn pop_closed_tab(self: Pin<&mut Browser>, index: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "closedTabs"]
        fn closed_tabs(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "pushClosedWindow"]
        fn push_closed_window(self: Pin<&mut Browser>, json: &QString);
        #[qinvokable]
        #[cxx_name = "popClosedWindow"]
        fn pop_closed_window(self: Pin<&mut Browser>, index: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "closedWindows"]
        fn closed_windows(self: &Browser) -> QString;
        #[qsignal]
        #[cxx_name = "closedChanged"]
        fn closed_changed(self: Pin<&mut Browser>);

        // downloads
        #[qinvokable]
        #[cxx_name = "downloadUpdate"]
        fn download_update(self: Pin<&mut Browser>, json: &QString) -> i32;
        /// The last private window closed: its finished downloads leave the list.
        #[qinvokable]
        #[cxx_name = "forgetPrivateDownloads"]
        fn forget_private_downloads(self: Pin<&mut Browser>);
        #[qinvokable]
        fn downloads(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "removeDownload"]
        fn remove_download(self: Pin<&mut Browser>, id: i32);
        #[qinvokable]
        #[cxx_name = "clearDownloads"]
        fn clear_downloads(self: Pin<&mut Browser>);
        #[qinvokable]
        #[cxx_name = "uniqueDownloadPath"]
        fn unique_download_path(self: &Browser, dir: &QString, name: &QString) -> QString;
        #[qsignal]
        #[cxx_name = "downloadsChanged"]
        fn downloads_changed(self: Pin<&mut Browser>);

        // search engines
        #[qinvokable]
        fn engines(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "saveEngines"]
        fn save_engines(self: Pin<&mut Browser>, json: &QString);
        #[qinvokable]
        #[cxx_name = "searchUrl"]
        fn search_url(self: &Browser, engine: &QString, query: &QString) -> QString;

        // site settings
        #[qinvokable]
        #[cxx_name = "siteSettings"]
        fn site_settings(self: &Browser, host: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "setSiteSetting"]
        fn set_site_setting(self: Pin<&mut Browser>, host: &QString, key: &QString, value: i32);
        #[qinvokable]
        #[cxx_name = "allSiteSettings"]
        fn all_site_settings(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "clearSiteSettings"]
        fn clear_site_settings(self: Pin<&mut Browser>, host: &QString);
        /// Whether a failed page load of `url` was an HTTPS-only upgrade.
        #[qinvokable]
        #[cxx_name = "httpsUpgradeFailed"]
        fn https_upgrade_failed(self: &Browser, url: &QString) -> bool;
        /// Fingerprint protection for a site (js/fingerprint.js), with its
        /// noise seed.
        #[qinvokable]
        #[cxx_name = "fingerprintScript"]
        fn fingerprint_script(self: &Browser, seed: u32) -> QString;
        /// Install the network hooks on new container profiles (Main.qml).
        #[qinvokable]
        #[cxx_name = "hookContainerProfiles"]
        fn hook_container_profiles(self: &Browser);
        /// Delete the cookies of sites set to "until Kite closes" (at quit).
        #[qinvokable]
        #[cxx_name = "deleteSessionCookies"]
        fn delete_session_cookies(self: &Browser);
        /// Custom start page ("home" or "newtab"): copy `path` into the
        /// profile (kite:startpage/<target>/).
        #[qinvokable]
        #[cxx_name = "startPageImport"]
        fn start_page_import(self: &Browser, target: &QString, path: &QString) -> bool;
        /// Custom start page: a starter page as the target's copy (unless it
        /// has one); its path.
        #[qinvokable]
        #[cxx_name = "startPageCreate"]
        fn start_page_create(self: &Browser, target: &QString) -> QString;
        /// The path of a target's copy in the profile.
        #[qinvokable]
        #[cxx_name = "startPageCopyPath"]
        fn start_page_copy_path(self: &Browser, target: &QString) -> QString;
        /// A file's modification time in ms (0 if it doesn't exist).
        #[qinvokable]
        #[cxx_name = "fileModified"]
        fn file_modified(self: &Browser, path: &QString) -> f64;
        /// Dangerous-site protection: the threat `url` is known for
        /// ("phishing", "malware", "unwanted"), or "".
        #[qinvokable]
        #[cxx_name = "safeBrowsingCheck"]
        fn safe_browsing_check(self: &Browser, url: &QString) -> QString;
        /// Visit `host` anyway this session.
        #[qinvokable]
        #[cxx_name = "safeBrowsingAllow"]
        fn safe_browsing_allow(self: &Browser, host: &QString);
        /// The lists' state, for Settings.
        #[qinvokable]
        #[cxx_name = "safeBrowsingStatus"]
        fn safe_browsing_status(self: &Browser) -> QString;
        /// Visit `host` over plain http this session (HTTPS-only exception).
        #[qinvokable]
        #[cxx_name = "allowHttp"]
        fn allow_http(self: &Browser, host: &QString);
        /// Text settings: "css" or "js".
        #[qinvokable]
        #[cxx_name = "setSiteText"]
        fn set_site_text(self: Pin<&mut Browser>, host: &QString, key: &QString, value: &QString);
        /// Back to the defaults, keeping custom CSS and JavaScript.
        #[qinvokable]
        #[cxx_name = "resetSiteSettings"]
        fn reset_site_settings(self: Pin<&mut Browser>, host: &QString);

        // content blocking
        #[qinvokable]
        #[cxx_name = "reloadAdblock"]
        fn reload_adblock(self: Pin<&mut Browser>, download: bool);
        #[qinvokable]
        #[cxx_name = "adblockBlocked"]
        fn adblock_blocked(self: &Browser) -> i32;
        #[qinvokable]
        #[cxx_name = "adblockAllow"]
        fn adblock_allow(self: Pin<&mut Browser>, host: &QString, allow: bool);
        #[qinvokable]
        #[cxx_name = "adblockAllowed"]
        fn adblock_allowed(self: &Browser, host: &QString) -> bool;
        #[qinvokable]
        fn cosmetic(self: &Browser, url: &QString, in_private: bool) -> QString;
        /// Keep the screen on for a tab holding a Screen Wake Lock (or stop).
        #[qinvokable]
        #[cxx_name = "setWakeLock"]
        fn set_wake_lock(self: &Browser, tab_id: i32, on: bool, site: &QString) -> bool;
        /// Page-world script providing navigator.wakeLock (js/wakelock.js).
        #[qinvokable]
        #[cxx_name = "wakeLockScript"]
        fn wake_lock_script(self: &Browser) -> QString;
        /// Script giving pages' custom scrollbars the overlay style
        /// (js/scrollbars.js), or "" without overlay scrollbars.
        #[qinvokable]
        #[cxx_name = "scrollbarScript"]
        fn scrollbar_script(self: &Browser) -> QString;
        /// Printing: Qt's dialog, answered by printChosen (JSON with where to
        /// put the PDF and for what paper, "" if cancelled).
        #[qinvokable]
        #[cxx_name = "printDialog"]
        fn print_dialog(self: Pin<&mut Browser>, title: &QString);
        #[qsignal]
        #[cxx_name = "printChosen"]
        fn print_chosen(self: Pin<&mut Browser>, json: QString);
        /// The PDF is there: on to the chosen printer or file; printDone says how it went.
        #[qinvokable]
        #[cxx_name = "printSubmit"]
        fn print_submit(self: &Browser);
        #[qinvokable]
        #[cxx_name = "printCancel"]
        fn print_cancel(self: &Browser);
        #[qsignal]
        #[cxx_name = "printDone"]
        fn print_done(self: Pin<&mut Browser>, ok: bool, message: QString);

        /// Reader view: the script that extracts the page's article, for an
        /// isolated world; it returns JSON, or "" without an article.
        #[qinvokable]
        #[cxx_name = "readerScript"]
        fn reader_script(self: &Browser) -> QString;
        /// Reader view: the script telling whether the page looks like an article.
        #[qinvokable]
        #[cxx_name = "readerProbeScript"]
        fn reader_probe_script(self: &Browser) -> QString;
        /// Keep an extracted article; the kite:reader address showing it, or "".
        #[qinvokable]
        #[cxx_name = "readerOpen"]
        fn reader_open(self: &Browser, source: &QString, json: &QString, background: &QString, text: &QString, link: &QString) -> QString;
        /// The page a kite:reader address was extracted from, or "".
        #[qinvokable]
        #[cxx_name = "readerSource"]
        fn reader_source(self: &Browser, url: &QString) -> QString;
        /// The script bringing an open reader page in line with the settings.
        #[qinvokable]
        #[cxx_name = "readerApplyScript"]
        fn reader_apply_script(self: &Browser) -> QString;
        /// The cached favicon of `page_url`'s site as a data: URL, or "".
        #[qinvokable]
        #[cxx_name = "cachedFavicon"]
        fn cached_favicon(self: &Browser, page_url: &QString) -> QString;
        /// Cache the favicon `icon_url` (image://favicon/...) shown for `page_url`.
        #[qinvokable]
        #[cxx_name = "saveFavicon"]
        fn save_favicon(self: &Browser, page_url: &QString, icon_url: &QString);
        /// An unloaded tab's view leaves Qt's favicon provider (`track`
        /// false) until it is loaded again (see bootstrap.cpp).
        #[qinvokable]
        #[cxx_name = "faviconTrack"]
        fn favicon_track(self: &Browser, view_name: &QString, track: bool);
        /// Settings → Shortcuts: a pressed key as a portable key sequence,
        /// and a stored sequence as shown to the user.
        #[qinvokable]
        #[cxx_name = "keySequenceText"]
        fn key_sequence_text(self: &Browser, key: i32, modifiers: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "keySequenceDisplay"]
        fn key_sequence_display(self: &Browser, sequence: &QString) -> QString;
        /// The system's scroll distance per mouse wheel notch, in px.
        #[qinvokable]
        #[cxx_name = "systemWheelPixels"]
        fn system_wheel_pixels(self: &Browser) -> i32;
        /// Name of the extension doing content blocking instead of Kite, or "".
        #[qinvokable]
        #[cxx_name = "contentBlocker"]
        fn content_blocker(self: &Browser, in_private: bool) -> QString;
        #[qsignal]
        #[cxx_name = "adblockStatus"]
        fn adblock_status(self: Pin<&mut Browser>, message: QString);

        // misc helpers
        #[qinvokable]
        #[cxx_name = "tempFile"]
        fn temp_file(self: &Browser, name: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "fileToDataUrl"]
        fn file_to_data_url(self: &Browser, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "hostOf"]
        fn host_of(self: &Browser, url: &QString) -> QString;

        // privacy
        #[qinvokable]
        #[cxx_name = "clearPrivateData"]
        fn clear_private_data(self: Pin<&mut Browser>, json: &QString);
    }

    impl cxx_qt::Threading for Browser {}
}

#[derive(Default)]
pub struct BrowserRust;

fn q(s: impl AsRef<str>) -> QString {
    QString::from(s.as_ref())
}
fn js(v: &Value) -> QString {
    q(v.to_string())
}
fn parse(s: &QString) -> Value {
    serde_json::from_str(&s.to_string()).unwrap_or(Value::Null)
}

/// Turn location-bar text into a URL to load (used by the start page too).
pub fn resolve_input(text: &str) -> String {
    let a = app();
    let engines = search::load_engines(&a.shared.db()).unwrap_or_default();
    let (def, enabled) = {
        let s = a.shared.settings();
        (s.get_str("general.searchEngine"), s.get_bool("general.searchFromLocationBar"))
    };
    let bm = a.shared.bookmarks.clone();
    let kw = move |k: &str| bm.lock().ok().and_then(|b| b.find_by_keyword(k).map(|n| n.url.clone()));
    match search::load_action(text, &engines, &def, enabled, &kw) {
        LoadAction::Url(u) => u,
        LoadAction::Search { url, .. } => url,
        LoadAction::Invalid => String::new(),
    }
}

fn node_brief(n: &Node) -> Value {
    json!({"id": n.id.to_string(), "type": match n.kind { Kind::Folder => "folder", Kind::Url => "url", Kind::Separator => "separator" },
           "title": n.name, "url": n.url, "keyword": n.keyword, "description": n.description,
           "children": n.children.len()})
}

fn notify_bookmarks(fx: Value) {
    let effects = app().runtime().browser_event("bookmarkEvent", &fx);
    crate::extensions::apply_effects_async(effects);
}

/// Settings the Rust side acts on itself (QML follows settingChanged).
fn apply_setting(key: &str, v: &serde_json::Value) {
    let a = app();
    match key {
        "adblock.enabled" => a.adblock.set_enabled(v.as_bool().unwrap_or(true)),
        "adblock.allowlist" => a.adblock.set_allowlist(serde_json::from_value(v.clone()).unwrap_or_default()),
        "web.doNotTrack" => a.do_not_track.store(v.as_bool().unwrap_or(false), std::sync::atomic::Ordering::Relaxed),
        "privacy.sendReferer" => a.send_referer.store(v.as_bool().unwrap_or(true), std::sync::atomic::Ordering::Relaxed),
        "privacy.cookies" => a.cookie_policy.store(crate::state::cookie_policy(v.as_str().unwrap_or("all")), std::sync::atomic::Ordering::Relaxed),
        "privacy.dnsOverHttps" | "privacy.dnsServer" => crate::ffi::apply_dns(),
        "browsing.safeBrowsing" | "browsing.safeBrowsingKey" => crate::state::safe_browsing_changed(),
        "web.wheelScrollPixels" => crate::ffi::set_wheel_pixels(v.as_i64().unwrap_or(0)),
        k if k.starts_with("privacy.") => a.privacy.apply(k, v),
        _ => {}
    }
}

impl qobject::Browser {
    // ---- settings ---------------------------------------------------------
    fn setting(&self, key: &QString) -> QString {
        js(&app().shared.settings().get(&key.to_string()))
    }
    fn set_setting(self: Pin<&mut Self>, key: &QString, json: &QString) {
        let key = key.to_string();
        let v = parse(json);
        let a = app();
        let _ = a.shared.settings().set(&key, v.clone());
        apply_setting(&key, &v);
        self.setting_changed(q(&key), js(&v));
    }
    fn reset_setting(self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        let v = {
            let mut s = app().shared.settings();
            let _ = s.reset(&key);
            s.get(&key)
        };
        apply_setting(&key, &v);
        self.setting_changed(q(&key), js(&v));
    }
    fn web_engine_path(&self) -> QString {
        q(app().shared.profile.webengine_dir().to_string_lossy())
    }
    fn profile_name(&self) -> QString {
        q(&app().shared.profile.name)
    }
    fn initial_urls(&self) -> QString {
        let urls: Vec<String> = app().initial_urls.iter().map(|u| resolve_input(u)).collect();
        js(&json!(urls))
    }
    fn is_private(&self) -> bool {
        app().private
    }

    // ---- location bar -----------------------------------------------------
    fn resolve_input_q(&self, text: &QString) -> QString {
        q(resolve_input(&text.to_string()))
    }

    /// History + bookmark completions for the location bar popup.
    /// (`in_private`: for a private window, where the history only shows if
    /// "privacy.privateHistorySuggestions" says so; bookmarks always do.)
    fn complete(&self, text: &QString, in_private: bool) -> QString {
        let text = text.to_string();
        let a = app();
        let mut out = vec![];
        let mut seen = std::collections::HashSet::new();
        for b in a.shared.bookmarks().search(&text).into_iter().take(4) {
            if seen.insert(b.url.clone()) {
                out.push(json!({"type": "bookmark", "url": b.url, "title": b.name}));
            }
        }
        let from_history = !in_private || a.shared.settings().get_bool("privacy.privateHistorySuggestions");
        let history = if from_history { kite_core::history::complete(&a.shared.db(), &text, 10).unwrap_or_default() } else { vec![] };
        for h in history {
            if seen.insert(h.url.clone()) {
                out.push(json!({"type": "history", "url": h.url, "title": h.title}));
            }
        }
        out.truncate(10);
        js(&Value::Array(out))
    }

    fn request_suggestions(self: Pin<&mut Self>, text: &QString) {
        let text = text.to_string();
        let a = app();
        if !a.shared.settings().get_bool("general.showSearchSuggestions") || text.trim().is_empty() || a.private {
            return;
        }
        let def = a.shared.settings().get_str("general.searchEngine");
        let engine = search::load_engines(&a.shared.db()).unwrap_or_default().into_iter().find(|e| e.name == def);
        let Some(engine) = engine.filter(|e| !e.suggestions_url.is_empty()) else { return };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let url = engine.suggestions_url.replace("%s", &percent_encoding::utf8_percent_encode(&text, percent_encoding::NON_ALPHANUMERIC).to_string());
            let body = ureq_get(&url);
            let list = search::parse_suggestions(&body);
            let _ = thread.queue(move |o| o.suggestions_ready(q(&text), js(&json!(list.into_iter().take(6).collect::<Vec<_>>()))));
        });
    }

    // ---- history ----------------------------------------------------------
    // (`a.private` is the whole of Kite started with --private; `in_private` a
    // private window of a normal run.)
    fn record_visit(self: Pin<&mut Self>, url: &QString, title: &QString, in_private: bool) {
        let a = app();
        if in_private || a.private || !a.shared.settings().get_bool("privacy.saveHistory") {
            return;
        }
        let (url, title) = (url.to_string(), title.to_string());
        let added = kite_core::history::add_visit(&a.shared.db(), &url, &title).ok().flatten();
        if let Some(id) = added {
            let entry = kite_core::history::get(&a.shared.db(), &url).ok().flatten();
            if let Some(e) = entry {
                let fx = a.runtime().browser_event("historyVisited", &json!({"entry": {"id": id.to_string(), "url": e.url, "title": e.title, "lastVisitTime": e.date, "visitCount": e.count, "typedCount": 0}}));
                crate::extensions::apply_effects_async(fx);
            }
            self.history_changed();
        }
    }
    fn update_title(self: Pin<&mut Self>, url: &QString, title: &QString, in_private: bool) {
        let a = app();
        if in_private || a.private {
            return;
        }
        let _ = kite_core::history::update_title(&a.shared.db(), &url.to_string(), &title.to_string());
    }
    fn history(&self, query: &QString, start: f64, end: f64, limit: i32) -> QString {
        let list = kite_core::history::search(&app().shared.db(), &query.to_string(), start as i64, end as i64, limit.max(1) as usize).unwrap_or_default();
        js(&serde_json::to_value(list).unwrap_or_default())
    }
    fn delete_history(self: Pin<&mut Self>, ids_json: &QString) {
        let ids: Vec<i64> = serde_json::from_str(&ids_json.to_string()).unwrap_or_default();
        {
            let db = app().shared.db();
            for id in ids {
                let _ = kite_core::history::delete_id(&db, id);
            }
        }
        self.history_changed();
    }
    fn delete_history_range(self: Pin<&mut Self>, start: f64, end: f64) {
        let _ = kite_core::history::delete_range(&app().shared.db(), start as i64, end as i64);
        self.history_changed();
    }
    fn clear_history(self: Pin<&mut Self>) {
        let _ = kite_core::history::clear(&app().shared.db());
        let fx = app().runtime().browser_event("historyRemoved", &json!({"allHistory": true, "urls": []}));
        crate::extensions::apply_effects_async(fx);
        self.history_changed();
    }

    // ---- bookmarks --------------------------------------------------------
    fn bookmark_tree(&self, folder: &QString) -> QString {
        let b = app().shared.bookmarks();
        let id: u64 = folder.to_string().parse().unwrap_or(b.toolbar_id());
        let Some(n) = b.find(id) else { return q("[]") };
        js(&Value::Array(n.children.iter().map(node_brief).collect()))
    }
    fn bookmark_roots(&self) -> QString {
        let b = app().shared.bookmarks();
        js(&json!({"toolbar": b.toolbar_id().to_string(), "menu": b.menu_id().to_string(), "unsorted": b.unsorted_id().to_string(),
                   "roots": b.root.children.iter().map(node_brief).collect::<Vec<_>>()}))
    }
    fn bookmark_folders(&self) -> QString {
        let b = app().shared.bookmarks();
        let mut out = vec![];
        fn walk(n: &Node, depth: usize, out: &mut Vec<Value>) {
            for c in n.children.iter().filter(|c| c.kind == Kind::Folder) {
                out.push(json!({"id": c.id.to_string(), "title": c.name, "depth": depth}));
                walk(c, depth + 1, out);
            }
        }
        walk(&b.root, 0, &mut out);
        js(&Value::Array(out))
    }
    fn search_bookmarks(&self, text: &QString) -> QString {
        let b = app().shared.bookmarks();
        js(&Value::Array(b.search(&text.to_string()).into_iter().map(node_brief).collect()))
    }
    fn bookmark_for_url(&self, url: &QString) -> QString {
        let b = app().shared.bookmarks();
        let url = url.to_string();
        match b.find_by_url(&url).first() {
            Some(n) => {
                let parent = b.parent_of(n.id).map(|(p, _)| p.to_string()).unwrap_or_default();
                let mut v = node_brief(n);
                v["parent"] = parent.into();
                js(&v)
            }
            None => q("null"),
        }
    }
    fn add_bookmark(mut self: Pin<&mut Self>, url: &QString, title: &QString, folder: &QString) -> QString {
        let id = {
            let mut b = app().shared.bookmarks();
            let parent = folder.to_string().parse().unwrap_or(b.unsorted_id());
            b.insert(parent, None, Node::url(&title.to_string(), &url.to_string())).ok()
        };
        if let Some(id) = id {
            let j = app().shared.bookmarks().node_json(id, false);
            notify_bookmarks(json!({"name": "onCreated", "args": [id.to_string(), j]}));
        }
        self.as_mut().bookmarks_changed();
        q(id.map(|i| i.to_string()).unwrap_or_default())
    }
    fn add_folder(mut self: Pin<&mut Self>, parent: &QString, name: &QString) -> QString {
        let id = {
            let mut b = app().shared.bookmarks();
            let parent = parent.to_string().parse().unwrap_or(b.unsorted_id());
            b.insert(parent, None, Node::folder(&name.to_string())).ok()
        };
        self.as_mut().bookmarks_changed();
        q(id.map(|i| i.to_string()).unwrap_or_default())
    }
    fn add_separator(self: Pin<&mut Self>, parent: &QString) {
        {
            let mut b = app().shared.bookmarks();
            let parent = parent.to_string().parse().unwrap_or(b.toolbar_id());
            let _ = b.insert(parent, None, Node::separator());
        }
        self.bookmarks_changed();
    }
    fn update_bookmark(self: Pin<&mut Self>, id: &QString, json: &QString) {
        let v = parse(json);
        let id_s = id.to_string();
        if let Ok(id) = id_s.parse::<u64>() {
            let _ = app().shared.bookmarks().update(id, v["title"].as_str(), v["url"].as_str(), v["keyword"].as_str(), v["description"].as_str());
            notify_bookmarks(json!({"name": "onChanged", "args": [id_s, {"title": v["title"], "url": v["url"]}]}));
        }
        self.bookmarks_changed();
    }
    fn move_bookmark(self: Pin<&mut Self>, id: &QString, parent: &QString, index: i32) {
        if let (Ok(id), Ok(parent)) = (id.to_string().parse::<u64>(), parent.to_string().parse::<u64>()) {
            let _ = app().shared.bookmarks().move_to(id, parent, (index >= 0).then_some(index as usize));
        }
        self.bookmarks_changed();
    }
    fn remove_bookmark(self: Pin<&mut Self>, id: &QString) {
        let id_s = id.to_string();
        if let Ok(id) = id_s.parse::<u64>() {
            let info = {
                let b = app().shared.bookmarks();
                b.parent_of(id).map(|(p, i)| json!({"parentId": p.to_string(), "index": i, "node": b.node_json(id, true)}))
            };
            if app().shared.bookmarks().remove(id).is_ok() {
                notify_bookmarks(json!({"name": "onRemoved", "args": [id_s, info]}));
            }
        }
        self.bookmarks_changed();
    }

    /// Import Falkon `bookmarks.json` or a Netscape bookmarks HTML file
    /// (exported by Firefox, Chrome, ...). Imported items go to a new folder.
    fn import_bookmarks(self: Pin<&mut Self>, path: &QString) -> QString {
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_owned();
        let result = (|| -> anyhow::Result<usize> {
            let text = std::fs::read_to_string(&path)?;
            let mut folder = Node::folder("Imported");
            if text.trim_start().starts_with('{') {
                let other = kite_core::bookmarks::Bookmarks::load(std::path::PathBuf::from(&path));
                folder.children = other.root.children.clone();
            } else {
                folder.children = parse_netscape(&text);
            }
            fn count(n: &Node) -> usize {
                n.children.iter().map(|c| if c.kind == Kind::Url { 1 } else { count(c) }).sum()
            }
            let n = count(&folder);
            let mut b = app().shared.bookmarks();
            let menu = b.menu_id();
            b.insert(menu, None, folder)?;
            Ok(n)
        })();
        self.bookmarks_changed();
        match result {
            Ok(n) => q(format!("Imported {n} bookmarks into \"Imported\" in the Bookmarks Menu.")),
            Err(e) => q(format!("Import failed: {e}")),
        }
    }

    /// Export as Netscape bookmarks HTML (importable by every browser).
    fn export_bookmarks(&self, path: &QString) -> QString {
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_owned();
        let b = app().shared.bookmarks();
        fn esc(s: &str) -> String {
            s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
        }
        fn walk(n: &Node, ind: usize, out: &mut String) {
            let pad = "    ".repeat(ind);
            for c in &n.children {
                match c.kind {
                    Kind::Folder => {
                        out.push_str(&format!("{pad}<DT><H3>{}</H3>\n{pad}<DL><p>\n", esc(&c.name)));
                        walk(c, ind + 1, out);
                        out.push_str(&format!("{pad}</DL><p>\n"));
                    }
                    Kind::Url => out.push_str(&format!("{pad}<DT><A HREF=\"{}\" SHORTCUTURL=\"{}\">{}</A>\n", esc(&c.url), esc(&c.keyword), esc(&c.name))),
                    Kind::Separator => out.push_str(&format!("{pad}<HR>\n")),
                }
            }
        }
        let mut out = String::from("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n");
        walk(&b.root, 1, &mut out);
        out.push_str("</DL>\n");
        match std::fs::write(&path, out) {
            Ok(_) => q(format!("Bookmarks exported to {path}")),
            Err(e) => q(format!("Export failed: {e}")),
        }
    }

    // ---- session ----------------------------------------------------------
    fn save_session(self: Pin<&mut Self>, json: &QString) {
        let a = app();
        if a.private {
            return;
        }
        if let Ok(windows) = serde_json::from_str(&json.to_string()) {
            let _ = a.shared.session().set_windows(windows);
        }
    }
    fn last_session(&self) -> QString {
        js(&serde_json::to_value(&app().shared.session().data.windows).unwrap_or_default())
    }
    fn set_running(self: Pin<&mut Self>, running: bool) {
        let mut s = app().shared.session();
        s.data.running = running;
        let _ = s.save();
    }
    fn crashed_last_time(&self) -> bool {
        app().shared.session().crashed_last_time()
    }
    fn push_closed_tab(self: Pin<&mut Self>, json: &QString) {
        if let Ok(t) = serde_json::from_str(&json.to_string()) {
            app().shared.session().push_closed_tab(t);
        }
        self.closed_changed();
    }
    fn pop_closed_tab(self: Pin<&mut Self>, index: i32) -> QString {
        let t = app().shared.session().pop_closed_tab(index.max(0) as usize);
        let out = js(&serde_json::to_value(t).unwrap_or_default());
        self.closed_changed();
        out
    }
    fn closed_tabs(&self) -> QString {
        js(&serde_json::to_value(&app().shared.session().data.closed_tabs).unwrap_or_default())
    }
    fn push_closed_window(self: Pin<&mut Self>, json: &QString) {
        if let Ok(w) = serde_json::from_str(&json.to_string()) {
            app().shared.session().push_closed_window(w);
        }
        self.closed_changed();
    }
    fn pop_closed_window(self: Pin<&mut Self>, index: i32) -> QString {
        let w = app().shared.session().pop_closed_window(index.max(0) as usize);
        let out = js(&serde_json::to_value(w).unwrap_or_default());
        self.closed_changed();
        out
    }
    fn closed_windows(&self) -> QString {
        js(&serde_json::to_value(&app().shared.session().data.closed_windows).unwrap_or_default())
    }

    // ---- downloads --------------------------------------------------------
    /// Create (id <= 0) or update a download record; returns its id.
    fn download_update(self: Pin<&mut Self>, json: &QString) -> i32 {
        let v = parse(json);
        let a = app();
        let (id, created, rec) = {
            let mut d = a.shared.downloads();
            let mut id = v["id"].as_u64().unwrap_or(0);
            let created = id == 0;
            if created {
                id = d.next_id();
            }
            let old = d.items.iter().find(|i| i.id == id).cloned();
            let rec = kite_core::downloads::DownloadRecord {
                id,
                url: v["url"].as_str().map(str::to_owned).or(old.as_ref().map(|o| o.url.clone())).unwrap_or_default(),
                path: v["path"].as_str().map(str::to_owned).or(old.as_ref().map(|o| o.path.clone())).unwrap_or_default(),
                mime: v["mime"].as_str().map(str::to_owned).or(old.as_ref().map(|o| o.mime.clone())).unwrap_or_default(),
                total: v["total"].as_i64().unwrap_or(old.as_ref().map(|o| o.total).unwrap_or(0)),
                received: v["received"].as_i64().unwrap_or(old.as_ref().map(|o| o.received).unwrap_or(0)),
                state: v["state"].as_str().map(str::to_owned).or(old.as_ref().map(|o| o.state.clone())).unwrap_or_else(|| "in_progress".into()),
                started: old.as_ref().map(|o| o.started).unwrap_or_else(kite_core::util::now_ms),
                private: v["private"].as_bool().or(old.as_ref().map(|o| o.private)).unwrap_or(false),
                finished: if v["state"].as_str().is_some_and(|s| s != "in_progress") { kite_core::util::now_ms() } else { old.as_ref().map(|o| o.finished).unwrap_or(0) },
            };
            if !a.private {
                d.upsert(rec.clone());
            }
            (id, created, rec)
        };
        let j = kite_core::webext::runtime::download_json(&rec);
        let fx = if rec.private {
            vec![]
        } else if created {
            a.runtime().browser_event("downloadCreated", &json!({"args": [j]}))
        } else if v["state"].is_string() {
            a.runtime().browser_event("downloadChanged", &json!({"args": [{"id": id, "state": {"current": rec.state}}]}))
        } else {
            vec![]
        };
        crate::extensions::apply_effects_async(fx);
        self.downloads_changed();
        id as i32
    }
    fn forget_private_downloads(self: Pin<&mut Self>) {
        app().shared.downloads().forget_private();
        self.downloads_changed();
    }
    fn downloads(&self) -> QString {
        js(&serde_json::to_value(&app().shared.downloads().items).unwrap_or_default())
    }
    fn remove_download(self: Pin<&mut Self>, id: i32) {
        app().shared.downloads().remove(id as u64);
        self.downloads_changed();
    }
    fn clear_downloads(self: Pin<&mut Self>) {
        app().shared.downloads().clear_finished();
        self.downloads_changed();
    }
    fn unique_download_path(&self, dir: &QString, name: &QString) -> QString {
        let dir = std::path::PathBuf::from(dir.to_string());
        let name = name.to_string();
        let name = if name.is_empty() { "download".to_owned() } else { name.replace('/', "_") };
        let mut p = dir.join(&name);
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut i = 1;
        while p.exists() {
            p = dir.join(format!("{stem} ({i}){ext}"));
            i += 1;
        }
        q(p.to_string_lossy())
    }

    // ---- search engines -----------------------------------------------------
    fn engines(&self) -> QString {
        js(&serde_json::to_value(search::load_engines(&app().shared.db()).unwrap_or_default()).unwrap_or_default())
    }
    fn save_engines(self: Pin<&mut Self>, json: &QString) {
        if let Ok(list) = serde_json::from_str::<Vec<search::SearchEngine>>(&json.to_string()) {
            let _ = search::save_engines(&app().shared.db(), &list);
        }
    }
    fn search_url(&self, engine: &QString, query: &QString) -> QString {
        let engines = search::load_engines(&app().shared.db()).unwrap_or_default();
        let name = engine.to_string();
        let e = engines.iter().find(|e| e.name == name).or_else(|| engines.first());
        q(e.map(|e| search::search_url(e, &query.to_string())).unwrap_or_default())
    }

    // ---- site settings ------------------------------------------------------
    fn site_settings(&self, host: &QString) -> QString {
        js(&serde_json::to_value(kite_core::sitesettings::get(&app().shared.db(), &host.to_string()).unwrap_or_default()).unwrap_or_default())
    }
    fn set_site_setting(self: Pin<&mut Self>, host: &QString, key: &QString, value: i32) {
        let a = app();
        if a.private && key.to_string() == "zoom" {
            return;
        }
        let _ = kite_core::sitesettings::set(&a.shared.db(), &host.to_string(), &key.to_string(), value as i64);
        app().privacy.load_cookie_rules(&app().shared);
    }
    fn all_site_settings(&self) -> QString {
        let list = kite_core::sitesettings::all(&app().shared.db()).unwrap_or_default();
        js(&Value::Array(list.into_iter().map(|(h, s)| json!({"host": h, "settings": s})).collect()))
    }
    fn clear_site_settings(self: Pin<&mut Self>, host: &QString) {
        let _ = kite_core::sitesettings::clear(&app().shared.db(), &host.to_string());
        app().privacy.load_cookie_rules(&app().shared);
    }
    fn https_upgrade_failed(&self, url: &QString) -> bool {
        let url = url.to_string();
        url.starts_with("https:") && app().privacy.was_upgraded(&url)
    }
    fn fingerprint_script(&self, seed: u32) -> QString {
        q(&include_str!("js/fingerprint.js").replace("__KITE_SEED__", &seed.to_string()))
    }
    fn hook_container_profiles(&self) {
        crate::ffi::hook_container_profiles();
    }
    fn delete_session_cookies(&self) {
        crate::ffi::delete_session_cookies();
    }
    fn start_page_import(&self, target: &QString, path: &QString) -> bool {
        let root = app().shared.profile.root().to_owned();
        match kite_core::startpage::import(&root, &target.to_string(), std::path::Path::new(&path.to_string())) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("start page: cannot copy {path}: {e}");
                false
            }
        }
    }
    fn start_page_create(&self, target: &QString) -> QString {
        let root = app().shared.profile.root().to_owned();
        q(&kite_core::startpage::create(&root, &target.to_string()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
    }
    fn start_page_copy_path(&self, target: &QString) -> QString {
        q(&kite_core::startpage::index(app().shared.profile.root(), &target.to_string()).to_string_lossy())
    }
    fn file_modified(&self, path: &QString) -> f64 {
        std::fs::metadata(path.to_string())
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
    fn safe_browsing_check(&self, url: &QString) -> QString {
        q(app().safe_browsing.check(&url.to_string()).map(|t| t.as_str()).unwrap_or(""))
    }
    fn safe_browsing_allow(&self, host: &QString) {
        app().safe_browsing.allow(&host.to_string());
    }
    fn safe_browsing_status(&self) -> QString {
        q(&app().safe_browsing.status())
    }
    fn allow_http(&self, host: &QString) {
        app().privacy.allow_http(&host.to_string());
    }
    fn set_site_text(self: Pin<&mut Self>, host: &QString, key: &QString, value: &QString) {
        let _ = kite_core::sitesettings::set_text(&app().shared.db(), &host.to_string(), &key.to_string(), &value.to_string());
    }
    fn reset_site_settings(self: Pin<&mut Self>, host: &QString) {
        let _ = kite_core::sitesettings::reset(&app().shared.db(), &host.to_string());
        app().privacy.load_cookie_rules(&app().shared);
    }

    // ---- content blocking ---------------------------------------------------
    fn reload_adblock(self: Pin<&mut Self>, download: bool) {
        let thread = self.qt_thread();
        app().reload_adblock(download, move |r| {
            let msg = match r {
                Ok(n) => format!("Content blocker ready ({n} filter lines)"),
                Err(e) => format!("Content blocker error: {e}"),
            };
            let _ = thread.queue(move |o| o.adblock_status(q(msg)));
        });
    }
    fn adblock_blocked(&self) -> i32 {
        app().adblock.blocked_count.load(std::sync::atomic::Ordering::Relaxed) as i32
    }
    fn adblock_allow(self: Pin<&mut Self>, host: &QString, allow: bool) {
        let a = app();
        let host = host.to_string().trim_start_matches("www.").to_owned();
        let mut list: Vec<String> = serde_json::from_value(a.shared.settings().get("adblock.allowlist")).unwrap_or_default();
        list.retain(|h| *h != host);
        if allow {
            list.push(host);
        }
        let _ = a.shared.settings().set("adblock.allowlist", json!(list));
        a.adblock.set_allowlist(list);
    }
    fn set_wake_lock(&self, tab_id: i32, on: bool, site: &QString) -> bool {
        let mut screen = app().screen.lock().unwrap_or_else(|e| e.into_inner());
        match screen.set(tab_id as i64, on, &format!("{site} is keeping the screen on")) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("screen wake lock for tab {tab_id}: {e}");
                false
            }
        }
    }

    fn wake_lock_script(&self) -> QString {
        q(include_str!("js/wakelock.js"))
    }

    fn scrollbar_script(&self) -> QString {
        // Whether Chromium was started with overlay scrollbars (main.rs, or
        // the user's own flags): the setting may have changed since.
        let on = std::env::var("QTWEBENGINE_CHROMIUM_FLAGS").is_ok_and(|f| f.contains("FluentOverlayScrollbar"));
        q(if on { include_str!("js/scrollbars.js") } else { "" })
    }

    fn print_dialog(self: Pin<&mut Self>, title: &QString) {
        *PRINT_THREAD.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.qt_thread());
        let Ok(title) = std::ffi::CString::new(title.to_string()) else { return };
        unsafe { crate::ffi::kite_cpp_print_dialog(title.as_ptr()) }
    }

    fn print_submit(&self) {
        unsafe { crate::ffi::kite_cpp_print_submit() }
    }

    fn print_cancel(&self) {
        unsafe { crate::ffi::kite_cpp_print_cancel() }
    }

    fn reader_script(&self) -> QString {
        q(concat!("(() => {", include_str!("js/readability/Readability.js"), include_str!("js/reader_extract.js"), "})()"))
    }

    fn reader_probe_script(&self) -> QString {
        q(concat!("(() => {", include_str!("js/readability/Readability-readerable.js"), "\nreturn isProbablyReaderable(document);})()"))
    }

    fn reader_open(&self, source: &QString, json: &QString, background: &QString, text: &QString, link: &QString) -> QString {
        let colors = kite_core::reader::Colors { background: background.to_string(), text: text.to_string(), link: link.to_string() };
        q(kite_core::reader::open(&source.to_string(), &json.to_string(), colors).unwrap_or_default())
    }

    fn reader_source(&self, url: &QString) -> QString {
        q(kite_core::reader::source_of(&url.to_string()).unwrap_or_default())
    }

    fn reader_apply_script(&self) -> QString {
        q(kite_core::reader::Prefs::from_settings(&app().shared.settings()).apply_script())
    }

    fn cached_favicon(&self, page_url: &QString) -> QString {
        use base64::Engine;
        match kite_core::favicons::for_url(&app().shared.db(), &page_url.to_string()) {
            Ok(Some(png)) => q(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))),
            _ => QString::default(),
        }
    }

    fn save_favicon(&self, page_url: &QString, icon_url: &QString) {
        crate::ffi::save_favicon(&page_url.to_string(), &icon_url.to_string());
    }

    fn favicon_track(&self, view_name: &QString, track: bool) {
        crate::ffi::favicon_track(&view_name.to_string(), track);
    }

    fn key_sequence_text(&self, key: i32, modifiers: i32) -> QString {
        q(crate::ffi::key_text(key, modifiers))
    }

    fn key_sequence_display(&self, sequence: &QString) -> QString {
        q(crate::ffi::key_native(&sequence.to_string()))
    }

    fn system_wheel_pixels(&self) -> i32 {
        crate::ffi::system_wheel_pixels()
    }

    fn content_blocker(&self, in_private: bool) -> QString {
        q(app().runtime().content_blocker_for(in_private).unwrap_or_default())
    }

    fn adblock_allowed(&self, host: &QString) -> bool {
        let host = host.to_string();
        let list: Vec<String> = serde_json::from_value(app().shared.settings().get("adblock.allowlist")).unwrap_or_default();
        list.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
    }
    fn cosmetic(&self, url: &QString, in_private: bool) -> QString {
        // Cosmetic filters and scriptlets are the extension's job too.
        if app().runtime().content_blocker_for(in_private).is_some() {
            return js(&json!({}));
        }
        let (css, script) = app().adblock.cosmetic(&url.to_string());
        js(&json!({"css": css, "script": script}))
    }

    // ---- misc -----------------------------------------------------------------
    fn temp_file(&self, name: &QString) -> QString {
        let dir = std::env::temp_dir().join(format!("kite-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        q(dir.join(name.to_string().replace('/', "_")).to_string_lossy())
    }
    fn file_to_data_url(&self, path: &QString) -> QString {
        use base64::Engine;
        let p = path.to_string();
        let p = p.strip_prefix("file://").unwrap_or(&p).to_owned();
        let mime = mime_guess::from_path(&p).first_or_octet_stream().essence_str().to_owned();
        match std::fs::read(&p) {
            Ok(b) => {
                let _ = std::fs::remove_file(&p);
                q(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)))
            }
            Err(_) => q(""),
        }
    }
    fn host_of(&self, url: &QString) -> QString {
        q(kite_core::util::host_of(&url.to_string()))
    }

    // ---- privacy ------------------------------------------------------------
    /// {history, cookies, cache, downloads, closedTabs, siteSettings, since(ms)}
    fn clear_private_data(mut self: Pin<&mut Self>, json: &QString) {
        let v = parse(json);
        let a = app();
        let since = v["since"].as_f64().unwrap_or(0.0) as i64;
        if v["history"].as_bool() == Some(true) {
            let db = a.shared.db();
            let _ = if since > 0 { kite_core::history::delete_range(&db, since, i64::MAX).map(|_| ()) } else { kite_core::history::clear(&db) };
        }
        if v["cookies"].as_bool() == Some(true) {
            crate::ffi::delete_all_cookies();
        }
        if v["cache"].as_bool() == Some(true) {
            crate::ffi::clear_http_cache();
        }
        if v["downloads"].as_bool() == Some(true) {
            a.shared.downloads().clear_finished();
        }
        if v["closedTabs"].as_bool() == Some(true) {
            let mut s = a.shared.session();
            s.data.closed_tabs.clear();
            s.data.closed_windows.clear();
            let _ = s.save();
        }
        self.as_mut().history_changed();
        self.as_mut().downloads_changed();
        self.as_mut().closed_changed();
    }
}

fn ureq_get(url: &str) -> String {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(4))).build().into();
    agent
        .get(url)
        .header("User-Agent", kite_core::webext::amo::USER_AGENT)
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
        .unwrap_or_default()
}

/// Minimal Netscape bookmark-file parser.
pub fn parse_netscape(html: &str) -> Vec<Node> {
    let re = regex::Regex::new(r#"(?is)<DT>\s*<H3[^>]*>(.*?)</H3>|<DT>\s*<A\s+([^>]*)>(.*?)</A>|<DL>|</DL>|<HR>"#).unwrap();
    let attr = regex::Regex::new(r#"(?i)(HREF|SHORTCUTURL)\s*=\s*"([^"]*)""#).unwrap();
    let unesc = |s: &str| s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
    let mut stack: Vec<Node> = vec![Node::folder("root")];
    let mut pending: Option<Node> = None;
    let mut depth = 0;
    for c in re.captures_iter(html) {
        let m = c.get(0).unwrap().as_str();
        if let Some(t) = c.get(1) {
            pending = Some(Node::folder(&unesc(t.as_str().trim())));
        } else if let Some(a) = c.get(2) {
            let mut url = String::new();
            let mut kw = String::new();
            for x in attr.captures_iter(a.as_str()) {
                match x[1].to_ascii_uppercase().as_str() {
                    "HREF" => url = unesc(&x[2]),
                    _ => kw = unesc(&x[2]),
                }
            }
            if !url.is_empty() && !url.starts_with("place:") {
                let mut n = Node::url(&unesc(c.get(3).map(|t| t.as_str().trim()).unwrap_or("")), &url);
                n.keyword = kw;
                stack.last_mut().unwrap().children.push(n);
            }
        } else if m.eq_ignore_ascii_case("<DL>") {
            depth += 1;
            if depth > 1 {
                stack.push(pending.take().unwrap_or_else(|| Node::folder("Folder")));
            }
        } else if m.eq_ignore_ascii_case("</DL>") {
            if depth > 1 {
                if let Some(done) = stack.pop() {
                    stack.last_mut().unwrap().children.push(done);
                }
            }
            depth -= 1;
        } else if m.eq_ignore_ascii_case("<HR>") {
            stack.last_mut().unwrap().children.push(Node::separator());
        }
    }
    while stack.len() > 1 {
        let done = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(done);
    }
    stack.pop().map(|r| r.children).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn netscape() {
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p>
            <DT><H3>Toolbar</H3><DL><p><DT><A HREF="https://kde.org/" SHORTCUTURL="kde">KDE &amp; co</A></DL><p>
            <HR><DT><A HREF="https://rust-lang.org/">Rust</A></DL>"#;
        let nodes = super::parse_netscape(html);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].name, "Toolbar");
        assert_eq!(nodes[0].children[0].name, "KDE & co");
        assert_eq!(nodes[0].children[0].keyword, "kde");
        assert_eq!(nodes[2].url, "https://rust-lang.org/");
    }
}
// Answers from the print dialog and lp (cpp/print.cpp), on the GUI thread.
static PRINT_THREAD: std::sync::Mutex<Option<cxx_qt::CxxQtThread<qobject::Browser>>> = std::sync::Mutex::new(None);

#[no_mangle]
pub unsafe extern "C" fn kite_print_chosen(json: *const std::os::raw::c_char) {
    let json = std::ffi::CStr::from_ptr(json).to_string_lossy().into_owned();
    if let Some(t) = PRINT_THREAD.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = t.queue(move |o| o.print_chosen(q(json)));
    }
}

#[no_mangle]
pub unsafe extern "C" fn kite_print_done(ok: i32, message: *const std::os::raw::c_char) {
    let message = std::ffi::CStr::from_ptr(message).to_string_lossy().into_owned();
    if let Some(t) = PRINT_THREAD.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = t.queue(move |o| o.print_done(ok != 0, q(message)));
    }
}
