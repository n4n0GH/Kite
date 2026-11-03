//! Blocking `webRequest` listeners.
//!
//! The network interceptor runs synchronously on Qt's UI thread, while
//! extension background pages live in renderer processes that normally talk
//! to Kite over QWebChannel — which is also served by the UI thread. So a
//! blocking listener can't be asked over the usual channel without
//! deadlocking.
//!
//! Instead, background pages with blocking listeners keep a WebSocket open to
//! this hub, a tiny localhost server running on its own threads. The
//! interceptor sends each request's details to the extensions that want it
//! and waits (with a timeout) for their verdicts; the replies are read by
//! the per-connection threads and never touch the UI thread.
//!
//! Protocol (JSON text frames):
//! * hub → page: `{"id": n, "event": "onBeforeRequest", "details": {...}}`
//! * page → hub: `{"id": n, "result": {"cancel": true} | {"redirectUrl": ...} | null}`
//!
//! Connections authenticate with the extension's per-session token in the
//! request path (`ws://127.0.0.1:<port>/<token>`).

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::protocol::Role;
use tungstenite::{Message, WebSocket};

struct Conn {
    id: u64,
    ws: Mutex<WebSocket<TcpStream>>,
}

/// A reply from one extension: (extension id, listener result).
pub type Verdict = (String, Value);

#[derive(Default)]
pub struct BlockingHub {
    port: AtomicU16,
    tokens: Mutex<HashMap<String, String>>,
    conns: Mutex<HashMap<String, Vec<Arc<Conn>>>>,
    pending: Mutex<HashMap<u64, (String, mpsc::Sender<Verdict>)>>,
    next: AtomicU64,
    /// Extensions that missed a deadline are skipped until then, so a busy
    /// background page (e.g. still loading its filter lists) doesn't stall
    /// every request by the full timeout.
    slow: Mutex<HashMap<String, Instant>>,
    /// Startup probes: question id -> extension. An extension is ready once
    /// it has answered its probe, which a listener like uBlock Origin's only
    /// does after its filter lists are loaded.
    probes: Mutex<HashMap<u64, String>>,
    ready: Mutex<HashSet<String>>,
}

/// Tab id of the startup probe: one that never exists. (uBlock Origin only
/// holds back requests that belong to a tab.)
const PROBE_TAB: i64 = i32::MAX as i64 - 1;

