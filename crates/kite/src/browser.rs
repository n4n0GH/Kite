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
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
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
        /// The last crash's report (crash.rs), or "".
        /// Hand a download to the external download manager (Settings →
        /// Downloads): {"name": the program} or {"error": why not}.
        #[qinvokable]
        #[cxx_name = "startExternalDownload"]
        fn start_external_download(self: &Browser, url: &QString, file_name: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "crashReport"]
        fn crash_report(self: &Browser) -> QString;
        /// Whether that report is new since the last start (true once).
        #[qinvokable]
        #[cxx_name = "takeNewCrashReport"]
        fn take_new_crash_report(self: &Browser) -> bool;
        /// The folder with the log and crash reports, as a file: URL.
        #[qinvokable]
        #[cxx_name = "logFolderUrl"]
        fn log_folder_url(self: &Browser) -> QString;
        /// Put Kite's crash handler back in front if QtWebEngine replaced it.
        #[qinvokable]
        #[cxx_name = "checkCrashHandler"]
        fn check_crash_handler(self: &Browser);
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
        /// What makes a site light ("Always force light" in Site Settings).
        #[qinvokable]
        #[cxx_name = "forceLightScript"]
        fn force_light_script(self: &Browser) -> QString;
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
        /// A "continue anyway" link from the dangerous-site or HTTPS-only
        /// warning (`kite:dangerous-allow?url=…&key=…`, `kite:https-allow?…`):
        /// makes the exception and returns the page to load, or "" when the
        /// link didn't come from Kite's own warning page.
        #[qinvokable]
        #[cxx_name = "allowFromWarning"]
        fn allow_from_warning(self: &Browser, link: &QString) -> QString;
        /// The lists' state, for Settings.
        #[qinvokable]
        #[cxx_name = "safeBrowsingStatus"]
        fn safe_browsing_status(self: &Browser) -> QString;
        /// Started with --debug (state.rs).
        #[qinvokable]
        #[cxx_name = "debugMode"]
        fn debug_mode(self: &Browser) -> bool;
        /// Dangerous Sites' stored entries, for --debug: {total, entries: [{list, entry, threat}]}.
        #[qinvokable]
        #[cxx_name = "safeBrowsingEntries"]
        fn safe_browsing_entries(self: &Browser, filter: &QString, offset: i32, limit: i32) -> QString;
        /// How Dangerous Sites would check an address, from what's stored (--debug).
        #[qinvokable]
        #[cxx_name = "safeBrowsingExplain"]
        fn safe_browsing_explain(self: &Browser, url: &QString) -> QString;
        /// Text settings: "css" or "js".
        #[qinvokable]
        #[cxx_name = "setSiteText"]
        fn set_site_text(self: Pin<&mut Browser>, host: &QString, key: &QString, value: &QString);
        /// Back to the defaults, keeping custom CSS and JavaScript.
        #[qinvokable]
        #[cxx_name = "resetSiteSettings"]
        fn reset_site_settings(self: Pin<&mut Browser>, host: &QString);
        /// The user agents a site can be given: [{name, userAgent}].
        #[qinvokable]
        #[cxx_name = "userAgentPresets"]
        fn user_agent_presets(self: &Browser) -> QString;
        /// QtWebEngine's own user agent, which a site's "Kite (naming QtWebEngine)" stands for.
        #[qinvokable]
        #[cxx_name = "setQtWebEngineUserAgent"]
        fn set_qtwebengine_user_agent(self: &Browser, ua: &QString);
        /// The page script making a site's pages see its chosen user agent ("" = none).
        /// The installed spelling dictionaries' languages (as a JSON list).
        #[qinvokable]
        #[cxx_name = "spellDictionaries"]
        fn spell_dictionaries(self: &Browser) -> QString;
        /// Languages (a JSON list) as the installed dictionaries name them.
        #[qinvokable]
        #[cxx_name = "resolveSpellLanguages"]
        fn resolve_spell_languages(self: &Browser, languages: &QString) -> QString;
        /// "web.spellcheckLanguages", resolved so (for the profiles).
        #[qinvokable]
        #[cxx_name = "spellCheckLanguages"]
        fn spell_check_languages(self: &Browser) -> QString;
        /// The page script making all pages see the user agent chosen for all
        /// sites, when it's another browser's ("" = none).
        #[qinvokable]
        #[cxx_name = "globalIdentityScript"]
        fn global_identity_script(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "siteIdentityScript"]
        fn site_identity_script(self: &Browser, host: &QString) -> QString;

        // content blocking
        #[qinvokable]
        #[cxx_name = "reloadAdblock"]
        fn reload_adblock(self: Pin<&mut Browser>, download: bool);
        /// The files in the folder at a file: URL, up to 10000 (more gives 10001).
        #[qinvokable]
        #[cxx_name = "folderFileCount"]
        fn folder_file_count(self: &Browser, folder: &QString) -> i32;
        /// The path of a file: URL, or "".
        #[qinvokable]
        #[cxx_name = "localPath"]
        fn local_path(self: &Browser, url: &QString) -> QString;
        /// Whether a file: URL is a folder.
        #[qinvokable]
        #[cxx_name = "folderExists"]
        fn folder_exists(self: &Browser, folder: &QString) -> bool;
        /// Name filters for a file dialog from what a page's file input accepts.
        #[qinvokable]
        #[cxx_name = "fileFilters"]
        fn file_filters(self: &Browser, accepted: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "refreshAdblock"]
        fn refresh_adblock(self: Pin<&mut Browser>);
        #[qinvokable]
        #[cxx_name = "adblockBlocked"]
        fn adblock_blocked(self: &Browser) -> i32;
        /// What was blocked on pages of a site, this session.
        #[qinvokable]
        #[cxx_name = "adblockBlockedOn"]
        fn adblock_blocked_on(self: &Browser, host: &QString) -> i32;
        /// The latest of those, newest first: [{url, kind, at, replaced}].
        #[qinvokable]
        #[cxx_name = "adblockBlockedList"]
        fn adblock_blocked_list(self: &Browser, host: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "adblockClearBlocked"]
        fn adblock_clear_blocked(self: &Browser, host: &QString);
        #[qinvokable]
        #[cxx_name = "adblockAllow"]
        fn adblock_allow(self: Pin<&mut Browser>, host: &QString, allow: bool);
        #[qinvokable]
        #[cxx_name = "adblockAllowed"]
        fn adblock_allowed(self: &Browser, host: &QString) -> bool;
        /// What the content blocker has for a page, as it's about to be loaded: "" for
        /// nothing, when it's off for the page or an extension does the blocking.
        #[qinvokable]
        #[cxx_name = "adblockPagePlan"]
        fn adblock_page_plan(self: &Browser, url: &QString, in_private: bool) -> QString;
        /// What generic rules hide among a page's classes and ids: {c, i, x} to a list.
        #[qinvokable]
        #[cxx_name = "adblockGeneric"]
        fn adblock_generic(self: &Browser, found: &QString) -> QString;
        /// The page script for choosing an element to block (js/element_picker.js).
        #[qinvokable]
        #[cxx_name = "elementPickerScript"]
        fn element_picker_script(self: &Browser) -> QString;
        /// The page script applying a plan (js/adblock_content.js).
        #[qinvokable]
        #[cxx_name = "adblockContentScript"]
        fn adblock_content_script(self: &Browser) -> QString;
        /// Keep the screen on for a tab holding a Screen Wake Lock (or stop).
        #[qinvokable]
        #[cxx_name = "setWakeLock"]
        fn set_wake_lock(self: &Browser, tab_id: i32, on: bool, site: &QString) -> bool;
        /// Plasma Integration: what a frame of a tab is playing, or "null" for nothing.
        #[qinvokable]
        #[cxx_name = "plasmaMedia"]
        fn plasma_media(self: &Browser, tab_id: i32, frame: &QString, state: &QString);
        #[qinvokable]
        #[cxx_name = "plasmaForgetTab"]
        fn plasma_forget_tab(self: &Browser, tab_id: i32);
        /// What the desktop asks of what's playing (kite_core::plasma::mpris::Command).
        #[qsignal]
        #[cxx_name = "plasmaCommand"]
        fn plasma_command(self: Pin<&mut Browser>, command: QString);
        /// KDE Connect devices as of the last refresh: [{id, name, icon}].
        #[qinvokable]
        #[cxx_name = "plasmaJobStarted"]
        fn plasma_job_started(self: &Browser, id: i32, path: &QString, url: &QString);
        #[qinvokable]
        #[cxx_name = "plasmaJobProgress"]
        fn plasma_job_progress(self: &Browser, id: i32, received: f64, total: f64);
        #[qinvokable]
        #[cxx_name = "plasmaJobPaused"]
        fn plasma_job_paused(self: &Browser, id: i32, paused: bool);
        /// state: "complete", "cancelled" or "interrupted".
        #[qinvokable]
        #[cxx_name = "plasmaJobFinished"]
        fn plasma_job_finished(self: &Browser, id: i32, state: &QString, received: f64, error: &QString);
        #[qinvokable]
        #[cxx_name = "plasmaJobsForget"]
        fn plasma_jobs_forget(self: &Browser);
        /// What the desktop has to say about a download (kite_core::plasma::jobs::Event).
        #[qsignal]
        #[cxx_name = "plasmaJobEvent"]
        fn plasma_job_event(self: Pin<&mut Browser>, event: QString);
        #[qinvokable]
        #[cxx_name = "plasmaRunnerConfigure"]
        fn plasma_runner_configure(self: &Browser, config: &QString);
        #[qinvokable]
        #[cxx_name = "plasmaRunnerAnswerTabs"]
        fn plasma_runner_answer_tabs(self: &Browser, tabs: &QString);
        /// KRunner is searching: what tabs are there? (plasmaRunnerAnswerTabs)
        #[qsignal]
        #[cxx_name = "plasmaRunnerWantsTabs"]
        fn plasma_runner_wants_tabs(self: Pin<&mut Browser>);
        /// Something was picked in KRunner (kite_core::plasma::krunner::Picked).
        #[qsignal]
        #[cxx_name = "plasmaRunnerPicked"]
        fn plasma_runner_picked(self: Pin<&mut Browser>, picked: QString);
        /// From here on, a Kite started on this profile passes on what it was asked
        /// to open (startedAgain). Until then, it waits.
        #[qinvokable]
        #[cxx_name = "listenForStarts"]
        fn listen_for_starts(self: &Browser);
        /// A Kite started on this profile: {urls, newWindow, token}.
        #[qsignal]
        #[cxx_name = "startedAgain"]
        fn started_again(self: Pin<&mut Browser>, request: QString);
        /// Web MIDI for a tab (kite_core::midi): the ports as JSON, or "" without MIDI
        /// here. Whether a site may have this is decided before asking (WebTab.qml).
        #[qinvokable]
        #[cxx_name = "midiStart"]
        fn midi_start(self: &Browser, tab_id: i32, site: &QString, sysex: bool) -> QString;
        #[qinvokable]
        #[cxx_name = "midiCommand"]
        fn midi_command(self: &Browser, tab_id: i32, command: &QString);
        #[qinvokable]
        #[cxx_name = "midiStop"]
        fn midi_stop(self: &Browser, tab_id: i32);
        #[qsignal]
        #[cxx_name = "midiEvent"]
        fn midi_event(self: Pin<&mut Browser>, tab_id: i32, event: QString);
        /// Page-world script providing navigator.requestMIDIAccess (js/webmidi.js).
        #[qinvokable]
        #[cxx_name = "webMidiScript"]
        fn web_midi_script(self: &Browser) -> QString;
        /// Lets the next window raised take the focus, on Wayland.
        #[qinvokable]
        #[cxx_name = "useActivationToken"]
        fn use_activation_token(self: &Browser, token: &QString);
        #[qinvokable]
        #[cxx_name = "plasmaDevices"]
        fn plasma_devices(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "plasmaRefreshDevices"]
        fn plasma_refresh_devices(self: &Browser);
        #[qinvokable]
        #[cxx_name = "plasmaCanSend"]
        fn plasma_can_send(self: &Browser, url: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "plasmaShare"]
        fn plasma_share(self: &Browser, device: &QString, url: &QString);
        /// Page-world script providing navigator.wakeLock (js/wakelock.js).
        #[qinvokable]
        #[cxx_name = "wakeLockScript"]
        fn wake_lock_script(self: &Browser) -> QString;
        /// Page-world script providing pointer lock, in every frame (js/pointerlock.js).
        #[qinvokable]
        #[cxx_name = "pointerLockScript"]
        fn pointer_lock_script(self: &Browser) -> QString;
        /// Its isolated-world relay to Kite (js/pointerlock_relay.js).
        #[qinvokable]
        #[cxx_name = "pointerLockRelayScript"]
        fn pointer_lock_relay_script(self: &Browser) -> QString;
        /// Kite's right-click menu on pages that keep it from the user
        /// (js/context_menu.js; `unblock`: "web.unblockContextMenu").
        #[qinvokable]
        #[cxx_name = "contextMenuScript"]
        fn context_menu_script(self: &Browser, unblock: bool) -> QString;
        /// Its request: the user's last right-click again, past the page.
        #[qinvokable]
        #[cxx_name = "replayContextClick"]
        fn replay_context_click(self: &Browser);
        /// Selecting and copying text on pages that keep it from the user
        /// (js/text_unblock.js, "web.unblockSelection").
        #[qinvokable]
        #[cxx_name = "textUnblockScript"]
        fn text_unblock_script(self: &Browser) -> QString;
        /// Temporary storage for third-party frames that are refused it.
        #[qinvokable]
        #[cxx_name = "thirdPartyStorageScript"]
        fn third_party_storage_script(self: &Browser) -> QString;
        /// Script giving pages' custom scrollbars the overlay style
        /// (js/scrollbars.js), or "" without overlay scrollbars.
        #[qinvokable]
        #[cxx_name = "scrollbarScript"]
        fn scrollbar_script(self: &Browser) -> QString;
        /// Sessions kept under a name (session::NamedSessions). What changes one
        /// answers with an error message, or "". The windows are as saveSession's.
        #[qinvokable]
        #[cxx_name = "namedSessions"]
        fn named_sessions(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "namedSession"]
        fn named_session(self: &Browser, name: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "saveNamedSession"]
        fn save_named_session(self: &Browser, name: &QString, windows: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "renameNamedSession"]
        fn rename_named_session(self: &Browser, from: &QString, to: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "deleteNamedSession"]
        fn delete_named_session(self: &Browser, name: &QString) -> QString;
        /// An address for another program (search::is_external_url).
        #[qinvokable]
        #[cxx_name = "isExternalUrl"]
        fn is_external_url(self: &Browser, url: &QString) -> bool;
        /// Forms in frames (webext::scripts::frame_forms_js): the script, and the
        /// random secret starting its console messages, for this run.
        #[qinvokable]
        #[cxx_name = "frameFormsScript"]
        fn frame_forms_script(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "frameSecret"]
        fn frame_secret(self: &Browser) -> QString;

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

        /// The profiles, as for the picker, and this one's name.
        #[qinvokable]
        fn profiles(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "currentProfile"]
        fn current_profile(self: &Browser) -> QString;
        /// Start Kite with that profile.
        #[qinvokable]
        #[cxx_name = "openProfile"]
        fn open_profile(self: &Browser, name: &QString);
        /// The profile picker, its choice starting another Kite.
        #[qinvokable]
        #[cxx_name = "manageProfiles"]
        fn manage_profiles(self: &Browser);

        /// Reader view (the reader-view plugin extracts articles; kite_core::reader
        /// shows them): keep an article; the kite:reader address showing it, or "".
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
        fn save_favicon(self: &Browser, page_url: &QString, icon_url: &QString, view_name: &QString);
        /// Cache a favicon a woken tab's page fetched itself (a PNG data: URL,
        /// see WebTab.qml fetchIcon).
        #[qinvokable]
        #[cxx_name = "saveFetchedFavicon"]
        fn save_fetched_favicon(self: &Browser, page_url: &QString, data_url: &QString);
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
        /// Kite's optional features (kite_core::plugins), as a JSON list.
        #[qinvokable]
        fn plugins(self: &Browser) -> QString;
        /// A file of a plugin's page script: from the list in this file, or installed.
        #[qinvokable]
        #[cxx_name = "pluginFile"]
        fn plugin_file(self: &Browser, path: &QString) -> QString;
        /// Where a page load should go instead, or "" (ffi::page_load_redirect).
        #[qinvokable]
        #[cxx_name = "pageLoadRedirect"]
        fn page_load_redirect(self: &Browser, url: &QString, in_private: bool) -> QString;
        /// A message from kite:welcome (WebTab): its answer, as JSON, or "null".
        #[qinvokable]
        #[cxx_name = "welcomeCommand"]
        fn welcome_command(self: Pin<&mut Browser>, message: &QString) -> QString;
        /// This profile hasn't finished its welcome page (never in private mode).
        #[qinvokable]
        #[cxx_name = "welcomePending"]
        fn welcome_pending(self: &Browser) -> bool;
        /// Browsing started before the welcome page was finished: the default
        /// (High) level, nothing imported.
        #[qinvokable]
        #[cxx_name = "welcomeAbandon"]
        fn welcome_abandon(self: Pin<&mut Browser>);
        /// A change from the kite:config page (WebTab): its answer, as JSON.
        #[qinvokable]
        #[cxx_name = "configCommand"]
        fn config_command(self: Pin<&mut Browser>, message: &QString) -> QString;
        /// What to tell the user once a window is open, or "".
        #[qinvokable]
        #[cxx_name = "startupNotice"]
        fn startup_notice(self: &Browser) -> QString;
        /// Started with --no-plugins.
        #[qinvokable]
        #[cxx_name = "pluginsDisabled"]
        fn plugins_disabled(self: &Browser) -> bool;
        /// What a plugin file is: {plugin, update} or {error}. Nothing of it runs.
        #[qinvokable]
        #[cxx_name = "inspectPlugin"]
        fn inspect_plugin(self: &Browser, path: &QString) -> QString;
        /// Installs the file last inspected, as it was then. An error, or "".
        #[qinvokable]
        #[cxx_name = "installInspectedPlugin"]
        fn install_inspected_plugin(self: &Browser) -> QString;
        /// Deletes an installed plugin. An error, or "".
        #[qinvokable]
        #[cxx_name = "removePlugin"]
        fn remove_plugin(self: &Browser, id: &QString) -> QString;
        /// The address selected text is written as, or "".
        // Stored cookies (Settings → Privacy): by site, as JSON. The main profile's.
        #[qinvokable]
        #[cxx_name = "cookiesBySite"]
        fn cookies_by_site(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "removeCookie"]
        fn remove_cookie(self: &Browser, cookie: &QString);
        #[qinvokable]
        #[cxx_name = "removeSiteCookies"]
        fn remove_site_cookies(self: &Browser, site: &QString);
        #[qinvokable]
        #[cxx_name = "removeAllCookies"]
        fn remove_all_cookies(self: &Browser);
        // Certificate exceptions (kite_core::certs).
        // Private windows in purple (kite_core::privatecolors).
        #[qinvokable]
        #[cxx_name = "privateWindowColors"]
        fn private_window_colors(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "colorSchemeForNewWindows"]
        fn color_scheme_for_new_windows(self: &Browser, purple: bool);
        #[qinvokable]
        #[cxx_name = "certFingerprint"]
        fn cert_fingerprint(self: &Browser, error: &QVariant) -> QString;
        #[qinvokable]
        #[cxx_name = "certAccepted"]
        fn cert_accepted(self: &Browser, host: &QString, fingerprint: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "addCertException"]
        fn add_cert_exception(self: &Browser, host: &QString, fingerprint: &QString, description: &QString);
        /// A site as typed, accepted with any certificate; false if it isn't a site.
        #[qinvokable]
        #[cxx_name = "addCertExceptionForSite"]
        fn add_cert_exception_for_site(self: &Browser, site: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "removeCertException"]
        fn remove_cert_exception(self: &Browser, host: &QString, fingerprint: &QString);
        #[qinvokable]
        #[cxx_name = "certExceptions"]
        fn cert_exceptions(self: &Browser) -> QString;
        /// For Google's sign-in: what its pages are told ("web.googleSignIn").
        #[qinvokable]
        #[cxx_name = "googleSignInScript"]
        fn google_sign_in_script(self: &Browser) -> QString;
        /// Wayland: see kite_core::kwin.
        #[qinvokable]
        #[cxx_name = "placePipWindow"]
        fn place_pip_window(self: &Browser, title: &QString, min_width: i32, all_desktops: bool);
        // Settings file (Settings → General). Export answers with a message;
        // preview with what's in a file, or {error}; import with a message.
        #[qinvokable]
        #[cxx_name = "exportSettings"]
        fn export_settings(self: &Browser, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "previewSettings"]
        fn preview_settings(self: &Browser, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "importSettings"]
        fn import_settings(self: Pin<&mut Browser>, path: &QString) -> QString;
        // Bookmarks of other browsers on this machine.
        #[qinvokable]
        #[cxx_name = "bookmarkSources"]
        fn bookmark_sources(self: &Browser) -> QString;
        #[qinvokable]
        #[cxx_name = "importBookmarksFrom"]
        fn import_bookmarks_from(self: Pin<&mut Browser>, source: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "selectionUrl"]
        fn selection_url(self: &Browser, text: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "copyText"]
        fn copy_text(self: &Browser, text: &QString);
        #[qinvokable]
        #[cxx_name = "engineInfo"]
        fn engine_info(self: &Browser) -> QString;
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
        #[qinvokable]
        #[cxx_name = "displayUrl"]
        fn display_url(self: &Browser, url: &QString) -> QString;

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

