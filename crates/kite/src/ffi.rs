//! C ABI between the C++ glue (scheme handlers, request interceptor, cookie
//! store) and Rust.

use crate::state::app;
use kite_core::webext::scripts;
use std::ffi::{c_char, CStr, CString};

#[repr(C)]
pub struct KiteBuf {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl KiteBuf {
    fn from_vec(v: Vec<u8>) -> Self {
        let mut v = std::mem::ManuallyDrop::new(v);
        KiteBuf { ptr: v.as_mut_ptr(), len: v.len(), cap: v.capacity() }
    }
    fn empty() -> Self {
        Self::from_vec(Vec::new())
    }
}

#[no_mangle]
pub extern "C" fn kite_buf_free(b: KiteBuf) {
    if !b.ptr.is_null() {
        unsafe { drop(Vec::from_raw_parts(b.ptr, b.len, b.cap)) };
    }
}

/// JSON list of a file dialog's name filters, from the JSON list of what a page's
/// file input accepts (`.pdf`, `image/*`, `text/plain`).
pub fn file_name_filters(accepted_json: &str) -> String {
    let Ok(json) = std::ffi::CString::new(accepted_json) else { return "[]".into() };
    unsafe { s(kite_cpp_file_name_filters(json.as_ptr())) }.to_owned()
}

unsafe fn s<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        CStr::from_ptr(p).to_str().unwrap_or("")
    }
}

extern "C" {
    fn kite_cpp_set_cookie(json: *const c_char);
    fn kite_cpp_delete_cookie(json: *const c_char);
    fn kite_cpp_delete_all_cookies();
    fn kite_cpp_clear_http_cache();
    fn kite_cpp_apply_dns(mode: std::ffi::c_int, server: *const c_char);
    fn kite_cpp_delete_cookies_of(hosts_json: *const c_char);
    fn kite_cpp_hook_container_profiles();
    fn kite_cpp_qwebchannel_js() -> *const c_char;
    fn kite_cpp_file_name_filters(accepted_json: *const c_char) -> *const c_char;
    fn kite_cpp_copy_text(text: *const c_char);
    fn kite_cpp_set_color_scheme_for_new_windows(path: *const c_char);
    fn kite_cpp_set_desktop_color_scheme(path: *const c_char);
    fn kite_cpp_site_of(host: *const c_char) -> *const c_char;
    fn kite_cpp_cert_fingerprint(error: *const std::ffi::c_void) -> *const c_char;
    fn kite_cpp_webengine_version() -> *const c_char;
    fn kite_cpp_chromium_version() -> *const c_char;
    fn kite_cpp_chromium_security_patch() -> *const c_char;
    fn kite_cpp_save_favicon(page_url: *const c_char, icon_url: *const c_char, view_name: *const c_char);
    fn kite_cpp_favicon_track(view_name: *const c_char, track: bool);
    fn kite_cpp_adopted_view(view_name: *const c_char);
    fn kite_cpp_key_text(key: std::ffi::c_int, modifiers: std::ffi::c_int) -> *const c_char;
    fn kite_cpp_key_native(sequence: *const c_char) -> *const c_char;
    fn kite_cpp_set_wheel_pixels(px: std::ffi::c_int);
    fn kite_cpp_replay_context_click();
    fn kite_cpp_system_wheel_pixels() -> std::ffi::c_int;
}

/// Cache the favicon a page is showing (`icon_url` = image://favicon/...), as
/// the web view `view_name` (its objectName) has it.
pub fn save_favicon(page_url: &str, icon_url: &str, view_name: &str) {
    let (Ok(p), Ok(i), Ok(v)) = (CString::new(page_url), CString::new(icon_url), CString::new(view_name)) else { return };
    unsafe { kite_cpp_save_favicon(p.as_ptr(), i.as_ptr(), v.as_ptr()) }
}

/// A web view (by objectName) was given a page that another page opened: the
/// engine's faulty helper for it is replaced (see bootstrap.cpp).
pub fn adopted_view(view_name: &str) {
    let Ok(n) = CString::new(view_name) else { return };
    unsafe { kite_cpp_adopted_view(n.as_ptr()) }
}

/// Take a web view (by objectName) out of Qt's favicon provider while it is
/// unloaded, or put it back (see bootstrap.cpp).
pub fn favicon_track(view_name: &str, track: bool) {
    let Ok(n) = CString::new(view_name) else { return };
    unsafe { kite_cpp_favicon_track(n.as_ptr(), track) }
}

/// A pressed key (Qt key code and modifiers) as a portable key sequence.
pub fn key_text(key: i32, modifiers: i32) -> String {
    unsafe { std::ffi::CStr::from_ptr(kite_cpp_key_text(key, modifiers)).to_string_lossy().into_owned() }
}

/// A portable key sequence as the desktop writes it (translated key names).
pub fn key_native(sequence: &str) -> String {
    let Ok(s) = CString::new(sequence) else { return sequence.to_owned() };
    unsafe { std::ffi::CStr::from_ptr(kite_cpp_key_native(s.as_ptr())).to_string_lossy().into_owned() }
}

/// Scroll distance per mouse wheel notch on pages, 0 for the system's.
/// The user's last right-click again, past the page (js/context_menu.js).
pub fn replay_context_click() {
    unsafe { kite_cpp_replay_context_click() }
}

pub fn set_wheel_pixels(px: i64) {
    unsafe { kite_cpp_set_wheel_pixels(px.clamp(0, 2000) as std::ffi::c_int) }
}

/// The system's scroll distance per wheel notch on pages (wheel scroll
/// lines × 20 px).
pub fn system_wheel_pixels() -> i32 {
    unsafe { kite_cpp_system_wheel_pixels() }
}

/// A favicon from `save_favicon`, as PNG.
#[no_mangle]
pub unsafe extern "C" fn kite_favicon_ready(page_url: *const c_char, png: *const u8, len: usize) {
    if png.is_null() || len == 0 {
        return;
    }
    let png = std::slice::from_raw_parts(png, len);
    if let Err(e) = kite_core::favicons::save(&app().shared.db(), s(page_url), png) {
        log::warn!("saving favicon for {}: {e}", s(page_url));
    }
}

pub fn set_cookie(json: &str) {
    let c = CString::new(json).unwrap_or_default();
    unsafe { kite_cpp_set_cookie(c.as_ptr()) }
}
pub fn delete_cookie(json: &str) {
    let c = CString::new(json).unwrap_or_default();
    unsafe { kite_cpp_delete_cookie(c.as_ptr()) }
}
pub fn delete_all_cookies() {
    unsafe { kite_cpp_delete_all_cookies() }
}
pub fn clear_http_cache() {
    unsafe { kite_cpp_clear_http_cache() }
}

pub fn site_of(host: &str) -> String {
    let Ok(host) = CString::new(host) else { return String::new() };
    unsafe { s(kite_cpp_site_of(host.as_ptr())).to_owned() }
}

/// The fingerprint of the certificate of a QWebEngineCertificateError held by a QVariant.
pub fn cert_fingerprint(error: *const std::ffi::c_void) -> String {
    unsafe { s(kite_cpp_cert_fingerprint(error)).to_owned() }
}

pub fn set_color_scheme_for_new_windows(path: &str) {
    let Ok(path) = CString::new(path) else { return };
    unsafe { kite_cpp_set_color_scheme_for_new_windows(path.as_ptr()) }
}

/// The desktop's colors changed inside a Flatpak (kite_core::desktopcolors):
/// the scheme file with the new ones. From any thread.
pub fn set_desktop_color_scheme(path: &std::path::Path) {
    let Ok(path) = CString::new(path.to_string_lossy().as_bytes()) else { return };
    unsafe { kite_cpp_set_desktop_color_scheme(path.as_ptr()) }
}