/// How long an extension that timed out is left out.
const BACKOFF: Duration = Duration::from_secs(1);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl BlockingHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of the listening socket, 0 if not started.
    pub fn port(&self) -> u16 {
        self.port.load(Ordering::Relaxed)
    }

    /// Bind to a random localhost port and start accepting connections.
    pub fn start(self: &Arc<Self>) -> std::io::Result<u16> {
        if self.port() != 0 {
            return Ok(self.port());
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        self.port.store(port, Ordering::Relaxed);
        let hub = self.clone();
        std::thread::Builder::new().name("kite-webrequest".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let hub = hub.clone();
                let _ = std::thread::Builder::new().name("kite-webrequest-conn".into()).spawn(move || hub.serve(stream));
            }
        })?;
        log::debug!("webRequest blocking hub listening on 127.0.0.1:{port}");
        Ok(port)
    }

    pub fn add_token(&self, token: &str, ext: &str) {
        lock(&self.tokens).insert(token.to_owned(), ext.to_owned());
    }

    /// Has the extension answered its startup probe?
    pub fn is_ready(&self, ext: &str) -> bool {
        lock(&self.ready).contains(ext)
    }

    pub fn is_connected(&self, ext: &str) -> bool {
        lock(&self.conns).get(ext).is_some_and(|v| !v.is_empty())
    }

    fn serve(&self, stream: TcpStream) {
        let _ = stream.set_nodelay(true);
        let mut ext = None;
        let accepted = tungstenite::accept_hdr(stream, |req: &Request, resp: Response| {
            let token = req.uri().path().trim_start_matches('/');
            match lock(&self.tokens).get(token) {
                Some(e) => {
                    ext = Some(e.clone());
                    Ok(resp)
                }
                None => {
                    let mut r = ErrorResponse::new(Some("forbidden".into()));
                    *r.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
                    Err(r)
                }
            }
        })
        .ok();
        let (Some(mut reader), Some(ext)) = (accepted, ext) else { return };
        // A second handle on the same socket for writing, so the interceptor
        // can send while this thread is blocked reading.
        let Ok(write_stream) = reader.get_ref().try_clone() else { return };
        let conn = Arc::new(Conn { id: self.next.fetch_add(1, Ordering::Relaxed), ws: Mutex::new(WebSocket::from_raw_socket(write_stream, Role::Server, None)) });
        lock(&self.conns).entry(ext.clone()).or_default().push(conn.clone());
        log::debug!("webRequest: {ext} connected for blocking listeners");
        // Probe with a request nobody blocks to learn when the listeners
        // are ready to answer.
        let probe = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.probes).insert(probe, ext.clone());
        let details = json!({
            "requestId": format!("kite-probe-{probe}"), "url": format!("http://127.0.0.1:{}/kite-probe", self.port()), "method": "GET",
            "type": "xmlhttprequest", "tabId": PROBE_TAB, "frameId": 0, "parentFrameId": -1, "originUrl": "", "documentUrl": "",
            "timeStamp": crate::util::now_ms(), "thirdParty": false, "frameAncestors": [],
        });
        let _ = lock(&conn.ws).send(Message::Text(json!({"id": probe, "event": "onBeforeRequest", "details": details}).to_string().into()));
        let since = Instant::now();
        loop {
            match reader.read() {
                Ok(Message::Text(t)) => {
                    let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                    let Some(id) = v["id"].as_u64() else { continue };
                    if lock(&self.probes).remove(&id).is_some() {
                        log::debug!("webRequest: {ext} ready after {:?}", since.elapsed());
                        lock(&self.ready).insert(ext.clone());
                        continue;
                    }
                    let mut pending = lock(&self.pending);
                    // Only the extension that was asked may answer.
                    if pending.get(&id).is_some_and(|(e, _)| *e == ext) {
                        let (_, tx) = pending.remove(&id).unwrap();
                        let _ = tx.send((ext.clone(), v["result"].clone()));
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
        let mut conns = lock(&self.conns);
        if let Some(v) = conns.get_mut(&ext) {
            v.retain(|c| c.id != conn.id);
            if v.is_empty() {
                conns.remove(&ext);
                lock(&self.ready).remove(&ext);
            }
        }
        log::debug!("webRequest: {ext} disconnected");
    }

    /// Ask the given extensions' blocking `event` listeners about a request
    /// and wait up to `timeout` for all of them. Extensions that aren't
    /// connected or don't answer in time are left out.
    pub fn ask(&self, exts: &[String], event: &str, details: &Value, timeout: Duration) -> Vec<Verdict> {
        let (tx, rx) = mpsc::channel();
        let mut ids = vec![];
        let now = Instant::now();
        lock(&self.slow).retain(|_, until| *until > now);
        for ext in exts {
            if lock(&self.slow).contains_key(ext) {
                continue;
            }
            let conns: Vec<Arc<Conn>> = lock(&self.conns).get(ext).cloned().unwrap_or_default();
            for c in conns {
                let id = self.next.fetch_add(1, Ordering::Relaxed);
                lock(&self.pending).insert(id, (ext.clone(), tx.clone()));
                let msg = json!({"id": id, "event": event, "details": details}).to_string();
                let ok = lock(&c.ws).send(Message::Text(msg.into())).is_ok();
                if ok {
                    ids.push((id, ext.clone()));
                } else {
                    lock(&self.pending).remove(&id);
                }
            }
        }
        drop(tx);
        let deadline = Instant::now() + timeout;
        let mut out = Vec::with_capacity(ids.len());
        while out.len() < ids.len() {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(v) => out.push(v),
                Err(_) => {
                    log::debug!("webRequest: {event} timed out for {}", details["url"].as_str().unwrap_or_default());
                    break;
                }
            }
        }
        // Forget unanswered questions and back off from those extensions.
        if out.len() < ids.len() {
            let mut pending = lock(&self.pending);
            let mut slow = lock(&self.slow);
            for (id, ext) in &ids {
                if pending.remove(id).is_some() {
                    slow.insert(ext.clone(), Instant::now() + BACKOFF);
                }
            }
        }
        out
    }
}

/// What the interceptor should do with a request, combining the replies of
/// all extensions like Firefox: any cancel wins, then the first redirect,
/// then request header changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Decision {
    pub cancel: bool,
    pub redirect: Option<(String, String)>,
    pub request_headers: Option<Vec<(String, String)>>,
}

