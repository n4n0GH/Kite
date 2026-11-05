//! API transport for extension pages loaded in sub-frames (e.g. Vimium's
//! Vomnibar iframe inside a web page). Qt's WebChannel only exists in the top
//! frame, so such pages talk to the runtime through their *own origin*:
//!
//! * `POST moz-extension://<uuid>/__kite__/rpc`  — one API call
//! * `GET  moz-extension://<uuid>/__kite__/poll?ctx=…` — long poll for replies
//!   and events (held open by the scheme handler until something arrives)
//!
//! Content scripts in sub-frames use the same endpoints from their isolated
//! world (the scheme ignores the page's CSP). Their requests carry the web
//! page's origin, so they must POST the extension's secret token instead,
//! and nothing secret is ever put in a URL (pages can see request URLs in
//! their resource timing).
//!
//! The traffic never passes through the hosting web page; every call is still
//! authenticated by the extension's token in the runtime.

use crate::state::app;
use kite_core::webext::runtime::Origin;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const VIEW_PREFIX: &str = "rpc:";

#[derive(Default)]
struct Mailboxes {
    /// view -> pending JSON messages
    queued: HashMap<String, Vec<String>>,
    /// view -> held poll request (job id)
    waiting: HashMap<String, u64>,
    /// view -> last time a poll arrived (to expire vanished frames)
    seen: HashMap<String, Instant>,
}

static MAIL: Mutex<Option<Mailboxes>> = Mutex::new(None);

fn with_mail<T>(f: impl FnOnce(&mut Mailboxes) -> T) -> T {
    let mut g = MAIL.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Mailboxes::default))
}

extern "C" {
    fn kite_cpp_rpc_complete(job: u64, data: *const u8, len: usize);
}

fn complete(job: u64, json: &str) {
    unsafe { kite_cpp_rpc_complete(job, json.as_ptr(), json.len()) }
}

/// Called on the Qt thread for Reply/Deliver effects addressed to an rpc view.
pub fn deliver(view: &str, json: String) {
    let ready = with_mail(|m| {
        m.queued.entry(view.to_owned()).or_default().push(json);
        m.waiting.remove(view).map(|job| (job, m.queued.remove(view).unwrap_or_default()))
    });
    if let Some((job, msgs)) = ready {
        complete(job, &format!("[{}]", msgs.join(",")));
    }
}

/// A held poll request went away (frame unloaded or request cancelled).
pub fn job_gone(job: u64) {
    with_mail(|m| m.waiting.retain(|_, j| *j != job));
}

/// Drop frames that stopped polling. Returns the views to close.
pub fn sweep() -> Vec<String> {
    with_mail(|m| {
        let stale: Vec<String> = m
            .seen
            .iter()
            .filter(|(v, t)| !m.waiting.contains_key(*v) && t.elapsed() > Duration::from_secs(20))
            .map(|(v, _)| v.clone())
            .collect();
        for v in &stale {
            m.seen.remove(v);
            m.queued.remove(v);
        }
        stale
    })
}

pub enum Handled {
    /// Respond now with this body (status, JSON).
    Now(i32, String),
    /// The job is held; `deliver` will complete it.
    Held,
}

/// Handle an `__kite__/rpc` or `__kite__/poll` request for extension `ext_id`
/// (whose pages live at `ext_base`).
pub fn handle(path_and_query: &str, ext_id: &str, ext_base: &str, initiator: &str, body: &[u8], job: u64) -> Handled {
    let (path, query) = path_and_query.split_once('?').unwrap_or((path_and_query, ""));
    let from_extension_page = initiator.trim_end_matches('/').eq_ignore_ascii_case(ext_base.trim_end_matches('/'));
    let body: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    // Anything else must be the extension's content script: it knows the token.
    let content_script = !from_extension_page && {
        let rt = app().runtime();
        body["token"].as_str().is_some_and(|t| rt.ext_for_token(t) == Some(ext_id))
    };
    if !from_extension_page && !content_script {
        return Handled::Now(403, "{}".into());
    }
    if path.ends_with("/poll") {
        let ctx = if content_script {
            body["ctx"].as_str().unwrap_or_default().to_owned()
        } else {
            url::form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == "ctx").map(|(_, v)| v.into_owned()).unwrap_or_default()
        };
        if ctx.is_empty() {
            return Handled::Now(400, "{}".into());
        }
        let view = format!("{VIEW_PREFIX}{ctx}");
        let (result, replaced) = with_mail(|m| {
            m.seen.insert(view.clone(), Instant::now());
            match m.queued.remove(&view) {
                Some(msgs) if !msgs.is_empty() => (Handled::Now(200, format!("[{}]", msgs.join(","))), None),
                // A newer poll replaces an older one.
                _ => (Handled::Held, m.waiting.insert(view, job)),
            }
        });
        // Outside the lock: completing may re-enter via job_gone().
        if let Some(old) = replaced {
            complete(old, "[]");
        }
        return result;
    }
    if path.ends_with("/rpc") {
        let v = body;
        let ctx = v["ctx"].as_str().unwrap_or_default().to_owned();
        if ctx.is_empty() {
            return Handled::Now(400, "{}".into());
        }
        let view = format!("{VIEW_PREFIX}{ctx}");
        with_mail(|m| {
            m.seen.entry(view.clone()).or_insert_with(Instant::now);
        });
        let fx = {
            let mut rt = app().runtime();
            let (tab_id, window_id) = match v["top"].as_str() {
                // A content script in a sub-frame: Qt doesn't say which tab
                // it is in, so go by the top frame's origin.
                Some(top) if content_script => {
                    let in_private = rt.ext_for_token(v["token"].as_str().unwrap_or("")).is_some_and(|e| rt.runs_in_private(e));
                    rt.tab_for_origin(top, in_private)
                }
                // UI iframes live in the tab the user is interacting with.
                _ => rt.active_tab(),
            };
            let origin = Origin { view, tab_id, window_id };
            rt.call(&origin, v["token"].as_str().unwrap_or(""), &ctx, v["method"].as_str().unwrap_or(""), v["args"].clone(), v["callId"].as_i64().unwrap_or(0))
        };
        crate::extensions::apply_effects_async(fx);
        return Handled::Now(200, "{}".into());
    }
    Handled::Now(404, "{}".into())
}