pub fn copy_text(text: &str) {
    let Ok(text) = CString::new(text) else { return };
    unsafe { kite_cpp_copy_text(text.as_ptr()) }
}

/// The engine in one line, for the log.
pub fn engine_version() -> String {
    unsafe { format!("QtWebEngine {} (Chromium {})", s(kite_cpp_webengine_version()), s(kite_cpp_chromium_version())) }
}

/// What the About box says about the engine.
pub fn engine_info() -> String {
    unsafe {
        format!(
            "Rendering engine: QtWebEngine {}\nBased on Chromium {}\nSecurity fixes up to Chromium {}",
            s(kite_cpp_webengine_version()),
            s(kite_cpp_chromium_version()),
            s(kite_cpp_chromium_security_patch()),
        )
    }
}

pub fn qwebchannel_js() -> &'static str {
    app().qwebchannel_js.get_or_init(|| unsafe { s(kite_cpp_qwebchannel_js()).to_owned() })
}

/// The `kite:certificate` page, from what the lookup in glue.cpp found.
#[no_mangle]
pub unsafe extern "C" fn kite_certificate_page(host: *const c_char, chain_json: *const c_char, error: *const c_char) -> KiteBuf {
    KiteBuf::from_vec(kite_core::certview::page_from_json(s(host), s(chain_json), s(error)).into_bytes())
}

/// Serve `moz-extension://` and `kite:` URLs. Returns the body; sets HTTP-like
/// status and mime type.
#[no_mangle]
pub unsafe extern "C" fn kite_serve(url: *const c_char, initiator: *const c_char, in_private: std::ffi::c_int, status: *mut i32, mime: *mut KiteBuf, cors: *mut std::ffi::c_int, alone: *mut std::ffi::c_int) -> KiteBuf {
    let (url, initiator) = (s(url), s(initiator));
    let (code, ty, body) = serve(url, initiator, in_private != 0);
    *status = code;
    *alone = (code == 200 && stands_alone(url)) as std::ffi::c_int;
    *mime = KiteBuf::from_vec(ty.into_bytes());
    *cors = (code == 200 && readable_cross_origin(url, initiator)) as std::ffi::c_int;
    KiteBuf::from_vec(body)
}

/// Whether a page of Kite's is served as an origin of its own (a sandbox, glue.cpp),
/// apart from Kite's other pages, which are all one origin with each other.
///
/// The pages that hold what changes things: `kite:config` and `kite:welcome`, with
/// their tokens. A script on any other `kite:` page (a start page somebody else wrote
/// and the user copied in, or a hole in one of Kite's own) could else open one of them
/// and read it. And the pages web content can lead to, the warnings: whatever got to
/// run there would not be one of Kite's pages to the rest.
///
/// None of them needs its origin: they keep nothing, load nothing, and talk to Kite
/// through the console.
fn stands_alone(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("kite:") else { return false };
    matches!(rest.split(['?', '#']).next().unwrap_or(""), "config" | "welcome" | "blocked" | "dangerous" | "https-only")
}

/// Whether other origins may read this response: only an extension's
/// web_accessible_resources (Firefox semantics). Everything else, `kite:`
/// pages included, stays opaque to them.
fn readable_cross_origin(url: &str, initiator: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else { return false };
    if u.scheme() != "moz-extension" {
        return false;
    }
    let Some(path) = extension_path(&u) else { return false };
    if path.starts_with("__kite__/") {
        return false;
    }
    let rt = app().runtime();
    rt.registry.by_uuid(u.host_str().unwrap_or("")).is_some_and(|ext| web_accessible(ext, &path, initiator))
}

/// The file an extension address is for: its path, decoded once. That text is what is
/// checked against what other sites may load, and what is then looked up. (Checked as
/// written and looked up decoded, "page.html%3F.png" passed for a picture and served
/// the page.) None if it can't be a file's path.
fn extension_path(u: &url::Url) -> Option<String> {
    let path = percent_encoding::percent_decode_str(u.path().trim_start_matches('/')).decode_utf8().ok()?;
    (!path.contains(['?', '#', '\\', '\0'])).then(|| path.into_owned())
}

/// Whether the site asking (`initiator`, its origin) may load that file of the
/// extension's: one it lists for any site, or for sites that one is among.
fn web_accessible(ext: &kite_core::webext::registry::Extension, path: &str, initiator: &str) -> bool {
    let listed = |g: &String| {
        let g = g.trim_start_matches('/');
        regex::Regex::new(&kite_core::webext::matchpattern::glob_to_regex(g)).map(|r| r.is_match(path)).unwrap_or(false)
    };
    let site = format!("{}/", initiator.trim_end_matches('/'));
    ext.manifest.web_accessible_resources.iter().any(listed)
        || ext.manifest.web_accessible_for.iter().any(|(sites, files)| files.iter().any(listed) && kite_core::webext::matchpattern::MatchSet::parse(sites.iter().map(String::as_str)).matches(&site))
}

