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
    fn kite_cpp_qwebchannel_js() -> *const c_char;
    fn kite_cpp_save_favicon(page_url: *const c_char, icon_url: *const c_char);
}

/// Cache the favicon a page is showing (`icon_url` = image://favicon/...).
pub fn save_favicon(page_url: &str, icon_url: &str) {
    let (Ok(p), Ok(i)) = (CString::new(page_url), CString::new(icon_url)) else { return };
    unsafe { kite_cpp_save_favicon(p.as_ptr(), i.as_ptr()) }
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

pub fn qwebchannel_js() -> &'static str {
    app().qwebchannel_js.get_or_init(|| unsafe { s(kite_cpp_qwebchannel_js()).to_owned() })
}

/// Serve `moz-extension://` and `kite:` URLs. Returns the body; sets HTTP-like
/// status and mime type.
#[no_mangle]
pub unsafe extern "C" fn kite_serve(url: *const c_char, initiator: *const c_char, status: *mut i32, mime: *mut KiteBuf) -> KiteBuf {
    let (code, ty, body) = serve(s(url), s(initiator));
    *status = code;
    *mime = KiteBuf::from_vec(ty.into_bytes());
    KiteBuf::from_vec(body)
}

fn serve(url: &str, initiator: &str) -> (i32, String, Vec<u8>) {
    if url.starts_with(kite_core::standin::SCHEME) {
        return match STAND_INS.get(url) {
            Some((mime, body)) => (200, mime, body),
            None => (404, "text/plain".into(), vec![]),
        };
    }
    if let Some(rest) = url.strip_prefix("kite:") {
        let page = rest.split(['?', '#']).next().unwrap_or("");
        return match page {
            "start" | "speeddial" | "newtab" | "home" => (200, "text/html".into(), crate::startpage::render().into_bytes()),
            "search" => {
                // kite:search?q=... from the start page form
                let q = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.query_pairs().find(|(k, _)| k == "q").map(|(_, v)| v.into_owned()))
                    .unwrap_or_default();
                let target = crate::browser::resolve_input(&q);
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

/// Network request interceptor. Returns "" (allow), "block",
/// "redirect:<url>" or "headers:<json [[name, value], ...]>".
#[no_mangle]
pub unsafe extern "C" fn kite_intercept(
    url: *const c_char,
    first_party: *const c_char,
    initiator: *const c_char,
    rtype: *const c_char,
    method: *const c_char,
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
        let mut rt = a.runtime();
        // webRequest listeners
        if rt.has_web_request_listeners() {
            let tab = rt.tab_for_first_party(first_party);
            static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let frame = kite_core::webext::blocking::frame_context(url, rtype, first_party, initiator);
            let mut details = serde_json::json!({
                "requestId": REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string(), "url": url, "method": method,
                "type": kite_core::webext::dnr::resource_type(rtype), "tabId": tab, "frameId": frame.frame_id,
                "parentFrameId": frame.parent_frame_id, "originUrl": initiator, "frameAncestors": frame.frame_ancestors,
                "timeStamp": kite_core::util::now_ms(), "thirdParty": kite_core::util::host_of(url) != kite_core::util::host_of(first_party),
                "incognito": a.private, "cookieStoreId": "firefox-default",
            });
            if let Some(doc) = frame.document_url {
                details["documentUrl"] = doc.into();
            }
            let (fx, ask) = rt.web_request(&details);
            let hub = rt.blocking.clone();
            drop(rt);
            crate::extensions::apply_effects_async(fx);
            // Never ask extensions about their own requests: the background
            // page would be waiting for itself.
            let from_extension = initiator.starts_with("moz-extension:") || first_party.starts_with("moz-extension:");
            if !ask.is_empty() && !from_extension {
                let t0 = std::time::Instant::now();
                let replies = hub.ask(&ask, "onBeforeRequest", &details, BLOCKING_TIMEOUT);
                log::trace!("webRequest: asked {ask:?} about {url} (tab {tab}, {rtype}) in {:?}: {replies:?}", t0.elapsed());
                let d = kite_core::webext::blocking::combine(&replies);
                if d.cancel {
                    log::debug!("webRequest: blocked {url}");
                    return "block".into();
                }
                if let Some((ext, to)) = d.redirect {
                    log::debug!("webRequest: {ext} redirected {url} to {to}");
                    return stand_in(&to, rtype);
                }
            }
            rt = a.runtime();
        }
        match rt.dnr_check(url, if initiator.is_empty() { first_party } else { initiator }, method, rtype) {
            kite_core::webext::dnr::Outcome::Block => return "block".into(),
            kite_core::webext::dnr::Outcome::Redirect(u) => return format!("redirect:{u}"),
            kite_core::webext::dnr::Outcome::Headers(h) => return format!("headers:{}", serde_json::to_string(&h).unwrap_or_default()),
            kite_core::webext::dnr::Outcome::None => {}
        }
        // An extension like uBlock Origin does the content blocking.
        if rt.content_blocker().is_some() {
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

/// Send the Do Not Track header ("web.doNotTrack").
#[no_mangle]
pub extern "C" fn kite_do_not_track() -> bool {
    app().do_not_track.load(std::sync::atomic::Ordering::Relaxed)
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
    KiteBuf::empty()
}