pub fn combine(replies: &[Verdict]) -> Decision {
    let mut d = Decision::default();
    for (ext, r) in replies {
        if r["cancel"].as_bool() == Some(true) {
            d.cancel = true;
        }
        if d.redirect.is_none() {
            if let Some(u) = r["redirectUrl"].as_str().filter(|u| !u.is_empty()) {
                d.redirect = Some((ext.clone(), u.to_owned()));
            }
        }
        if let Some(h) = r["requestHeaders"].as_array() {
            d.request_headers = Some(
                h.iter()
                    .filter_map(|h| Some((h["name"].as_str()?.to_owned(), h["value"].as_str().unwrap_or_default().to_owned())))
                    .collect(),
            );
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn combine_prefers_cancel_then_first_redirect() {
        let d = combine(&[
            ("a".into(), json!({"redirectUrl": "https://x/"})),
            ("b".into(), json!({"redirectUrl": "https://y/"})),
            ("c".into(), Value::Null),
        ]);
        assert!(!d.cancel);
        assert_eq!(d.redirect, Some(("a".into(), "https://x/".into())));
        assert!(combine(&[("a".into(), json!({"cancel": true}))]).cancel);
        assert_eq!(combine(&[]), Decision::default());
    }

    fn client(port: u16, token: &str) -> WebSocket<TcpStream> {
        let s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        tungstenite::client(format!("ws://127.0.0.1:{port}/{token}"), s).unwrap().0
    }

    #[test]
    fn asks_connected_extension_and_times_out_otherwise() {
        let hub = Arc::new(BlockingHub::new());
        hub.add_token("tok", "ext@x");
        let port = hub.start().unwrap();

        // Unknown token is refused.
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(format!("GET /nope HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").as_bytes()).unwrap();
        let mut buf = [0u8; 12];
        s.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"HTTP/1.1 403");

        // Not connected: nothing asked, no wait.
        let t = Instant::now();
        assert!(hub.ask(&["ext@x".into()], "onBeforeRequest", &json!({}), Duration::from_secs(5)).is_empty());
        assert!(t.elapsed() < Duration::from_secs(1));

        let mut c = client(port, "tok");
        let start = Instant::now();
        while !hub.is_connected("ext@x") && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        // The probe comes first; answering it makes the extension ready.
        let probe: Value = serde_json::from_str(c.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(probe["details"]["tabId"], PROBE_TAB);
        assert!(!hub.is_ready("ext@x"));
        c.send(Message::Text(json!({"id": probe["id"], "result": null}).to_string().into())).unwrap();
        let start = Instant::now();
        while !hub.is_ready("ext@x") && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(hub.is_ready("ext@x"));
        let page = std::thread::spawn(move || {
            let m = c.read().unwrap();
            let v: Value = serde_json::from_str(m.to_text().unwrap()).unwrap();
            assert_eq!(v["details"]["url"], "https://ads.example/");
            c.send(Message::Text(json!({"id": v["id"], "result": {"cancel": true}}).to_string().into())).unwrap();
            c
        });
        let r = hub.ask(&["ext@x".into()], "onBeforeRequest", &json!({"url": "https://ads.example/"}), Duration::from_secs(5));
        assert_eq!(r, vec![("ext@x".to_owned(), json!({"cancel": true}))]);
        let _c = page.join().unwrap();

        // Connected but silent: times out.
        let t = Instant::now();
        assert!(hub.ask(&["ext@x".into()], "onBeforeRequest", &json!({}), Duration::from_millis(50)).is_empty());
        assert!(t.elapsed() >= Duration::from_millis(50));
        // ...and is skipped right after.
        let t = Instant::now();
        assert!(hub.ask(&["ext@x".into()], "onBeforeRequest", &json!({}), Duration::from_millis(50)).is_empty());
        assert!(t.elapsed() < Duration::from_millis(20));
    }
}