fn serve(url: &str, initiator: &str, in_private: bool) -> (i32, String, Vec<u8>) {
    if url.starts_with(kite_core::standin::SCHEME) {
        return match STAND_INS.get(url) {
            Some((mime, body)) => (200, mime, body),
            None => (404, "text/plain".into(), vec![]),
        };
    }
    if let Some(rest) = url.strip_prefix("kite:") {
        let page = rest.split(['?', '#']).next().unwrap_or("");
        // Kite's pages are for Kite: the address bar, its own pages, and the
        // tab itself (empty initiator). A web page (an opaque "null" one
        // included) may only reach the dangerous-site warning and the page for a
        // site blocked in a child's profile, which the request interceptor sends
        // their links to. Anything else let pages
        // probe history (favicon), bounce through kite:search to file: and
        // kite: addresses, or frame Kite's pages.
        let from_kite = initiator.is_empty() || initiator.starts_with("kite:");
        if !from_kite && page != "dangerous" && page != "blocked" {
            return (403, "text/plain".into(), vec![]);
        }
        return match page {
            "start" | "speeddial" | "newtab" | "home" => {
                let (msg, undo) = crate::startpage::edit(url, initiator).unwrap_or_default();
                (200, "text/html".into(), crate::startpage::render(&msg, &undo, in_private).into_bytes())
            }
            "search" => {
                // kite:search?q=... from the start page form
                let q = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.query_pairs().find(|(k, _)| k == "q").map(|(_, v)| v.into_owned()))
                    .unwrap_or_default();
                let target = crate::browser::resolve_input(&q);
                // Another program's address: a link to click (anything can link
                // to this page, and a click on it still asks before opening).
                if kite_core::search::is_external_url(&target) {
                    let shown = target.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;");
                    let html = format!("<!DOCTYPE html><meta charset=utf-8><title>Open in Another App</title><p style=\"font:16px sans-serif;margin:3em\"><a href=\"{shown}\">Open {shown} in another app</a></p>");
                    return (200, "text/html".into(), html.into_bytes());
                }
                // From the start page's search box: a search or a web address
                // (or another app's, above), never a kite: or file: one.
                if !initiator.is_empty() && !(target.starts_with("https://") || target.starts_with("http://")) {
                    return (403, "text/plain".into(), vec![]);
                }
                let html = format!("<!DOCTYPE html><meta http-equiv=\"refresh\" content=\"0;url={}\">", target.replace('"', "%22"));
                (200, "text/html".into(), html.into_bytes())
            }
            // Cached site icons for the speed dial: kite:favicon?url=<page url>
            "favicon" => {
                let page = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.query_pairs().find(|(k, _)| k == "url").map(|(_, v)| v.into_owned()))
                    .unwrap_or_default();
                match kite_core::favicons::for_url(&app().shared.db(), &page) {
                    Ok(Some(png)) => (200, "image/png".into(), png),
                    _ => (404, "text/plain".into(), vec![]),
                }
            }
            // The custom start page copied into the profile, and its files.
            p if p == "startpage" || p.starts_with("startpage/") => {
                let rel = rest.strip_prefix("startpage").unwrap_or("");
                match kite_core::startpage::resolve(app().shared.profile.root(), rel) {
                    Some(file) => match std::fs::read(&file) {
                        Ok(body) => (200, mime_guess::from_path(&file).first_or_octet_stream().essence_str().to_owned(), body),
                        Err(_) => (404, "text/plain".into(), vec![]),
                    },
                    None => (404, "text/html".into(), b"<h1>Not found</h1><p>The start page copy has no such file.</p>".to_vec()),
                }
            }
            // An article in reader view (kite_core::reader).
            "reader" => {
                let prefs = kite_core::reader::Prefs::from_settings(&app().shared.settings());
                (200, "text/html".into(), kite_core::reader::page(url, &prefs).into_bytes())
            }
            // A known phishing or malware site (dangerous-site protection).
            "dangerous" => (200, "text/html".into(), dangerous_page(&web_address(&query_param(url, "url")), &query_param(url, "threat")).into_bytes()),
            // A site that doesn't open in a child's profile (parental controls).
            "blocked" => (200, "text/html".into(), blocked_page(&web_address(&query_param(url, "url")), &query_param(url, "why")).into_bytes()),
            // Every setting. It holds what's needed to change them, so no web page gets it.
            // (Not in a child's profile that may not change settings.)
            "config" if from_kite && !app().parental.allows(kite_core::parental::Permission::Settings) => {
                let html = "<!DOCTYPE html><meta charset=utf-8><meta name=color-scheme content=\"light dark\"><title>Settings are locked</title><body style=\"font:15px/1.5 system-ui,sans-serif;max-width:38em;margin:12vh auto;padding:0 1.5em\"><h1 style=\"font-size:1.5em;font-weight:600\">Settings are locked</h1><p>This is a child's profile. A parent can unlock its settings in Settings → Parental Controls.</p>";
                (200, "text/html".into(), html.as_bytes().to_vec())
            }
            "config" if from_kite => {
                (200, "text/html".into(), kite_core::config::page(&crate::browser::config_entries(), crate::state::config_token(), &kite_core::config::secret()).into_bytes())
            }
            "config" => (403, "text/plain".into(), vec![]),
            // The first-launch page (kite_core::welcome).
            "welcome" => {
                let sources = crate::browser::import_sources();
                // The web search engines this profile has, to choose from.
                let have: Vec<String> = kite_core::search::load_engines(&app().shared.db()).unwrap_or_default().into_iter().map(|e| e.name).collect();
                let engines: Vec<String> = kite_core::search::WEB_SEARCH.iter().filter(|n| have.iter().any(|h| h == *n)).map(|n| n.to_string()).collect();
                let engine = app().shared.settings().get_str("general.searchEngine");
                (200, "text/html".into(), kite_core::welcome::page(crate::state::welcome_token(), &kite_core::config::secret(), &sources, &engines, &engine).into_bytes())
            }
            // HTTPS-only: the site has no working https version.
            "https-only" => (200, "text/html".into(), https_only_page(&web_address(&query_param(url, "url"))).into_bytes()),
            _ => (404, "text/html".into(), b"<h1>Not found</h1>".to_vec()),
        };
    }
    let Ok(u) = url::Url::parse(url) else { return (400, "text/plain".into(), vec![]) };
    let uuid = u.host_str().unwrap_or("");
    let Some(path) = extension_path(&u) else { return (404, "text/plain".into(), vec![]) };
    let rt = app().runtime();
    // (In a private window, only an extension allowed there is: to the others' files
    // and pages a private tab is as if they weren't installed.)
    let Some(ext) = rt.registry.by_uuid(uuid).filter(|e| e.info.enabled && (!in_private || e.runs_in_private())) else {
        return (404, "text/plain".into(), b"Extension not found".to_vec());
    };
    // Same-extension or browser-initiated loads may load anything; other
    // origins only web_accessible_resources (Firefox semantics). An opaque
    // origin ("null": sandboxed or srcdoc/data: frames) is never taken for
    // the extension's: treating it as same-origin handed any page the
    // extension's token through api.js. (What it does get: further down.)
    let same_origin = initiator.is_empty() || initiator.trim_end_matches('/').eq_ignore_ascii_case(ext.base_url().trim_end_matches('/'));
    if path == scripts::API_PATH {
        if !same_origin {
            return (403, "text/plain".into(), vec![]);
        }
        let token = rt.token_of(&ext.info.id).unwrap_or("").to_owned();
        let firefox_ua = app().shared.settings().get_bool("extensions.firefoxUserAgent").then_some(kite_core::util::FIREFOX_USER_AGENT);
        return (200, "application/javascript".into(), scripts::page_api_js(ext, &token, qwebchannel_js(), rt.blocking.port(), firefox_ua).into_bytes());
    }
    if path == scripts::BACKGROUND_PAGE {
        // Kite's to load, once: in a frame of some site it would be a second copy of the
        // extension's background scripts, running there.
        if !same_origin {
            return (403, "text/plain".into(), vec![]);
        }
        return match scripts::background_page(ext) {
            Some(html) => (200, "text/html".into(), html.into_bytes()),
            None => (404, "text/plain".into(), vec![]),
        };
    }
    // What isn't web-accessible, asked for by another origin. An opaque one may be a page of
    // the extension's own in a sandboxed frame, drawing with its own script and stylesheet (a
    // password manager's menu inside a form field; Firefox knows whose sandbox it is, the
    // engine here says "null" for a web page's just the same). It gets what a page is drawn
    // with (`for_sandboxed_frames`), never a page: those stay for the sites the manifest names.
    let foreign = !same_origin && !web_accessible(ext, &path, initiator);
    if foreign && initiator != "null" {
        return (403, "text/plain".into(), vec![]);
    }
    let Some(file) = ext.file(&path) else { return (403, "text/plain".into(), vec![]) };
    let file = if file.is_dir() { file.join("index.html") } else { file };
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&file).first_or_octet_stream().essence_str().to_owned();
            let mime = if path.ends_with(".mjs") { "application/javascript".into() } else { mime };
            if foreign && !for_sandboxed_frames(&mime) {
                return (403, "text/plain".into(), vec![]);
            }
            if mime == "text/html" {
                let html = String::from_utf8_lossy(&bytes);
                (200, mime, scripts::inject_api_into_html(&html).into_bytes())
            } else if mime == "text/css" {
                // Firefox supports __MSG_*__ in extension CSS.
                let css = String::from_utf8_lossy(&bytes);
                (200, mime, ext.messages.localize(&css, &ext.info.uuid).into_bytes())
            } else {
                (200, mime, bytes)
            }
        }
        // (Not telling another origin which files there are.)
        Err(_) if foreign => (403, "text/plain".into(), vec![]),
        Err(_) => (404, "text/plain".into(), b"Not found".to_vec()),
    }
}

/// The kinds of file a sandboxed frame may load from an extension without their being
/// web-accessible: what a page is drawn with. Not documents (a page of the extension's, framed
/// by any site), nor data a script would read.
fn for_sandboxed_frames(mime: &str) -> bool {
    matches!(mime, "text/css" | "application/javascript" | "text/javascript") || mime.starts_with("image/") || mime.starts_with("font/")
}

