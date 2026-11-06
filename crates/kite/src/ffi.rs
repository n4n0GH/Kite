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
    fn kite_cpp_copy_text(text: *const c_char);
    fn kite_cpp_site_of(host: *const c_char) -> *const c_char;
    fn kite_cpp_cert_fingerprint(error: *const std::ffi::c_void) -> *const c_char;
    fn kite_cpp_webengine_version() -> *const c_char;
    fn kite_cpp_chromium_version() -> *const c_char;
    fn kite_cpp_chromium_security_patch() -> *const c_char;
    fn kite_cpp_save_favicon(page_url: *const c_char, icon_url: *const c_char);
    fn kite_cpp_favicon_track(view_name: *const c_char, track: bool);
    fn kite_cpp_key_text(key: std::ffi::c_int, modifiers: std::ffi::c_int) -> *const c_char;
    fn kite_cpp_key_native(sequence: *const c_char) -> *const c_char;
    fn kite_cpp_set_wheel_pixels(px: std::ffi::c_int);
    fn kite_cpp_system_wheel_pixels() -> std::ffi::c_int;
}

/// Cache the favicon a page is showing (`icon_url` = image://favicon/...).
pub fn save_favicon(page_url: &str, icon_url: &str) {
    let (Ok(p), Ok(i)) = (CString::new(page_url), CString::new(icon_url)) else { return };
    unsafe { kite_cpp_save_favicon(p.as_ptr(), i.as_ptr()) }
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

pub fn copy_text(text: &str) {
    let Ok(text) = CString::new(text) else { return };
    unsafe { kite_cpp_copy_text(text.as_ptr()) }
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
pub unsafe extern "C" fn kite_serve(url: *const c_char, initiator: *const c_char, in_private: std::ffi::c_int, status: *mut i32, mime: *mut KiteBuf) -> KiteBuf {
    let (code, ty, body) = serve(s(url), s(initiator), in_private != 0);
    *status = code;
    *mime = KiteBuf::from_vec(ty.into_bytes());
    KiteBuf::from_vec(body)
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
            "dangerous" => (200, "text/html".into(), dangerous_page(&query_param(url, "url"), &query_param(url, "threat")).into_bytes()),
            // Every setting. It holds what's needed to change them, so no web page gets it.
            "config" if initiator.is_empty() || initiator.starts_with("kite:") => {
                (200, "text/html".into(), kite_core::config::page(&crate::browser::config_entries(), crate::state::config_token(), &kite_core::config::secret()).into_bytes())
            }
            "config" => (403, "text/plain".into(), vec![]),
            // HTTPS-only: the site has no working https version.
            "https-only" => (200, "text/html".into(), https_only_page(&query_param(url, "url")).into_bytes()),
            _ => (404, "text/html".into(), b"<h1>Not found</h1>".to_vec()),
        };
    }
    let Ok(u) = url::Url::parse(url) else { return (400, "text/plain".into(), vec![]) };
    let uuid = u.host_str().unwrap_or("");
    let path = u.path().trim_start_matches('/').to_owned();
    let rt = app().runtime();
    let Some(ext) = rt.registry.by_uuid(uuid).filter(|e| e.info.enabled) else {
        return (404, "text/plain".into(), b"Extension not found".to_vec());
    };
    // Same-extension or top-level navigations may load anything; other
    // origins only web_accessible_resources (Firefox semantics).
    let same_origin = initiator.is_empty() || initiator == "null" || initiator.trim_end_matches('/').eq_ignore_ascii_case(ext.base_url().trim_end_matches('/'));
    if path == scripts::API_PATH {
        if !same_origin {
            return (403, "text/plain".into(), vec![]);
        }
        let token = rt.token_of(&ext.info.id).unwrap_or("").to_owned();
        return (200, "application/javascript".into(), scripts::page_api_js(ext, &token, qwebchannel_js(), rt.blocking.port()).into_bytes());
    }
    if path == scripts::BACKGROUND_PAGE {
        return match scripts::background_page(ext) {
            Some(html) => (200, "text/html".into(), html.into_bytes()),
            None => (404, "text/plain".into(), vec![]),
        };
    }
    if !same_origin {
        let allowed = ext.manifest.web_accessible_resources.iter().any(|g| {
            let g = g.trim_start_matches('/');
            regex::Regex::new(&kite_core::webext::matchpattern::glob_to_regex(g)).map(|r| r.is_match(&path)).unwrap_or(false)
        });
        if !allowed {
            return (403, "text/plain".into(), vec![]);
        }
    }
    let Some(file) = ext.resolve(&path) else { return (403, "text/plain".into(), vec![]) };
    let file = if file.is_dir() { file.join("index.html") } else { file };
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&file).first_or_octet_stream().essence_str().to_owned();
            let mime = if path.ends_with(".mjs") { "application/javascript".into() } else { mime };
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
        Err(_) => (404, "text/plain".into(), b"Not found".to_vec()),
    }
}