/// The page script making pages see `ua` as their user agent (identity.js).
fn identity_script(ua: &str) -> QString {
    q(include_str!("js/identity.js").replace("__KITE_UA__", &Value::String(ua.to_owned()).to_string()))
}
fn parse(s: &QString) -> Value {
    serde_json::from_str(&s.to_string()).unwrap_or(Value::Null)
}

/// Turn location-bar text into a URL to load (used by the start page too).
/// The tabs using MIDI devices.
fn midi_sessions() -> std::sync::MutexGuard<'static, std::collections::HashMap<i32, kite_core::midi::Session>> {
    static SESSIONS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<i32, kite_core::midi::Session>>> = std::sync::OnceLock::new();
    SESSIONS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

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

/// Bookmarks from another browser's profile (`source`: a browser_import::Source),
/// into a new folder in the Bookmarks Menu; what's there already stays out, so
/// that importing again only brings what's new. Says how it went.
fn import_bookmarks(source: &Value) -> String {
    use kite_core::browser_import as import;
    let result = (|| -> anyhow::Result<String> {
        let source: import::Source = serde_json::from_value(source.clone())?;
        let nodes = import::read(&source)?;
        let mut b = app().shared.bookmarks();
        let mut known = std::collections::HashSet::new();
        import::urls_in(&b.root.children, &mut known);
        let (nodes, new, already) = import::select(nodes, &known);
        let also = if already > 0 { format!(" ({already} you already had stayed out)") } else { String::new() };
        if new == 0 {
            return Ok(format!("Nothing new in {}{also}.", source.label()));
        }
        let name = format!("Imported from {}", source.label());
        let mut folder = Node::folder(&name);
        folder.children = nodes;
        let menu = b.menu_id();
        b.insert(menu, None, folder)?;
        let what = if new == 1 { "1 bookmark".to_owned() } else { format!("{new} bookmarks") };
        Ok(format!("Imported {what} into \"{name}\" in the Bookmarks Menu{also}."))
    })();
    result.unwrap_or_else(|e| format!("Import failed: {e}"))
}