/// `__kite__/rpc` and `__kite__/poll` for extension pages in sub-frames.
/// Returns 1 if the request is held (completed later via
/// `kite_cpp_rpc_complete`), else 0 with `status`/`out` filled in.
#[no_mangle]
pub unsafe extern "C" fn kite_rpc(url: *const c_char, initiator: *const c_char, in_private: std::ffi::c_int, body: *const u8, body_len: usize, job: u64, status: *mut i32, out: *mut KiteBuf) -> i32 {
    let url = s(url);
    let initiator = s(initiator);
    let body = if body.is_null() { &[][..] } else { std::slice::from_raw_parts(body, body_len) };
    let Ok(u) = url::Url::parse(url) else {
        *status = 400;
        *out = KiteBuf::empty();
        return 0;
    };
    let (ext_id, base) = {
        let rt = app().runtime();
        match rt.registry.by_uuid(u.host_str().unwrap_or("")).filter(|e| e.info.enabled && (in_private == 0 || e.runs_in_private())) {
            Some(e) => (e.info.id.clone(), e.base_url()),
            None => {
                *status = 404;
                *out = KiteBuf::empty();
                return 0;
            }
        }
    };
    let pq = match u.query() {
        Some(q) => format!("{}?{q}", u.path()),
        None => u.path().to_owned(),
    };
    match crate::rpc::handle(&pq, &ext_id, &base, initiator, body, job) {
        crate::rpc::Handled::Now(code, json) => {
            *status = code;
            *out = KiteBuf::from_vec(json.into_bytes());
            0
        }
        crate::rpc::Handled::Held => 1,
    }
}

#[no_mangle]
pub extern "C" fn kite_rpc_job_gone(job: u64) {
    crate::rpc::job_gone(job);
}

/// How long the interceptor waits for blocking webRequest listeners.
/// Firefox waits indefinitely, but here the UI thread waits; extensions that
/// miss it are skipped for a moment (see `BlockingHub`).
const BLOCKING_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

static STAND_INS: std::sync::LazyLock<kite_core::standin::StandIns> = std::sync::LazyLock::new(Default::default);

/// Verdict for redirecting a request to a neutered stand-in (an extension's
/// file, or a data: URL from uBlock Origin or the built-in blocker).
///
/// * XHR/fetch are blocked instead: QtWebEngine answers an interceptor
///   redirect with an internal redirect response whose
///   Access-Control-Allow-Origin is the *target's* origin rather than the
///   page's, so every cross-origin redirect of a CORS request fails anyway.
///   Blocking gives the page the same failed request without a misleading
///   CORS error or the extension's moz-extension:// id.
/// * data: URLs are served from kite-res: (Chromium refuses to redirect
///   subresources to data: URLs).
/// * Extension files are redirected to as they are (moz-extension: is a
///   valid redirect target).
fn stand_in(to: &str, rtype: &str) -> String {
    let neutered = to.starts_with("data:") || to.starts_with("moz-extension:");
    if neutered && rtype == "xhr" {
        return "block".into();
    }
    if to.starts_with("data:") && !matches!(rtype, "mainFrame" | "subFrame") {
        return match STAND_INS.register(to) {
            Some(u) => format!("redirect:{u}"),
            None => "block".into(),
        };
    }
    format!("redirect:{to}")
}

/// Why a request doesn't go through in a child's profile (parental controls), if so.
fn blocked_for_child(url: &str) -> Option<kite_core::parental::Blocked> {
    let filter = &app().parental.filter;
    if !filter.is_on() {
        return None;
    }
    filter.check(&kite_core::util::host_of(url))
}

/// Where Kite itself sends a page load instead: to the page for a site blocked in a
/// child's profile, to a warning about a dangerous site, without tracking parameters,
/// over https (privacy.*).
fn builtin_page_redirect(url: &str) -> Option<String> {
    let a = app();
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    // A child's profile: sites it's kept from get a page saying so.
    if let Some(why) = blocked_for_child(url) {
        log::info!("parental controls: {} blocked ({})", kite_core::util::host_of(url), why.as_str());
        return Some(format!("kite:blocked?why={}&url={}", why.as_str(), enc(url)));
    }
    if let Some(threat) = a.safe_browsing.check(url) {
        log::info!("safebrowsing: {} listed as {}", url, threat.as_str());
        return Some(format!("kite:dangerous?threat={}&url={}", threat.as_str(), enc(url)));
    }
    if a.privacy.strip_params.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(clean) = kite_core::privacy::strip_tracking_params(url) {
            log::debug!("privacy: removed tracking parameters from {url}");
            return Some(clean);
        }
    }
    if let Some(secure) = a.privacy.upgrade(url) {
        log::debug!("privacy: upgraded {url} to https");
        return Some(secure);
    }
    None
}

enum ListenersSay {
    Nothing,
    Block,
    Redirect(String),
}

/// What extensions' webRequest listeners want done with a request. `tab`: the
/// tab it is from, where that is known (a page load's); anything else is
/// matched to a tab by the page it is for.
fn ask_web_request_listeners(url: &str, first_party: &str, initiator: &str, rtype: &str, method: &str, in_private: bool, tab: Option<i64>) -> ListenersSay {
    let a = app();
    let mut rt = a.runtime();
    if !rt.has_web_request_listeners() {
        return ListenersSay::Nothing;
    }
    let tab = tab.filter(|t| rt.knows_tab(*t, in_private)).unwrap_or_else(|| rt.tab_for_first_party(first_party, in_private));
    static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let frame = kite_core::webext::blocking::frame_context(url, rtype, first_party, initiator);
    let mut details = serde_json::json!({
        "requestId": REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string(), "url": url, "method": method,
        "type": kite_core::webext::dnr::resource_type(rtype), "tabId": tab, "frameId": frame.frame_id,
        "parentFrameId": frame.parent_frame_id, "originUrl": initiator, "frameAncestors": frame.frame_ancestors,
        "timeStamp": kite_core::util::now_ms(), "thirdParty": kite_core::util::host_of(url) != kite_core::util::host_of(first_party) && !first_party.is_empty(),
        "incognito": a.private || in_private, "cookieStoreId": if in_private { "firefox-private" } else { "firefox-default" },
    });
    if let Some(doc) = frame.document_url {
        details["documentUrl"] = doc.into();
    }
    // An extension's own requests (from its pages, or inside them) are told
    // to that extension alone: another's listener would read in them what the
    // first one asks for, and of whom.
    let owner = rt.request_owner(initiator, first_party);
    let (fx, ask) = rt.web_request(&details, in_private, owner.as_deref());
    let hub = rt.blocking.clone();
    drop(rt);
    crate::extensions::apply_effects_async(fx);
    // Never ask extensions about their own requests: the background
    // page would be waiting for itself.
    if ask.is_empty() || owner.is_some() {
        return ListenersSay::Nothing;
    }
    let t0 = std::time::Instant::now();
    let replies = hub.ask(&ask, "onBeforeRequest", &details, BLOCKING_TIMEOUT);
    log::trace!("webRequest: asked {ask:?} about {url} (tab {tab}, {rtype}) in {:?}: {replies:?}", t0.elapsed());
    let d = kite_core::webext::blocking::combine(&replies);
    if d.cancel {
        log::debug!("webRequest: blocked {url}");
        return ListenersSay::Block;
    }
    if let Some((ext, to)) = d.redirect {
        if !a.runtime().redirect_allowed(&ext, &to, rtype == "mainFrame") {
            log::warn!("webRequest: {ext} redirected {url} to {}, which is not followed: not an address an extension may send a request to", &to[..to.floor_char_boundary(80)]);
            return ListenersSay::Nothing;
        }
        log::debug!("webRequest: {ext} redirected {url} to {to}");
        return ListenersSay::Redirect(to);
    }
    ListenersSay::Nothing
}

/// Page loads onNavigationRequested went through, to not ask extensions twice.
static DECIDED_PAGE_LOADS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn take_decided_page_load(url: &str) -> bool {
    let mut decided = DECIDED_PAGE_LOADS.lock().unwrap_or_else(|e| e.into_inner());
    match decided.iter().position(|u| u == url) {
        Some(i) => {
            decided.remove(i);
            true
        }
        None => false,
    }
}