/// `__kite__/rpc` and `__kite__/poll` for extension pages in sub-frames.
/// Returns 1 if the request is held (completed later via
/// `kite_cpp_rpc_complete`), else 0 with `status`/`out` filled in.
#[no_mangle]
pub unsafe extern "C" fn kite_rpc(url: *const c_char, initiator: *const c_char, body: *const u8, body_len: usize, job: u64, status: *mut i32, out: *mut KiteBuf) -> i32 {
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
        match rt.registry.by_uuid(u.host_str().unwrap_or("")).filter(|e| e.info.enabled) {
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

/// Where Kite itself sends a page load instead: to a warning about a dangerous site,
/// without tracking parameters, over https (privacy.*).
fn builtin_page_redirect(url: &str) -> Option<String> {
    let a = app();
    if let Some(threat) = a.safe_browsing.check(url) {
        log::info!("safebrowsing: {} listed as {}", url, threat.as_str());
        let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
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

/// What extensions' webRequest listeners want done with a request.
fn ask_web_request_listeners(url: &str, first_party: &str, initiator: &str, rtype: &str, method: &str, in_private: bool) -> ListenersSay {
    let a = app();
    let mut rt = a.runtime();
    if !rt.has_web_request_listeners() {
        return ListenersSay::Nothing;
    }
    let tab = rt.tab_for_first_party(first_party, in_private);
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
    let (fx, ask) = rt.web_request(&details, in_private);
    let hub = rt.blocking.clone();
    drop(rt);
    crate::extensions::apply_effects_async(fx);
    // Never ask extensions about their own requests: the background
    // page would be waiting for itself.
    let from_extension = initiator.starts_with("moz-extension:") || first_party.starts_with("moz-extension:");
    if ask.is_empty() || from_extension {
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

/// Where a page load should go instead, or "". For WebTab's onNavigationRequested,
/// as a redirect from the request interceptor never finishes on a page a service
/// worker answers for: the page just stays blank. Before the load starts, there's
/// nothing to get in the way.
pub fn page_load_redirect(url: &str, in_private: bool) -> String {
    if !(url.starts_with("http:") || url.starts_with("https:")) || kite_core::webext::amo::is_amo_download(url) {
        return String::new();
    }
    if let Some(to) = builtin_page_redirect(url) {
        return to;
    }
    match ask_web_request_listeners(url, "", "", "mainFrame", "GET", in_private) {
        ListenersSay::Redirect(to) if to != url => to,
        // (Blocking is the interceptor's: it asks again.)
        ListenersSay::Block | ListenersSay::Redirect(_) => String::new(),
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
        // Never interfere with add-on installs from AMO.
        if kite_core::webext::amo::is_amo_download(url) {
            return String::new();
        }
        // Page loads: what onNavigationRequested already went through isn't asked again.
        let decided = rtype == "mainFrame" && take_decided_page_load(url);
        if rtype == "mainFrame" && !decided {
            if let Some(to) = builtin_page_redirect(url) {
                return format!("redirect:{to}");
            }
        }
        // (From a private window, only extensions allowed there are involved.)
        let in_private = in_private != 0;
        // webRequest listeners
        if !decided {
            match ask_web_request_listeners(url, first_party, initiator, rtype, method, in_private) {
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

/// Qt/QML log output. QML categories `console.page` / `console.extension`
/// become log targets `console::page` / `console::extension`; other Qt
/// categories `qt::<category>`.
#[no_mangle]
pub unsafe extern "C" fn kite_log(level: std::ffi::c_int, category: *const c_char, message: *const c_char) {
    let level = match level {
        1 => log::Level::Error,
        2 => log::Level::Warn,
        3 => log::Level::Info,
        _ => log::Level::Debug,
    };
    let category = s(category);
    let target = match category {
        c if c.starts_with("console.") => c.replace('.', "::"),
        "default" => "qt".to_owned(),
        c => format!("qt::{c}"),
    };
    log::log!(target: target.as_str(), level, "{}", s(message));
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

/// Should this user agent override apply (AMO sees Kite as Firefox so it
/// offers "Add to Firefox" buttons that lead to .xpi files).
#[no_mangle]
pub unsafe extern "C" fn kite_user_agent_for(url: *const c_char) -> KiteBuf {
    let url = s(url);
    let spoof = app().shared.settings().get_bool("extensions.amoSpoofFirefox");
    if spoof && kite_core::util::host_of(url) == "addons.mozilla.org" {
        return KiteBuf::from_vec(kite_core::webext::amo::USER_AGENT.as_bytes().to_vec());
    }
    if kite_core::util::is_google_sign_in(url) && app().shared.settings().get_bool("web.googleSignIn") {
        return KiteBuf::from_vec(kite_core::util::FIREFOX_USER_AGENT.as_bytes().to_vec());
    }
    KiteBuf::empty()
}

fn query_param(url: &str, name: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned())).unwrap_or_default()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The page shown when HTTPS-only couldn't load a site over https.
fn https_only_page(http_url: &str) -> String {
    let host = html_escape(&kite_core::util::host_of(http_url));
    let allow = format!("kite:https-allow?url={}", url::form_urlencoded::byte_serialize(http_url.as_bytes()).collect::<String>());
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Secure connection not available</title>
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
<div class="buttons"><a class="button primary" href="javascript:history.back()">Go back</a><a class="button" href="{allow}">Continue to the HTTP site</a></div>
</body></html>"#,
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
    let allow = format!("kite:dangerous-allow?url={}", url::form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>());
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>{title}</title>
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
<div class="buttons"><a class="button primary" href="javascript:history.back()">Go back</a><a class="button" href="{allow}">Continue anyway</a></div>
</body></html>"#,
        allow = html_escape(&allow)
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

#[no_mangle]
pub extern "C" fn kite_profiles_json() -> KiteBuf {
    let json = profiles().map(|p| serde_json::json!({"profiles": p.list(), "last": p.choices().last}).to_string()).unwrap_or_default();
    KiteBuf::from_vec(json.into_bytes())
}

fn error_buf(r: anyhow::Result<()>) -> KiteBuf {
    KiteBuf::from_vec(r.err().map(|e| e.to_string()).unwrap_or_default().into_bytes())
}

#[no_mangle]
pub unsafe extern "C" fn kite_profile_create(name: *const c_char) -> KiteBuf {
    error_buf(kite_core::profiles::Profiles::open().and_then(|p| p.create(s(name))))
}

#[no_mangle]
pub unsafe extern "C" fn kite_profile_delete(name: *const c_char) -> KiteBuf {
    error_buf(kite_core::profiles::Profiles::open().and_then(|p| p.delete(s(name))))
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
    if PICK_OPENS.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(exe) = crate::own_executable() {
            let _ = std::process::Command::new(exe).args(["--profile", name]).spawn();
        }
    } else {
        println!("{name}");
    }
}

// ---- printing (cpp/print.cpp, browser.rs) ---------------------------------

extern "C" {
    pub fn kite_cpp_print_dialog(title: *const c_char);
    pub fn kite_cpp_print_submit();
    pub fn kite_cpp_print_cancel();
}