/// The browsers found to import bookmarks from: [{label, source}].
pub fn import_sources() -> Value {
    let Some(dirs) = directories::BaseDirs::new() else { return json!([]) };
    let sources = kite_core::browser_import::detect(dirs.home_dir(), dirs.config_dir());
    json!(sources.iter().map(|s| json!({"label": s.label(), "source": s})).collect::<Vec<_>>())
}

fn local_folder(url: &QString) -> Option<std::path::PathBuf> {
    url::Url::parse(&url.to_string()).ok()?.to_file_path().ok()
}

fn report_adblock(thread: cxx_qt::CxxQtThread<qobject::Browser>, built: anyhow::Result<usize>) {
    let msg = match built {
        Ok(n) => format!("Content blocker ready ({n} filter lines)"),
        Err(e) => format!("Content blocker error: {e}"),
    };
    let _ = thread.queue(move |o| o.adblock_status(q(msg)));
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
        let needle = text.trim().to_lowercase();
        let rank = |url: &str| kite_core::history::match_rank(url, &text);
        // What fits the address best first, bookmarks before the history among equals.
        // (A bookmark's keyword is the address it stands for.)
        let mut bookmarks: Vec<(u8, String, String)> = a.shared.bookmarks().search(&text).into_iter()
            .map(|b| (if b.keyword.to_lowercase() == needle { 0 } else { rank(&b.url) }, b.url.clone(), b.name.clone()))
            .collect();
        bookmarks.sort_by_key(|b| b.0);
        bookmarks.truncate(4);
        let mut found: Vec<(u8, u8, String, String)> = bookmarks.into_iter().map(|(r, url, title)| (r, 0, url, title)).collect();
        let from_history = !in_private || a.shared.settings().get_bool("privacy.privateHistorySuggestions");
        let history = if from_history { kite_core::history::complete(&a.shared.db(), &text, 10).unwrap_or_default() } else { vec![] };
        found.extend(history.into_iter().map(|h| (rank(&h.url), 1, h.url, h.title)));
        found.sort_by_key(|f| (f.0, f.1));
        let mut seen = std::collections::HashSet::new();
        let out: Vec<Value> = found.into_iter()
            .filter(|f| seen.insert(f.2.clone()))
            .take(10)
            .map(|(_, source, url, title)| json!({"type": if source == 0 { "bookmark" } else { "history" }, "url": url, "title": title}))
            .collect();
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
        let query = query.to_string();
        // Searching is looking for a page; without it, it's what happened, in order.
        let find = if query.trim().is_empty() { kite_core::history::search } else { kite_core::history::search_ranked };
        let list = find(&app().shared.db(), &query, start as i64, end as i64, limit.max(1) as usize).unwrap_or_default();
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
        let path = kite_core::util::local_path(&path.to_string());
        let result = (|| -> anyhow::Result<usize> {
            let text = std::fs::read_to_string(&path)?;
            let mut folder = Node::folder("Imported");
            if text.trim_start().starts_with('{') {
                let other = kite_core::bookmarks::Bookmarks::load(path.clone());
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
        let path = kite_core::util::local_path(&path.to_string()).to_string_lossy().into_owned();
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
    fn start_external_download(&self, url: &QString, file_name: &QString) -> QString {
        let (program, arguments) = {
            let s = app().shared.settings();
            (s.get_str("downloads.externalProgram"), s.get_str("downloads.externalArguments"))
        };
        js(&match kite_core::external::start_download_manager(&program, &arguments, &url.to_string(), &file_name.to_string()) {
            Ok(name) => json!({"name": name}),
            Err(e) => {
                log::warn!("external download manager: {e}");
                json!({"error": e})
            }
        })
    }
    fn crash_report(&self) -> QString {
        q(crate::crash::report().unwrap_or_default())
    }
    fn take_new_crash_report(&self) -> bool {
        crate::crash::take_new_report()
    }
    fn log_folder_url(&self) -> QString {
        q(crate::crash::dir().and_then(|d| url::Url::from_directory_path(d).ok()).map(|u| u.to_string()).unwrap_or_default())
    }
    fn check_crash_handler(&self) {
        crate::crash::install_signal_handlers();
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
        let db = a.shared.db();
        let _ = kite_core::sitesettings::set(&db, &host.to_string(), &key.to_string(), value as i64);
        // (A site left with nothing of its own isn't one with settings.)
        let _ = kite_core::sitesettings::clear_if_empty(&db, &host.to_string());
        drop(db);
        app().privacy.load_site_rules(&app().shared);
    }
    fn all_site_settings(&self) -> QString {
        let list = kite_core::sitesettings::all(&app().shared.db()).unwrap_or_default();
        js(&Value::Array(list.into_iter().map(|(h, s)| json!({"host": h, "settings": s})).collect()))
    }
    fn clear_site_settings(self: Pin<&mut Self>, host: &QString) {
        let _ = kite_core::sitesettings::clear(&app().shared.db(), &host.to_string());
        app().privacy.load_site_rules(&app().shared);
    }
    fn https_upgrade_failed(&self, url: &QString) -> bool {
        let url = url.to_string();
        url.starts_with("https:") && app().privacy.was_upgraded(&url)
    }
    fn fingerprint_script(&self, seed: u32) -> QString {
        q(&include_str!("js/fingerprint.js").replace("__KITE_SEED__", &seed.to_string()))
    }
    fn force_light_script(&self) -> QString {
        q(include_str!("js/force_light.js"))
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
    fn allow_from_warning(&self, link: &QString) -> QString {
        let link = link.to_string();
        let Some(rest) = link.strip_prefix("kite:") else { return q("") };
        let (page, query) = rest.split_once('?').unwrap_or((rest, ""));
        let param = |name: &str| url::form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == name).map(|(_, v)| v.into_owned()).unwrap_or_default();
        if param("key") != crate::state::warning_key() {
            return q("");
        }
        let target = param("url");
        let Ok(u) = url::Url::parse(&target) else { return q("") };
        let Some(host) = u.host_str() else { return q("") };
        match (page, u.scheme()) {
            ("dangerous-allow", "http" | "https") => app().safe_browsing.allow(host),
            ("https-allow", "http") => app().privacy.allow_http(host),
            _ => return q(""),
        }
        q(&target)
    }
    fn safe_browsing_status(&self) -> QString {
        q(&app().safe_browsing.status())
    }
    fn debug_mode(&self) -> bool {
        crate::state::DEBUG.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn safe_browsing_entries(&self, filter: &QString, offset: i32, limit: i32) -> QString {
        if !self.debug_mode() {
            return q("{}");
        }
        let (total, page) = app().safe_browsing.debug_entries(&filter.to_string(), offset.max(0) as usize, limit.clamp(0, 5000) as usize);
        js(&json!({"total": total, "entries": page.into_iter().map(|(list, entry, threat)| json!({"list": list, "entry": entry, "threat": threat})).collect::<Vec<_>>()}))
    }
    fn safe_browsing_explain(&self, url: &QString) -> QString {
        if !self.debug_mode() {
            return q("{}");
        }
        js(&app().safe_browsing.debug_explain(&url.to_string()))
    }
    fn set_site_text(self: Pin<&mut Self>, host: &QString, key: &QString, value: &QString) {
        {
            let db = app().shared.db();
            let _ = kite_core::sitesettings::set_text(&db, &host.to_string(), &key.to_string(), &value.to_string());
            let _ = kite_core::sitesettings::clear_if_empty(&db, &host.to_string());
        }
        app().privacy.load_site_rules(&app().shared);
    }
    fn user_agent_presets(&self) -> QString {
        let mut list = vec![json!({"name": "Kite, naming QtWebEngine", "userAgent": kite_core::util::QTWEBENGINE_USER_AGENT})];
        list.extend(kite_core::util::USER_AGENT_PRESETS.iter().map(|(name, ua)| json!({"name": name, "userAgent": ua})));
        js(&Value::Array(list))
    }
    fn set_qtwebengine_user_agent(&self, ua: &QString) {
        *app().privacy.qtwebengine_user_agent.write().unwrap() = ua.to_string();
    }
    fn spell_dictionaries(&self) -> QString {
        js(&json!(kite_core::util::spell_dictionary_dir().map(|d| kite_core::util::spell_dictionaries(&d)).unwrap_or_default()))
    }
    fn resolve_spell_languages(&self, languages: &QString) -> QString {
        let wanted: Vec<String> = serde_json::from_str(&languages.to_string()).unwrap_or_default();
        let available = kite_core::util::spell_dictionary_dir().map(|d| kite_core::util::spell_dictionaries(&d)).unwrap_or_default();
        js(&json!(kite_core::util::resolve_spell_languages(&wanted, &available)))
    }
    fn spell_check_languages(&self) -> QString {
        let wanted: Vec<String> = serde_json::from_value(app().shared.settings().get("web.spellcheckLanguages")).unwrap_or_default();
        self.resolve_spell_languages(&q(serde_json::to_string(&wanted).unwrap_or_default()))
    }
    fn global_identity_script(&self) -> QString {
        crate::ffi::global_user_agent().map(|ua| identity_script(&ua)).unwrap_or_default()
    }
    fn site_identity_script(&self, host: &QString) -> QString {
        match app().privacy.site_user_agent(&host.to_string()) {
            Some(ua) => identity_script(&ua),
            None => QString::default(),
        }
    }
    fn reset_site_settings(self: Pin<&mut Self>, host: &QString) {
        let _ = kite_core::sitesettings::reset(&app().shared.db(), &host.to_string());
        app().privacy.load_site_rules(&app().shared);
    }

    // ---- content blocking ---------------------------------------------------
    fn reload_adblock(self: Pin<&mut Self>, download: bool) {
        let thread = self.qt_thread();
        app().reload_adblock(download, move |r| report_adblock(thread, r));
    }
    fn folder_file_count(&self, folder: &QString) -> i32 {
        local_folder(folder).map_or(0, |f| kite_core::util::count_files(&f, 10000) as i32)
    }
    fn local_path(&self, url: &QString) -> QString {
        q(local_folder(url).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
    }
    fn folder_exists(&self, folder: &QString) -> bool {
        local_folder(folder).is_some_and(|f| f.is_dir())
    }
    fn file_filters(&self, accepted: &QString) -> QString {
        q(crate::ffi::file_name_filters(&accepted.to_string()))
    }
    fn refresh_adblock(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        app().refresh_adblock(move |r| report_adblock(thread, r));
    }
    fn adblock_blocked_on(&self, host: &QString) -> i32 {
        app().adblock.blocked_on(&host.to_string()) as i32
    }
    fn adblock_blocked_list(&self, host: &QString) -> QString {
        js(&json!(app().adblock.blocked_list(&host.to_string())))
    }
    fn adblock_clear_blocked(&self, host: &QString) {
        app().adblock.clear_blocked(&host.to_string());
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
    fn pointer_lock_script(&self) -> QString {
        q(include_str!("js/pointerlock.js"))
    }
    fn context_menu_script(&self, unblock: bool) -> QString {
        q(&include_str!("js/context_menu.js").replace("__KITE_FRAME_SECRET__", &FRAME_SECRET).replace("__KITE_AUTO__", if unblock { "true" } else { "false" }))
    }
    fn replay_context_click(&self) {
        crate::ffi::replay_context_click();
    }
    fn text_unblock_script(&self) -> QString {
        q(include_str!("js/text_unblock.js"))
    }
    fn pointer_lock_relay_script(&self) -> QString {
        q(&include_str!("js/pointerlock_relay.js").replace("__KITE_FRAME_SECRET__", &FRAME_SECRET))
    }
    fn third_party_storage_script(&self) -> QString {
        q(include_str!("js/third_party_storage.js"))
    }

    fn scrollbar_script(&self) -> QString {
        // Whether Chromium was started with overlay scrollbars (main.rs, or
        // the user's own flags): the setting may have changed since.
        let on = std::env::var("QTWEBENGINE_CHROMIUM_FLAGS").is_ok_and(|f| f.contains("FluentOverlayScrollbar"));
        q(if on { include_str!("js/scrollbars.js") } else { "" })
    }

    fn named_sessions(&self) -> QString {
        js(&json!(named_sessions().list()))
    }

    fn named_session(&self, name: &QString) -> QString {
        js(&named_sessions().get(&name.to_string()).map_or(json!([]), |s| json!(s.windows)))
    }

    fn save_named_session(&self, name: &QString, windows: &QString) -> QString {
        // (A Kite started with --private keeps nothing.)
        if app().private {
            return q("A private Kite doesn't keep sessions.");
        }
        let result = serde_json::from_str(&windows.to_string()).map_err(anyhow::Error::from).and_then(|w| named_sessions().save(&name.to_string(), w, kite_core::util::now_ms()));
        q(result.err().map(|e| e.to_string()).unwrap_or_default())
    }

    fn rename_named_session(&self, from: &QString, to: &QString) -> QString {
        q(named_sessions().rename(&from.to_string(), &to.to_string()).err().map(|e| e.to_string()).unwrap_or_default())
    }

    fn delete_named_session(&self, name: &QString) -> QString {
        q(named_sessions().delete(&name.to_string()).err().map(|e| e.to_string()).unwrap_or_default())
    }

    fn is_external_url(&self, url: &QString) -> bool {
        search::is_external_url(&url.to_string())
    }

    fn frame_forms_script(&self) -> QString {
        q(kite_core::webext::scripts::frame_forms_js(&FRAME_SECRET))
    }

    fn frame_secret(&self) -> QString {
        q(&*FRAME_SECRET)
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

    fn profiles(&self) -> QString {
        js(&json!(kite_core::profiles::Profiles::open().map(|p| p.list()).unwrap_or_default()))
    }

    fn current_profile(&self) -> QString {
        q(&app().shared.profile.name)
    }

    fn open_profile(&self, name: &QString) {
        if let Some(exe) = crate::own_executable() {
            let _ = std::process::Command::new(exe).args(["--profile", &name.to_string()]).spawn();
        }
    }

    fn manage_profiles(&self) {
        if let Some(exe) = crate::own_executable() {
            let _ = std::process::Command::new(exe).arg("--pick-profile-open").spawn();
        }
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

    fn save_favicon(&self, page_url: &QString, icon_url: &QString, view_name: &QString) {
        crate::ffi::save_favicon(&page_url.to_string(), &icon_url.to_string(), &view_name.to_string());
    }

    fn save_fetched_favicon(&self, page_url: &QString, data_url: &QString) {
        use base64::Engine;
        let data_url = data_url.to_string();
        let Some(b64) = data_url.strip_prefix("data:image/png;base64,") else { return };
        // (Small: it was scaled to 64 px in the page.)
        if b64.len() > 512 * 1024 {
            return;
        }
        let Ok(png) = base64::engine::general_purpose::STANDARD.decode(b64) else { return };
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return;
        }
        if let Err(e) = kite_core::favicons::save(&app().shared.db(), &page_url.to_string(), &png) {
            log::warn!("saving favicon for {page_url}: {e}");
        }
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

    // ---- Plasma Integration (plugins/plasma-integration) --------------------
    fn plasma_mpris(&self) -> &'static kite_core::plasma::mpris::Mpris {
        static MPRIS: std::sync::OnceLock<kite_core::plasma::mpris::Mpris> = std::sync::OnceLock::new();
        MPRIS.get_or_init(|| {
            let thread = self.qt_thread();
            kite_core::plasma::mpris::Mpris::start(move |command| {
                let _ = thread.queue(move |o| o.plasma_command(js(&json!(command))));
            })
        })
    }
    fn plasma_media(&self, tab_id: i32, frame: &QString, state: &QString) {
        // (From the page: anything that doesn't fit counts as nothing being there.)
        let report = serde_json::from_str(&state.to_string()).unwrap_or(None);
        self.plasma_mpris().report(tab_id, frame.to_string(), report);
    }
    fn plasma_forget_tab(&self, tab_id: i32) {
        self.plasma_mpris().forget_tab(tab_id);
    }
    fn plasma_jobs(&self) -> &'static kite_core::plasma::jobs::Jobs {
        static JOBS: std::sync::OnceLock<kite_core::plasma::jobs::Jobs> = std::sync::OnceLock::new();
        JOBS.get_or_init(|| {
            let thread = self.qt_thread();
            kite_core::plasma::jobs::Jobs::start(move |event| {
                let _ = thread.queue(move |o| o.plasma_job_event(js(&json!(event))));
            })
        })
    }
    fn plasma_job_started(&self, id: i32, path: &QString, url: &QString) {
        self.plasma_jobs().started(id, path.to_string(), url.to_string());
    }
    fn plasma_job_progress(&self, id: i32, received: f64, total: f64) {
        self.plasma_jobs().progress(id, received as i64, total as i64);
    }
    fn plasma_job_paused(&self, id: i32, paused: bool) {
        self.plasma_jobs().paused(id, paused);
    }
    fn plasma_job_finished(&self, id: i32, state: &QString, received: f64, error: &QString) {
        use kite_core::plasma::jobs::End;
        let end = match state.to_string().as_str() {
            "complete" => End::Complete,
            "cancelled" => End::Cancelled,
            _ => End::Failed,
        };
        self.plasma_jobs().finished(id, end, received as i64, error.to_string());
    }
    fn plasma_jobs_forget(&self) {
        self.plasma_jobs().forget_all();
    }
    fn plasma_runner(&self) -> &'static kite_core::plasma::krunner::Runner {
        use kite_core::plasma::krunner::{Browser as RunnerBrowser, Picked, Runner};
        static RUNNER: std::sync::OnceLock<Runner> = std::sync::OnceLock::new();
        struct Link(cxx_qt::CxxQtThread<qobject::Browser>);
        // (Only ever queueing onto the interface's thread.)
        impl RunnerBrowser for Link {
            fn wants_tabs(&self) {
                let _ = self.0.queue(|o| o.plasma_runner_wants_tabs());
            }
            fn picked(&self, picked: Picked) {
                let _ = self.0.queue(move |o| o.plasma_runner_picked(js(&json!(picked))));
            }
        }
        RUNNER.get_or_init(|| Runner::start(app().shared.clone(), Link(self.qt_thread())))
    }
    fn plasma_runner_configure(&self, config: &QString) {
        // (From the plugin's own QML: it always fits.)
        self.plasma_runner().configure(serde_json::from_str(&config.to_string()).unwrap_or_default());
    }
    fn plasma_runner_answer_tabs(&self, tabs: &QString) {
        self.plasma_runner().answer_tabs(serde_json::from_str(&tabs.to_string()).unwrap_or_default());
    }
    fn listen_for_starts(&self) {
        let Some(instance) = crate::state::INSTANCE.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            instance.serve(move |r| {
                let _ = qt.queue(move |o| {
                    let urls: Vec<String> = r.urls.iter().map(|u| resolve_input(u)).filter(|u| !u.is_empty()).collect();
                    o.started_again(js(&json!({"urls": urls, "newWindow": r.urls.is_empty(), "token": r.token})));
                });
            });
        });
    }
    fn midi_start(&self, tab_id: i32, site: &QString, sysex: bool) -> QString {
        let qt = self.qt_thread();
        let on_event = move |event: kite_core::midi::Event| {
            let _ = qt.queue(move |o| o.midi_event(tab_id, js(&json!(event))));
        };
        match kite_core::midi::Session::open(&site.to_string(), sysex, on_event) {
            Ok((session, ports)) => {
                midi_sessions().insert(tab_id, session);
                js(&json!(ports))
            }
            Err(e) => {
                log::info!("Web MIDI: {e}");
                QString::default()
            }
        }
    }
    fn midi_command(&self, tab_id: i32, command: &QString) {
        let sessions = midi_sessions();
        let Some(session) = sessions.get(&tab_id) else { return };
        let Ok(command) = serde_json::from_str::<Value>(&command.to_string()) else { return };
        let id = command["id"].as_str().unwrap_or_default();
        match command["op"].as_str() {
            Some("open") => session.open_port(id),
            Some("close") => session.close_port(id),
            Some("allowSysex") => session.allow_sysex(),
            Some("send") => {
                let Some(data) = command["data"].as_array() else { return };
                let bytes: Option<Vec<u8>> = data.iter().map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok())).collect();
                if let Some(bytes) = bytes {
                    session.send(id, bytes);
                }
            }
            _ => {}
        }
    }
    fn midi_stop(&self, tab_id: i32) {
        midi_sessions().remove(&tab_id);
    }
    fn web_midi_script(&self) -> QString {
        q(include_str!("js/webmidi.js"))
    }
    fn use_activation_token(&self, token: &QString) {
        // Qt's Wayland code takes it from there when a window asks to be activated.
        if !token.is_empty() {
            std::env::set_var("XDG_ACTIVATION_TOKEN", token.to_string());
        }
    }
    fn plasma_devices(&self) -> QString {
        js(&json!(*PLASMA_DEVICES.lock().unwrap_or_else(|e| e.into_inner())))
    }
    fn plasma_refresh_devices(&self) {
        use std::sync::atomic::{AtomicBool, Ordering};
        // Only one at a time: a stuck daemon shouldn't collect threads.
        static ASKING: AtomicBool = AtomicBool::new(false);
        if ASKING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(|| {
            let devices = kite_core::plasma::kdeconnect::devices().unwrap_or_else(|e| {
                log::debug!("KDE Connect: {e}");
                Vec::new()
            });
            *PLASMA_DEVICES.lock().unwrap_or_else(|e| e.into_inner()) = devices;
            ASKING.store(false, Ordering::SeqCst);
        });
    }
    fn plasma_can_send(&self, url: &QString) -> bool {
        kite_core::plasma::kdeconnect::can_send(&url.to_string())
    }
    fn plasma_share(&self, device: &QString, url: &QString) {
        let (device, url) = (device.to_string(), url.to_string());
        std::thread::spawn(move || {
            if let Err(e) = kite_core::plasma::kdeconnect::share(&device, &url) {
                log::warn!("KDE Connect: {e}");
            }
        });
    }

    fn plugins(&self) -> QString {
        q(serde_json::to_string(&all_plugins()).unwrap_or_else(|_| "[]".into()))
    }

    fn plugin_file(&self, path: &QString) -> QString {
        let path = path.to_string();
        if let Some((_, text)) = PLUGIN_FILES.iter().find(|(p, _)| *p == path) {
            return q(*text);
        }
        let Some(dir) = installed_plugins_dir() else { return q("") };
        q(kite_core::plugins::read_installed_file(&dir, &path).unwrap_or_else(|e| {
            log::warn!("plugin: {e}");
            String::new()
        }))
    }

    fn page_load_redirect(&self, url: &QString, in_private: bool) -> QString {
        q(crate::ffi::page_load_redirect(&url.to_string(), in_private))
    }

    fn config_command(mut self: Pin<&mut Self>, message: &QString) -> QString {
        let m: Value = serde_json::from_str(&message.to_string()).unwrap_or(Value::Null);
        // Only that page has this.
        if m["token"].as_str() != Some(crate::state::config_token()) {
            log::warn!("kite:config: a change without its token");
            return q("null");
        }
        let key = m["key"].as_str().unwrap_or("").to_owned();
        let Some(before) = config_entries().into_iter().find(|e| e.key == key) else {
            return js(&json!({"key": key, "error": "There is no such setting."}));
        };
        let a = app();
        match m["op"].as_str() {
            Some("set") => match kite_core::config::parse_value(before.kind, m["value"].as_str().unwrap_or("")) {
                Ok(value) => {
                    if let Err(e) = a.shared.settings().set(&key, value) {
                        return js(&json!({"key": key, "error": e.to_string()}));
                    }
                }
                Err(error) => return js(&json!({"key": key, "error": error})),
            },
            Some("reset") => {
                if let Err(e) = a.shared.settings().reset(&key) {
                    return js(&json!({"key": key, "error": e.to_string()}));
                }
            }
            _ => return q("null"),
        }
        let value = a.shared.settings().get(&key);
        apply_setting(&key, &value);
        self.as_mut().setting_changed(q(&key), js(&value));
        js(&json!({"key": key, "entry": config_entries().into_iter().find(|e| e.key == key)}))
    }

    fn startup_notice(&self) -> QString {
        q(std::mem::take(&mut *crate::state::STARTUP_NOTICE.lock().unwrap_or_else(|e| e.into_inner())))
    }

    fn plugins_disabled(&self) -> bool {
        crate::state::NO_PLUGINS.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn inspect_plugin(&self, path: &QString) -> QString {
        let path = kite_core::util::local_path(&path.to_string());
        let inspected = (|| -> anyhow::Result<(kite_core::plugins::Plugin, Vec<u8>)> {
            anyhow::ensure!(std::fs::metadata(&path)?.len() <= kite_core::plugins::MAX_PACKAGE, "the file is too large for a plugin");
            let bytes = std::fs::read(&path)?;
            Ok((kite_core::plugins::inspect(&bytes, &builtin_plugin_ids())?, bytes))
        })();
        let mut pending = PENDING_PLUGIN.lock().unwrap_or_else(|e| e.into_inner());
        *pending = None;
        match inspected {
            Ok((plugin, bytes)) => {
                *pending = Some(bytes);
                let update = installed_plugins().iter().any(|p| p.id == plugin.id);
                js(&json!({"plugin": plugin, "update": update}))
            }
            Err(e) => js(&json!({"error": e.to_string()})),
        }
    }

    fn install_inspected_plugin(&self) -> QString {
        let bytes = PENDING_PLUGIN.lock().unwrap_or_else(|e| e.into_inner()).take();
        let (Some(bytes), Some(dir)) = (bytes, installed_plugins_dir()) else { return q("There is no plugin file to install") };
        match kite_core::plugins::install(&bytes, &builtin_plugin_ids(), &dir) {
            Ok(_) => QString::default(),
            Err(e) => q(e.to_string()),
        }
    }

    fn remove_plugin(&self, id: &QString) -> QString {
        let id = id.to_string();
        let Some(dir) = installed_plugins_dir() else { return q("There is no plugins folder") };
        match kite_core::plugins::remove(&dir, &id) {
            Ok(()) => {
                // Its being on doesn't outlive it: installed again, it starts off.
                let _ = app().shared.settings().remove_prefix(&format!("plugins.{id}."));
                QString::default()
            }
            Err(e) => q(e.to_string()),
        }
    }

    fn google_sign_in_script(&self) -> QString {
        identity_script(kite_core::util::FIREFOX_USER_AGENT)
    }

    fn place_pip_window(&self, title: &QString, min_width: i32, all_desktops: bool) {
        kite_core::kwin::place_pip_window(&title.to_string(), min_width, all_desktops);
    }

    fn cookies_by_site(&self) -> QString {
        let cookies = app().shared.cookies().clone();
        let sites = kite_core::shared::cookies_by_site(&cookies, &crate::ffi::site_of);
        js(&json!(sites.iter().map(|(site, cookies)| json!({"site": site, "cookies": cookies})).collect::<Vec<_>>()))
    }

    fn remove_cookie(&self, cookie: &QString) {
        crate::ffi::delete_cookie(&cookie.to_string());
    }

    fn remove_site_cookies(&self, site: &QString) {
        crate::ffi::delete_cookies_of(&[site.to_string()]);
    }

    fn remove_all_cookies(&self) {
        crate::ffi::delete_all_cookies();
    }

    fn private_window_colors(&self) -> QString {
        q(kite_core::privatecolors::colors_json())
    }

    fn color_scheme_for_new_windows(&self, purple: bool) {
        let path = if purple {
            match kite_core::privatecolors::scheme_path() {
                Ok(p) => p.to_string_lossy().into_owned(),
                Err(e) => {
                    log::warn!("private window color scheme: {e}");
                    String::new()
                }
            }
        } else {
            String::new()
        };
        crate::ffi::set_color_scheme_for_new_windows(&path);
    }

    fn cert_fingerprint(&self, error: &cxx_qt_lib::QVariant) -> QString {
        q(crate::ffi::cert_fingerprint(error as *const cxx_qt_lib::QVariant as *const std::ffi::c_void))
    }

    fn cert_accepted(&self, host: &QString, fingerprint: &QString) -> bool {
        kite_core::certs::accepted(&app().shared.db(), &host.to_string(), &fingerprint.to_string())
    }

    fn add_cert_exception(&self, host: &QString, fingerprint: &QString, description: &QString) {
        // (Without a fingerprint this would take any certificate: that's asked for
        // by name, below.)
        if fingerprint.to_string().is_empty() {
            return;
        }
        let now = kite_core::util::now_ms() / 1000;
        let _ = kite_core::certs::add(&app().shared.db(), &host.to_string(), &fingerprint.to_string(), &description.to_string(), now);
    }

    fn add_cert_exception_for_site(&self, site: &QString) -> bool {
        let Some(host) = kite_core::certs::host_of_input(&site.to_string()) else { return false };
        let now = kite_core::util::now_ms() / 1000;
        kite_core::certs::add(&app().shared.db(), &host, "", "", now).is_ok()
    }

    fn remove_cert_exception(&self, host: &QString, fingerprint: &QString) {
        let _ = kite_core::certs::remove(&app().shared.db(), &host.to_string(), &fingerprint.to_string());
    }

    fn cert_exceptions(&self) -> QString {
        js(&json!(kite_core::certs::list(&app().shared.db()).unwrap_or_default()))
    }

    fn export_settings(&self, path: &QString) -> QString {
        let path = kite_core::util::local_path(&path.to_string());
        let a = app();
        let result = (|| -> anyhow::Result<()> {
            let backup = kite_core::backup::export(&a.shared.settings(), &a.shared.db())?;
            std::fs::write(&path, serde_json::to_string_pretty(&backup)?)?;
            Ok(())
        })();
        q(match result {
            Ok(()) => format!("Settings exported to {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        })
    }

    fn preview_settings(&self, path: &QString) -> QString {
        let path = kite_core::util::local_path(&path.to_string());
        let result = std::fs::read_to_string(&path).map_err(anyhow::Error::from).and_then(|t| kite_core::backup::parse(&t));
        js(&match result {
            Ok(b) => json!({
                "settings": b.settings.len(), "engines": b.search_engines.len(), "sites": b.site_settings.len(),
                // Scripts that would run in those sites.
                "scripts": b.site_settings.iter().filter(|s| !s.settings.js.is_empty()).map(|s| s.host.clone()).collect::<Vec<_>>(),
            }),
            Err(e) => json!({"error": e.to_string()}),
        })
    }

    fn import_settings(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let path = kite_core::util::local_path(&path.to_string());
        let a = app();
        let result = (|| -> anyhow::Result<Vec<(String, Value)>> {
            let backup = kite_core::backup::parse(&std::fs::read_to_string(&path)?)?;
            // A way back.
            let before = kite_core::backup::export(&a.shared.settings(), &a.shared.db())?;
            std::fs::write(a.shared.profile.path("settings-before-import.json"), serde_json::to_string_pretty(&before)?)?;
            kite_core::backup::apply_to_db(&a.shared.db(), &backup)?;
            let changes = kite_core::backup::changes(a.shared.settings().all(), &backup);
            for (key, value) in &changes {
                a.shared.settings().set(key, value.clone())?;
            }
            Ok(changes)
        })();
        match result {
            Ok(changes) => {
                for (key, value) in &changes {
                    apply_setting(key, value);
                    self.as_mut().setting_changed(q(key), js(value));
                }
                q(format!("Settings imported. The ones from before are in {}", a.shared.profile.path("settings-before-import.json").display()))
            }
            Err(e) => q(format!("Import failed: {e}")),
        }
    }

    fn bookmark_sources(&self) -> QString {
        js(&import_sources())
    }

    /// Into a new folder in the Bookmarks Menu; what's there already stays out, so
    /// that importing again only brings what's new.
    fn import_bookmarks_from(self: Pin<&mut Self>, source: &QString) -> QString {
        let message = import_bookmarks(&serde_json::from_str(&source.to_string()).unwrap_or(Value::Null));
        self.bookmarks_changed();
        q(message)
    }

    fn welcome_command(mut self: Pin<&mut Self>, message: &QString) -> QString {
        let m: Value = serde_json::from_str(&message.to_string()).unwrap_or(Value::Null);
        // Only that page has this.
        if m["token"].as_str() != Some(crate::state::welcome_token()) {
            log::warn!("kite:welcome: a message without its token");
            return q("null");
        }
        match m["op"].as_str() {
            Some("finish") => {
                let Some(level) = m["level"].as_str().and_then(kite_core::welcome::Level::parse) else { return q("null") };
                let imported = if m["source"].is_object() {
                    let message = import_bookmarks(&m["source"]);
                    self.as_mut().bookmarks_changed();
                    message
                } else {
                    String::new()
                };
                // The search engine chosen (one this profile has).
                if let Some(engine) = m["engine"].as_str() {
                    let known = search::load_engines(&app().shared.db()).unwrap_or_default().iter().any(|e| e.name == engine);
                    if known && app().shared.settings().get_str("general.searchEngine") != engine {
                        let value = json!(engine);
                        let _ = app().shared.settings().set("general.searchEngine", value.clone());
                        apply_setting("general.searchEngine", &value);
                        self.as_mut().setting_changed(q("general.searchEngine"), js(&value));
                    }
                }
                self.as_mut().apply_welcome(level);
                js(&json!({"imported": imported}))
            }
            // Done showing "All done": on to the start page.
            Some("go") => js(&json!({"go": true})),
            _ => q("null"),
        }
    }

    fn welcome_pending(&self) -> bool {
        !app().private && app().shared.settings().get_str("general.welcome") == "pending"
    }

    fn welcome_abandon(mut self: Pin<&mut Self>) {
        if self.welcome_pending() {
            log::info!("welcome: browsing started first, so the default level it is");
            self.as_mut().apply_welcome(kite_core::welcome::Level::High);
        }
    }

    /// A privacy level's settings, applied as if each were changed in Settings,
    /// and the welcome page marked as done.
    fn apply_welcome(mut self: Pin<&mut Self>, level: kite_core::welcome::Level) {
        let a = app();
        let mut changed = vec![];
        for (key, value) in kite_core::welcome::settings_for(level) {
            if a.shared.settings().get(key) != value {
                let _ = a.shared.settings().set(key, value.clone());
                changed.push((key, value));
            }
        }
        // From High up, the search engines aren't told it's Kite searching.
        if kite_core::welcome::hides_kite_from_search(level) {
            let db = a.shared.db();
            if let Ok(engines) = search::load_engines(&db) {
                let plain: Vec<search::SearchEngine> = engines.iter().cloned().map(|mut e| { e.url = search::without_kite_params(&e.url); e }).collect();
                if plain != engines {
                    let _ = search::save_engines(&db, &plain);
                }
            }
        }
        let _ = a.shared.settings().set("general.welcome", json!("done"));
        for (key, value) in changed {
            apply_setting(key, &value);
            self.as_mut().setting_changed(q(key), js(&value));
        }
    }

    fn selection_url(&self, text: &QString) -> QString {
        q(search::selection_url(&text.to_string()).unwrap_or_default())
    }

    fn copy_text(&self, text: &QString) {
        crate::ffi::copy_text(&text.to_string());
    }

    fn engine_info(&self) -> QString {
        q(crate::ffi::engine_info())
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
    fn adblock_page_plan(&self, url: &QString, in_private: bool) -> QString {
        // Cosmetic filters and scriptlets are the extension's job too.
        if app().runtime().content_blocker_for(in_private).is_some() {
            return QString::default();
        }
        match app().adblock.page_plan(&url.to_string()) {
            Some(plan) => js(&json!(plan)),
            None => QString::default(),
        }
    }
    fn adblock_generic(&self, found: &QString) -> QString {
        let Ok(found) = serde_json::from_str::<Value>(&found.to_string()) else { return q("[]") };
        let list = |key: &str| -> Vec<String> { found[key].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default() };
        js(&json!(app().adblock.generic_selectors(&list("c"), &list("i"), &list("x"))))
    }
    fn element_picker_script(&self) -> QString {
        q(include_str!("js/element_picker.js"))
    }
    fn adblock_content_script(&self) -> QString {
        q(include_str!("js/adblock_content.js"))
    }

    // ---- misc -----------------------------------------------------------------
    fn temp_file(&self, name: &QString) -> QString {
        match kite_core::util::private_temp_dir() {
            Ok(dir) => q(dir.join(name.to_string().replace('/', "_")).to_string_lossy()),
            Err(_) => q(""),
        }
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
    fn display_url(&self, url: &QString) -> QString {
        q(kite_core::util::display_url(&url.to_string()))
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

/// Every setting for kite:config: those with defaults, those stored, and what plugins
/// declare.
pub(crate) fn config_entries() -> Vec<kite_core::config::Entry> {
    let mut defaults = kite_core::settings::defaults();
    for p in all_plugins() {
        defaults.insert(format!("plugins.{}.enabled", p.id), Value::Bool(p.default_enabled));
        for (key, value) in p.settings {
            defaults.insert(format!("plugins.{}.{key}", p.id), value);
        }
    }
    let settings = app().shared.settings();
    let stored: serde_json::Map<String, Value> = settings.all().iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
    kite_core::config::entries(&stored, &defaults)
}

/// A plugin file being asked about, kept as it was read: what's installed is what
/// the user was shown, even if the file has changed since.
static PENDING_PLUGIN: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// Where plugins installed from a file are, for all profiles.
fn installed_plugins_dir() -> Option<std::path::PathBuf> {
    kite_core::paths::base_dir().ok().map(|d| d.join("plugins"))
}

fn builtin_plugin_ids() -> Vec<String> {
    builtin_plugins().into_iter().map(|p| p.id).collect()
}

/// The plugins installed from a file. Their manifests name "plugins/<id>/…", so
/// addresses of their files start from the folder above that.
fn installed_plugins() -> Vec<kite_core::plugins::Plugin> {
    let Some(dir) = installed_plugins_dir() else { return vec![] };
    let (mut list, broken) = kite_core::plugins::list_installed(&dir, &builtin_plugin_ids());
    for b in broken {
        log::warn!("plugin not loaded: {b}");
    }
    let base = dir.parent().and_then(|d| url::Url::from_directory_path(d).ok()).map(String::from).unwrap_or_default();
    for p in &mut list {
        p.base_url = base.clone();
    }
    list
}

fn all_plugins() -> Vec<kite_core::plugins::Plugin> {
    let mut all = builtin_plugins();
    all.extend(installed_plugins());
    all
}

/// Each plugin of crates/kite/plugins/. They are checked by a test below, so a
/// mistake can't get this far.
fn builtin_plugins() -> Vec<kite_core::plugins::Plugin> {
    let mut plugins = kite_core::plugins::parse_all(&[
        include_str!("../plugins/gestures/plugin.json"),
        include_str!("../plugins/vim-keys/plugin.json"),
        include_str!("../plugins/pip/plugin.json"),
        include_str!("../plugins/plasma-integration/plugin.json"),
        include_str!("../plugins/reader-view/plugin.json"),
        include_str!("../plugins/youtube-download/plugin.json"),
    ])
    .expect("plugin.json");
    for p in &mut plugins {
        p.base_url = "qrc:/qt/qml/org/kde/kite/".into();
    }
    plugins
}

/// The files of plugins' page scripts (see the same test).
static PLASMA_DEVICES: std::sync::Mutex<Vec<kite_core::plasma::kdeconnect::Device>> = std::sync::Mutex::new(Vec::new());

const PLUGIN_FILES: &[(&str, &str)] = &[
    ("plugins/vim-keys/keys.js", include_str!("../plugins/vim-keys/keys.js")),
    ("plugins/vim-keys/page.js", include_str!("../plugins/vim-keys/page.js")),
    ("plugins/pip/page.js", include_str!("../plugins/pip/page.js")),
    ("plugins/plasma-integration/PlasmaApp.qml", include_str!("../plugins/plasma-integration/PlasmaApp.qml")),
    ("plugins/plasma-integration/page-world.js", include_str!("../plugins/plasma-integration/page-world.js")),
    ("plugins/plasma-integration/page-share-world.js", include_str!("../plugins/plasma-integration/page-share-world.js")),
    ("plugins/plasma-integration/page-share.js", include_str!("../plugins/plasma-integration/page-share.js")),
    ("plugins/plasma-integration/page.js", include_str!("../plugins/plasma-integration/page.js")),
    // (Run in pages by its window item, when needed.)
    ("plugins/reader-view/readability/Readability.js", include_str!("../plugins/reader-view/readability/Readability.js")),
    ("plugins/reader-view/readability/Readability-readerable.js", include_str!("../plugins/reader-view/readability/Readability-readerable.js")),
    ("plugins/reader-view/extract.js", include_str!("../plugins/reader-view/extract.js")),
];

#[cfg(test)]
mod tests {
    #[test]
    fn plugins_are_valid_and_complete() {
        let plugins = super::builtin_plugins();
        assert!(!plugins.is_empty());
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for p in &plugins {
            for f in p.files() {
                assert!(here.join(f).is_file(), "{}: {f}", p.id);
            }
        }
        // Every page script's file can be had.
        for p in &plugins {
            for f in p.page_scripts.iter().flat_map(|s| s.files.iter()) {
                assert!(super::PLUGIN_FILES.iter().any(|(path, _)| path == f), "{}: {f} isn't in PLUGIN_FILES", p.id);
            }
        }
        // Reader view's defaults are the same in its manifest and where Kite reads them.
        let reader = plugins.iter().find(|p| p.id == "reader-view").unwrap();
        // (Nothing stored: loading a file that isn't there doesn't make it.)
        let none = std::env::temp_dir().join(format!("kite-test-{}-none/settings.json", std::process::id()));
        let prefs = kite_core::reader::Prefs::from_settings(&kite_core::settings::Settings::load(none));
        assert_eq!(reader.settings["fontSize"], prefs.font_size);
        assert_eq!(reader.settings["width"], prefs.width);
        assert_eq!(reader.settings["font"], prefs.font);
        assert_eq!(reader.settings["theme"], prefs.theme);
        // Every folder is on the list.
        let folders = std::fs::read_dir(here.join("plugins")).unwrap().count();
        assert_eq!(folders, plugins.len());
    }

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

fn named_sessions() -> kite_core::session::NamedSessions {
    kite_core::session::NamedSessions::load(app().shared.profile.path("sessions.json"))
}

static FRAME_SECRET: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    // (Random letters and digits: the script holds it in a string.)
    let mut bytes = [0u8; 24];
    let _ = getrandom::getrandom(&mut bytes);
    format!("kiteframe{}:", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
});

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