/// Page loads the interceptor is still to see, and the tab each is in: a
/// request doesn't say whose it is, and extensions are told (`tabId`).
static PAGE_LOAD_TABS: std::sync::Mutex<Vec<(String, i64)>> = std::sync::Mutex::new(Vec::new());

fn note_page_load_tab(url: &str, tab: i64) {
    if tab < 0 {
        return;
    }
    let mut loads = PAGE_LOAD_TABS.lock().unwrap_or_else(|e| e.into_inner());
    loads.retain(|(u, _)| u != url);
    // (A load that never reached the interceptor stays here: this keeps them few.)
    if loads.len() >= 64 {
        loads.remove(0);
    }
    loads.push((url.to_owned(), tab));
}

fn take_page_load_tab(url: &str) -> Option<i64> {
    let mut loads = PAGE_LOAD_TABS.lock().unwrap_or_else(|e| e.into_inner());
    let i = loads.iter().position(|(u, _)| u == url)?;
    Some(loads.remove(i).1)
}

/// Where a page load should go instead, or "". For WebTab's onNavigationRequested,
/// as a redirect from the request interceptor never finishes on a page a service
/// worker answers for: the page just stays blank. Before the load starts, there's
/// nothing to get in the way.
///
/// `tab`: the tab it is in, which extensions' listeners are told. `redirect_here`
/// false: a load that can't simply be started again (going back, a form being
/// posted) is the interceptor's to decide; this only says which tab it is in.
pub fn page_load_redirect(url: &str, in_private: bool, tab: i64, redirect_here: bool) -> String {
    if !(url.starts_with("http:") || url.starts_with("https:")) || kite_core::webext::amo::is_amo_download(url) {
        return String::new();
    }
    if !redirect_here {
        note_page_load_tab(url, tab);
        return String::new();
    }
    if let Some(to) = builtin_page_redirect(url) {
        return to;
    }
    match ask_web_request_listeners(url, "", "", "mainFrame", "GET", in_private, Some(tab)) {
        ListenersSay::Redirect(to) if to != url => to,
        // (Blocking is the interceptor's: it asks again.)
        ListenersSay::Block | ListenersSay::Redirect(_) => {
            note_page_load_tab(url, tab);
            String::new()
        }
        ListenersSay::Nothing => {
            let mut decided = DECIDED_PAGE_LOADS.lock().unwrap_or_else(|e| e.into_inner());
            // (A load that never reached the interceptor stays here: this keeps them few.)
            if decided.len() >= 64 {
                decided.remove(0);
            }
            decided.push(url.to_owned());
            String::new()
        }
    }
}

/// Network request interceptor. Returns "" (allow), "block",
/// "redirect:<url>" or "headers:<json [[name, value], ...]>".
#[no_mangle]
pub unsafe extern "C" fn kite_intercept(
    url: *const c_char,
    first_party: *const c_char,
    initiator: *const c_char,
    rtype: *const c_char,
    method: *const c_char,
    in_private: std::ffi::c_int,
) -> KiteBuf {
    let (url, first_party, initiator, rtype, method) = (s(url), s(first_party), s(initiator), s(rtype), s(method));
    let a = app();
    let started = std::time::Instant::now();
    let out = (|| {
        if !(url.starts_with("http") || url.starts_with("ws")) {
            return String::new();
        }
        // The connections of extensions' blocking listeners to Kite itself
        // (webext/blocking.rs) are no one's to see, block or send elsewhere:
        // what is sent over them says which extension it is.
        if let Some(rest) = url.strip_prefix("ws://127.0.0.1:") {
            let port = a.runtime().blocking.port();
            if port != 0 && rest.split(['/', '?', '#']).next() == Some(port.to_string().as_str()) {
                return String::new();
            }
        }
        // Never interfere with add-on installs from AMO.
        if kite_core::webext::amo::is_amo_download(url) {
            return String::new();
        }
        // A child's profile: nothing is fetched from a site it's kept from, whatever
        // asks. (A page load gets the page saying so, below.)
        if rtype != "mainFrame" && blocked_for_child(url).is_some() {
            return "block".into();
        }
        // Page loads: what onNavigationRequested already went through isn't asked again.
        let decided = rtype == "mainFrame" && take_decided_page_load(url);
        let tab = if rtype == "mainFrame" { take_page_load_tab(url) } else { None };
        if rtype == "mainFrame" && !decided {
            if let Some(to) = builtin_page_redirect(url) {
                return format!("redirect:{to}");
            }
        }
        // (From a private window, only extensions allowed there are involved.)
        let in_private = in_private != 0;
        // webRequest listeners
        if !decided {
            match ask_web_request_listeners(url, first_party, initiator, rtype, method, in_private, tab) {
                ListenersSay::Block => return "block".into(),
                ListenersSay::Redirect(to) => return stand_in(&to, rtype),
                ListenersSay::Nothing => {}
            }
        }
        let rt = a.runtime();
        match rt.dnr_check(url, if initiator.is_empty() { first_party } else { initiator }, method, rtype, in_private) {
            kite_core::webext::dnr::Outcome::Block => return "block".into(),
            kite_core::webext::dnr::Outcome::Redirect(u) => return format!("redirect:{u}"),
            kite_core::webext::dnr::Outcome::Headers(h) => return format!("headers:{}", serde_json::to_string(&h).unwrap_or_default()),
            kite_core::webext::dnr::Outcome::None => {}
        }
        // An extension like uBlock Origin does the content blocking.
        if rt.content_blocker_for(in_private).is_some() {
            return String::new();
        }
        drop(rt);
        match a.adblock.check(url, first_party, rtype) {
            kite_core::adblock::Verdict::Block => "block".into(),
            kite_core::adblock::Verdict::Redirect(r) if r.starts_with("http") => format!("redirect:{r}"),
            // adblock redirect resources are data: URLs
            kite_core::adblock::Verdict::Redirect(r) => stand_in(&r, rtype),
            kite_core::adblock::Verdict::Allow => String::new(),
        }
    })();
    // This runs on the UI thread: anything slow here stutters the UI and video.
    log::trace!("intercepted {rtype} in {:?}: {}", started.elapsed(), &url[..url.floor_char_boundary(120)]);
    KiteBuf::from_vec(out.into_bytes())
}

/// Qt/QML log output. QML categories `console.page` / `console.extension` /
/// `kite.<name>` become log targets `console::page` / `console::extension` /
/// `kite::<name>`; other Qt categories `qt::<category>`. Any of them with
/// "private." before it comes from a private window: its addresses are scrambled.
#[no_mangle]
pub unsafe extern "C" fn kite_log(level: std::ffi::c_int, category: *const c_char, message: *const c_char) {
    let level = match level {
        1 => log::Level::Error,
        2 => log::Level::Warn,
        3 => log::Level::Info,
        _ => log::Level::Debug,
    };
    let category = s(category);
    // A private window's lines ("private.<category>"): addresses scrambled.
    let (category, private) = match category.strip_prefix("private.") {
        Some(rest) => (rest, true),
        None => (category, false),
    };
    let target = match category {
        c if c.starts_with("console.") || c.starts_with("kite.") => c.replace('.', "::"),
        "default" => "qt".to_owned(),
        c => format!("qt::{c}"),
    };
    let message = s(message);
    if private {
        log::log!(target: target.as_str(), level, "{}", kite_core::crashlog::scramble_urls(message));
    } else {
        log::log!(target: target.as_str(), level, "{message}");
    }
}

/// Cookie store change (from QWebEngineCookieStore signals).
#[no_mangle]
pub unsafe extern "C" fn kite_cookie_event(json: *const c_char, removed: i32, initial: i32) {
    let Ok(cookie) = serde_json::from_str::<serde_json::Value>(s(json)) else { return };
    let fx = app().runtime().browser_event("cookieChanged", &serde_json::json!({"cookie": cookie, "removed": removed != 0, "initial": initial != 0}));
    crate::extensions::apply_effects_async(fx);
}

/// DNS over HTTPS from the settings ("privacy.dnsOverHttps", "privacy.dnsServer").
pub fn apply_dns() {
    let s = app().shared.settings();
    let mode = match s.get_str("privacy.dnsOverHttps").as_str() {
        "auto" => 1,
        "secure" => 2,
        _ => 0,
    };
    let server = CString::new(s.get_str("privacy.dnsServer")).unwrap_or_default();
    drop(s);
    unsafe { kite_cpp_apply_dns(mode, server.as_ptr()) }
}

/// At startup (bootstrap.cpp, once Qt WebEngine is up).
#[no_mangle]
pub extern "C" fn kite_init_dns() {
    apply_dns();
}

/// The site cookie rule for a cookie's host (glue.cpp's cookie filter):
/// 0 = none, 1 = allow, 2 = block, 3 = allow until Kite closes.
#[no_mangle]
pub unsafe extern "C" fn kite_cookie_rule(host: *const c_char) -> std::ffi::c_int {
    app().privacy.cookie_rule(s(host)) as std::ffi::c_int
}

/// Hosts whose cookies go when Kite closes, as a JSON array.
#[no_mangle]
pub extern "C" fn kite_session_cookie_hosts() -> KiteBuf {
    KiteBuf::from_vec(serde_json::to_vec(&app().privacy.session_cookie_hosts()).unwrap_or_default())
}

pub fn hook_container_profiles() {
    unsafe { kite_cpp_hook_container_profiles() }
}

/// Delete the cookies of the sites set to "until Kite closes".
/// Those of these hosts and the names under them.
pub fn delete_cookies_of(hosts: &[String]) {
    let c = CString::new(serde_json::to_string(hosts).unwrap_or_default()).unwrap_or_default();
    unsafe { kite_cpp_delete_cookies_of(c.as_ptr()) }
}

pub fn delete_session_cookies() {
    let hosts = serde_json::to_string(&app().privacy.session_cookie_hosts()).unwrap_or_default();
    let c = CString::new(hosts).unwrap_or_default();
    unsafe { kite_cpp_delete_cookies_of(c.as_ptr()) }
}

/// Send the Global Privacy Control header ("privacy.globalPrivacyControl").
#[no_mangle]
pub extern "C" fn kite_gpc() -> bool {
    app().privacy.gpc.load(std::sync::atomic::Ordering::Relaxed)
}

/// Send the Do Not Track header ("web.doNotTrack").
#[no_mangle]
pub extern "C" fn kite_do_not_track() -> bool {
    app().do_not_track.load(std::sync::atomic::Ordering::Relaxed)
}

/// Send referrers ("privacy.sendReferer").
#[no_mangle]
pub extern "C" fn kite_send_referer() -> bool {
    app().send_referer.load(std::sync::atomic::Ordering::Relaxed)
}

/// Cookie policy ("privacy.cookies"): 0 = all, 1 = no third-party, 2 = none.
#[no_mangle]
pub extern "C" fn kite_cookie_policy() -> std::ffi::c_int {
    app().cookie_policy.load(std::sync::atomic::Ordering::Relaxed) as std::ffi::c_int
}

/// Should this user agent override apply: the one chosen for the site (Site
/// Settings), else AMO sees Kite as Firefox so it offers "Add to Firefox"
/// buttons that lead to .xpi files (and Google's sign-in), else another
/// browser's chosen for all sites (Settings → Browsing). Kite's own, with or
/// without "QtWebEngine", is the profile's and needs none.
#[no_mangle]
pub unsafe extern "C" fn kite_user_agent_for(url: *const c_char) -> KiteBuf {
    let url = s(url);
    if url.starts_with("http") {
        if let Some(ua) = app().privacy.site_user_agent(&kite_core::util::host_of(url)) {
            return KiteBuf::from_vec(ua.into_bytes());
        }
    }
    let spoof = app().shared.settings().get_bool("extensions.amoSpoofFirefox");
    if spoof && kite_core::util::host_of(url) == "addons.mozilla.org" {
        return KiteBuf::from_vec(kite_core::webext::amo::USER_AGENT.as_bytes().to_vec());
    }
    if kite_core::util::is_google_sign_in(url) && app().shared.settings().get_bool("web.googleSignIn") {
        return KiteBuf::from_vec(kite_core::util::FIREFOX_USER_AGENT.as_bytes().to_vec());
    }
    if url.starts_with("http") {
        if let Some(ua) = global_user_agent() {
            return KiteBuf::from_vec(ua.into_bytes());
        }
    }
    KiteBuf::empty()
}

/// Another browser's user agent chosen for all sites ("web.userAgent"), if one is.
pub fn global_user_agent() -> Option<String> {
    let ua = app().shared.settings().get_str("web.userAgent");
    let ua = ua.trim();
    (!ua.is_empty() && ua != kite_core::util::QTWEBENGINE_USER_AGENT).then(|| ua.to_owned())
}

fn query_param(url: &str, name: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned())).unwrap_or_default()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The address a warning page was given, if it is a web page's ("" otherwise). These
/// pages put it in links and go there by themselves, and any web page can send one
/// here: a `javascript:` address in a link would run as one of Kite's own pages, and a
/// `kite:` one be asked for by one.
fn web_address(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => u.into(),
        _ => String::new(),
    }
}

/// What a warning page may load and run: nothing but its own style, and its own script
/// by that nonce. Whatever ends up in one of its links, it never runs there.
fn warning_policy(nonce: &str) -> String {
    format!(r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-{nonce}'; base-uri 'none'; form-action 'none'">"#)
}

/// A warning page's "Go back", and the script behind it.
const GO_BACK: &str = r#"<a class="button primary" id="back" href="">Go back</a>"#;

fn go_back_script(nonce: &str) -> String {
    format!(r#"<script nonce="{nonce}">const back = document.getElementById('back'); if (back) back.addEventListener('click', e => {{ e.preventDefault(); history.back(); }});</script>"#)
}

/// The page shown when HTTPS-only couldn't load a site over https.
fn https_only_page(http_url: &str) -> String {
    let host = html_escape(&kite_core::util::host_of(http_url));
    let allow = format!("kite:https-allow?url={}&key={}", url::form_urlencoded::byte_serialize(http_url.as_bytes()).collect::<String>(), crate::state::warning_key());
    let nonce = kite_core::config::secret();
    let (policy, script) = (warning_policy(&nonce), go_back_script(&nonce));
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8">{policy}<title>Secure connection not available</title>
<meta name="color-scheme" content="light dark">
<style>
body {{ font: 15px/1.5 system-ui, sans-serif; max-width: 38em; margin: 12vh auto; padding: 0 1.5em; }}
h1 {{ font-size: 1.5em; font-weight: 600; }}
.host {{ font-weight: 600; }}
.buttons {{ margin-top: 2em; display: flex; gap: .75em; flex-wrap: wrap; }}
a.button {{ padding: .5em 1.1em; border-radius: 4px; text-decoration: none; border: 1px solid #888; color: inherit; }}
a.primary {{ background: #3daee9; border-color: #3daee9; color: #fff; }}
</style></head><body>
<h1>Secure connection not available</h1>
<p>HTTPS-only mode is on, but <span class="host">{host}</span> could not be loaded over a secure connection.</p>
<p>The site may not support HTTPS. If you continue, the page is loaded without encryption: others on your network could see or change what you send and receive.</p>
<div class="buttons">{GO_BACK}<a class="button" href="{allow}">Continue to the HTTP site</a></div>
{script}</body></html>"#,
        allow = html_escape(&allow)
    )
}

/// The warning shown instead of a known dangerous site.
fn dangerous_page(target: &str, threat: &str) -> String {
    let host = html_escape(&kite_core::util::host_of(target));
    let (title, text) = match threat {
        "phishing" => ("Deceptive site ahead", "This site is listed as a phishing site: it may pretend to be another site to steal passwords, payment details or other personal information."),
        "unwanted" => ("Site with unwanted software ahead", "This site is listed for offering software that changes your system or browser in ways you may not want."),
        _ => ("Dangerous site ahead", "This site is listed for spreading malware: it may try to install programs that steal information or damage your computer."),
    };
    let source = match app().safe_browsing.mode() {
        kite_core::safebrowsing::Mode::Google => "Google Safe Browsing",
        _ => "the malware-filter project's open lists",
    };
    let allow = format!("kite:dangerous-allow?url={}&key={}", url::form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>(), crate::state::warning_key());
    let nonce = kite_core::config::secret();
    let (policy, script) = (warning_policy(&nonce), go_back_script(&nonce));
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8">{policy}<title>{title}</title>
<meta name="color-scheme" content="light dark">
<style>
body {{ font: 15px/1.5 system-ui, sans-serif; max-width: 38em; margin: 12vh auto; padding: 0 1.5em; }}
h1 {{ font-size: 1.5em; font-weight: 600; color: #da4453; }}
.host {{ font-weight: 600; }}
.small {{ opacity: .7; font-size: .9em; }}
.buttons {{ margin-top: 2em; display: flex; gap: .75em; flex-wrap: wrap; }}
a.button {{ padding: .5em 1.1em; border-radius: 4px; text-decoration: none; border: 1px solid #888; color: inherit; }}
a.primary {{ background: #3daee9; border-color: #3daee9; color: #fff; }}
</style></head><body>
<h1>{title}</h1>
<p><span class="host">{host}</span>: {text}</p>
<p class="small">Listed by {source}. Lists can be wrong; only continue if you trust this site.</p>
<div class="buttons">{GO_BACK}<a class="button" href="{allow}">Continue anyway</a></div>
{script}</body></html>"#,
        allow = html_escape(&allow)
    )
}

/// The page shown in a child's profile instead of a site it's kept from.
fn blocked_page(target: &str, why: &str) -> String {
    let host = html_escape(&kite_core::util::host_of(target));
    let (title, text) = match why {
        "listed" => ("This site is blocked", format!("<span class=\"host\">{host}</span> has been blocked for this profile.")),
        "not-allowed" => ("This site isn't open to this profile", format!("Only some sites open in this profile, and <span class=\"host\">{host}</span> isn't one of them.")),
        "search" => ("This search engine is blocked", format!("<span class=\"host\">{host}</span> can't be kept to safe search, so it doesn't open in this profile. DuckDuckGo, Google, Bing and Ecosia do.")),
        "no-lists" => ("Not ready yet", "Kite is downloading its lists of blocked sites, and no site opens until it has them. This page tries again by itself; if it stays, check the internet connection.".to_owned()),
        _ => ("This site is blocked", format!("<span class=\"host\">{host}</span> is on a list of sites that aren't for children.")),
    };
    let allow = format!("kite:blocked-allow?url={}&key={}", url::form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>(), crate::state::warning_key());
    let nonce = kite_core::config::secret();
    let (policy, script) = (warning_policy(&nonce), go_back_script(&nonce));
    // (`target`: a web page's address or none, see `web_address`.)
    let buttons = if why != "no-lists" {
        format!("{GO_BACK}<a class=\"button\" href=\"{}\">Allow this site…</a>", html_escape(&allow))
    } else if target.is_empty() {
        GO_BACK.to_owned()
    } else {
        format!("<a class=\"button primary\" href=\"{}\">Try again</a>", html_escape(target))
    };
    let note = if why == "no-lists" { "" } else { "<p class=\"small\">Allowing a site takes the parent password.</p>" };
    // (The lists are on their way when Kite starts: the page is tried again by itself.)
    let again = if why == "no-lists" && !target.is_empty() { format!("<meta http-equiv=\"refresh\" content=\"3;url={}\">", html_escape(target)) } else { String::new() };
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8">{policy}<title>{title}</title>
<meta name="color-scheme" content="light dark">{again}
<style>
body {{ font: 15px/1.5 system-ui, sans-serif; max-width: 38em; margin: 12vh auto; padding: 0 1.5em; }}
h1 {{ font-size: 1.5em; font-weight: 600; }}
.host {{ font-weight: 600; }}
.small {{ opacity: .7; font-size: .9em; }}
.buttons {{ margin-top: 2em; display: flex; gap: .75em; flex-wrap: wrap; }}
a.button {{ padding: .5em 1.1em; border-radius: 4px; text-decoration: none; border: 1px solid #888; color: inherit; }}
a.primary {{ background: #3daee9; border-color: #3daee9; color: #fff; }}
</style></head><body>
<h1>{title}</h1>
<p>{text}</p>
<div class="buttons">{buttons}</div>
{note}
{script}</body></html>"#
    )
}

// ---- the profile picker (cpp/profiles.cpp) --------------------------------

static PICK_OPENS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_pick_profile_opens(open: bool) {
    PICK_OPENS.store(open, std::sync::atomic::Ordering::Relaxed);
}

fn profiles() -> Option<kite_core::profiles::Profiles> {
    kite_core::profiles::Profiles::open().ok()
}

fn parental() -> Option<kite_core::parental::Parental> {
    kite_core::parental::Parental::open().ok()
}

/// In the picker's own process: the parent password was given in it.
static PARENT_PRESENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The parent is at the picker: the password was given in it, or the Kite that
/// opened it handed it a pass (one a parent's profile gives).
fn parent_present() -> bool {
    PARENT_PRESENT.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var(kite_core::parental::PASS_VARIABLE).is_ok_and(|pass| parental().is_some_and(|p| p.pass_is_good(&pass)))
}

/// Profiles are made, renamed and deleted by the parent only, once one is a child's.
fn may_manage_profiles() -> anyhow::Result<()> {
    if parental().is_some_and(|p| p.guards_profiles()) && !parent_present() {
        anyhow::bail!("This takes the parent password.");
    }
    Ok(())
}

/// `guarded`: managing profiles takes the parent password first (`kite_parental_verify`).
#[no_mangle]
pub extern "C" fn kite_profiles_json() -> KiteBuf {
    let json = profiles()
        .map(|p| {
            serde_json::json!({"profiles": p.list(), "last": p.choices().last, "hasPassword": parental().is_some_and(|p| p.has_password()),
                "guarded": may_manage_profiles().is_err(), "parentPresent": parent_present(), "minPassword": kite_core::parental::MIN_PASSWORD})
            .to_string()
        })
        .unwrap_or_default();
    KiteBuf::from_vec(json.into_bytes())
}

/// The parent password, given in the picker or the dialog asking for it: "" if that's it.
#[no_mangle]
pub unsafe extern "C" fn kite_parental_verify(password: *const c_char) -> KiteBuf {
    let checked = kite_core::parental::Parental::open().and_then(|p| p.verify(s(password)));
    if checked.is_ok() {
        PARENT_PRESENT.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    error_buf(checked)
}

/// A new child's profile: with the first one the parent password is set (`password`),
/// later ones take it (unless the parent is at the picker already).
fn create_child_profile(name: &str, password: &str) -> anyhow::Result<()> {
    let parental = kite_core::parental::Parental::open()?;
    let profiles = kite_core::profiles::Profiles::open()?;
    if parental.has_password() {
        if !parent_present() {
            parental.verify(password)?;
        }
    } else if password.chars().count() < kite_core::parental::MIN_PASSWORD {
        anyhow::bail!("The password needs at least {} characters.", kite_core::parental::MIN_PASSWORD);
    }
    profiles.create(name)?;
    if !parental.has_password() {
        parental.set_password("", password)?;
    }
    PARENT_PRESENT.store(true, std::sync::atomic::Ordering::Relaxed);
    parental.set_child(name, Some(Default::default()))
}

fn error_buf(r: anyhow::Result<()>) -> KiteBuf {
    KiteBuf::from_vec(r.err().map(|e| e.to_string()).unwrap_or_default().into_bytes())
}

/// `child`: a child's profile, with the parent password (see `create_child_profile`).
#[no_mangle]
pub unsafe extern "C" fn kite_profile_create(name: *const c_char, child: std::ffi::c_int, password: *const c_char) -> KiteBuf {
    if child != 0 {
        return error_buf(create_child_profile(s(name), s(password)));
    }
    error_buf(may_manage_profiles().and_then(|_| kite_core::profiles::Profiles::open()).and_then(|p| p.create(s(name))))
}

#[no_mangle]
pub unsafe extern "C" fn kite_profile_delete(name: *const c_char) -> KiteBuf {
    error_buf(may_manage_profiles().and_then(|_| kite_core::profiles::Profiles::open()).and_then(|p| p.delete(s(name))))
}

#[no_mangle]
pub unsafe extern "C" fn kite_profile_rename(from: *const c_char, to: *const c_char) -> KiteBuf {
    error_buf(may_manage_profiles().and_then(|_| kite_core::profiles::Profiles::open()).and_then(|p| p.rename(s(from), s(to))))
}

/// A profile's picture: `len` bytes of a PNG file, or none to remove it. A child's
/// profile's is anyone's to change; another's is the parent's, once one is a child's.
#[no_mangle]
pub unsafe extern "C" fn kite_profile_set_avatar(name: *const c_char, png: *const u8, len: usize) -> KiteBuf {
    let name = s(name);
    let png = (!png.is_null() && len > 0).then(|| std::slice::from_raw_parts(png, len));
    let allowed = if parental().is_some_and(|p| p.is_child(name)) { Ok(()) } else { may_manage_profiles() };
    error_buf(allowed.and_then(|_| kite_core::profiles::Profiles::open()).and_then(|p| p.set_avatar(name, png)))
}

/// The file of a profile's picture, "" without one (for the pictures Kite's windows show).
#[no_mangle]
pub unsafe extern "C" fn kite_profile_avatar_path(name: *const c_char) -> KiteBuf {
    let name = s(name);
    let path = profiles().and_then(|p| p.list().into_iter().find(|i| i.name == name)).map(|i| i.avatar).unwrap_or_default();
    KiteBuf::from_vec(path.into_bytes())
}

#[no_mangle]
pub extern "C" fn kite_profiles_ask() -> i32 {
    profiles().is_none_or(|p| p.choices().ask) as i32
}

#[no_mangle]
pub extern "C" fn kite_profiles_set_ask(ask: i32) {
    if let Some(p) = profiles() {
        let _ = p.set_choices(&kite_core::profiles::Choices { ask: ask != 0, ..p.choices() });
    }
}

#[no_mangle]
pub unsafe extern "C" fn kite_profile_chosen(name: *const c_char) {
    let name = s(name);
    if let Some(p) = profiles() {
        let _ = p.set_choices(&kite_core::profiles::Choices { last: name.to_owned(), ..p.choices() });
    }
    // The parent password was given here: it isn't asked again for the profile chosen.
    let pass = parental().filter(|p| PARENT_PRESENT.load(std::sync::atomic::Ordering::Relaxed) && p.needs_password(Some(name))).and_then(|p| p.grant_pass());
    if PICK_OPENS.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(exe) = crate::own_executable() {
            let mut start = std::process::Command::new(exe);
            start.args(["--profile", name]);
            if let Some(pass) = pass {
                start.env(kite_core::parental::PASS_VARIABLE, pass);
            }
            let _ = start.spawn();
        }
    } else {
        println!("{name}\n{}", pass.unwrap_or_default());
    }
}

// ---- printing (cpp/print.cpp, browser.rs) ---------------------------------

extern "C" {
    pub fn kite_cpp_print_dialog(title: *const c_char);
    pub fn kite_cpp_print_submit();
    pub fn kite_cpp_print_cancel();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_pages_only_take_a_web_pages_address() {
        assert_eq!(web_address("https://kde.org/a?b=1"), "https://kde.org/a?b=1");
        assert_eq!(web_address("http://localhost:8080/"), "http://localhost:8080/");
        for other in ["javascript:alert(1)", "JaVaScRiPt:alert(1)", "java\tscript:alert(1)", "kite:start?add=x.y", "kite:config", "file:///etc/passwd", "data:text/html,x", "about:blank", "", "kde.org", "https://"] {
            assert_eq!(web_address(other), "", "{other}");
        }
    }

    #[test]
    fn the_pages_with_something_to_guard_stand_alone() {
        for alone in ["kite:config", "kite:welcome", "kite:blocked?why=listed&url=https://kde.org/", "kite:dangerous?url=x", "kite:https-only?url=x", "kite:config#x"] {
            assert!(stands_alone(alone), "{alone}");
        }
        // The others share an origin: the start page's form goes to kite:search, a copied
        // start page keeps its storage and loads its own files.
        for together in ["kite:start", "kite:newtab", "kite:search?q=x", "kite:startpage/home/", "kite:reader?id=1", "kite:favicon?url=x", "kite:configuration", "moz-extension://a/config", "https://kde.org/config"] {
            assert!(!stands_alone(together), "{together}");
        }
    }

    #[test]
    fn an_extension_files_path_is_decoded_once() {
        let path = |address: &str| extension_path(&url::Url::parse(address).unwrap());
        assert_eq!(path("moz-extension://a/img/a%20b.png").as_deref(), Some("img/a b.png"));
        assert_eq!(path("moz-extension://a/page.html?x=1#y").as_deref(), Some("page.html"));
        // What is checked against web_accessible_resources is what is served: a "?" or
        // "#" that only shows once decoded isn't the end of the path, it is no file's path.
        assert_eq!(path("moz-extension://a/page.html%3F.png"), None);
        assert_eq!(path("moz-extension://a/page.html%23.png"), None);
        assert_eq!(path("moz-extension://a/img%5C..%5Cpage.html"), None);
        // Once, not until nothing is left to decode.
        assert_eq!(path("moz-extension://a/%2541.png").as_deref(), Some("%41.png"));
    }

    #[test]
    fn the_blocked_page_links_to_the_web_or_nowhere() {
        // Waiting for the lists: it goes back to the site by itself, and by a link.
        let page = blocked_page(&web_address("https://kde.org/"), "no-lists");
        assert!(page.contains(r#"href="https://kde.org/">Try again"#) && page.contains(r#"content="3;url=https://kde.org/""#));
        // Any web page can name an address here: nothing but a web page's is taken.
        for evil in ["javascript:alert(document.domain)", "kite:start?add=evil.example&title=Bank"] {
            let page = blocked_page(&web_address(evil), "no-lists");
            assert!(!page.contains("alert(") && !page.contains("kite:start") && !page.contains("http-equiv=\"refresh\""), "{evil}");
            assert!(page.contains("Go back"));
        }
        // And no script but the page's own runs, whatever is in a link.
        for why in ["no-lists", "listed", ""] {
            let page = blocked_page("https://kde.org/", why);
            assert!(page.contains("script-src 'nonce-") && !page.contains("javascript:"), "{why}");
        }
    }
}
