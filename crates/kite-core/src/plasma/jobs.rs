//! Downloads as Plasma's jobs: in the notification area with their progress, a way
//! to pause or cancel them, and when they're done, Plasma's own Open button.
//!
//! The server is `org.kde.JobViewServerV2`, in plasmashell. Elsewhere there's none:
//! `Event::NotShown` then says that a finished download is still to be announced.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::message::{Flags, Message};
use zbus::zvariant::{OwnedObjectPath, Value};

const SERVER: &str = "org.kde.JobViewServer";
const SERVER_PATH: &str = "/JobViewServer";
const SERVER_INTERFACE: &str = "org.kde.JobViewServerV2";
const VIEW_INTERFACE: &str = "org.kde.JobViewV3";
const DESKTOP_ENTRY: &str = "io.github.n4n0gh.kite";
// In the capabilities of a view.
const KILLABLE: i32 = 1;
const SUSPENDABLE: i32 = 2;
// How it ended. (KJob::KilledJobError; KIO's "unknown".) Plasma says nothing of a
// job that was cancelled.
const CANCELLED: u32 = 1;
const FAILED: u32 = 100;

/// What's left of a path's characters in a file address. As `url` would have it, and
/// `/` stays.
const PATH: &AsciiSet = &CONTROLS
    .add(b' ').add(b'"').add(b'#').add(b'%').add(b'<').add(b'>').add(b'?').add(b'[').add(b'\\')
    .add(b']').add(b'^').add(b'`').add(b'{').add(b'|').add(b'}');

fn file_url(path: &str) -> String {
    format!("file://{}", utf8_percent_encode(path, PATH))
}

fn folder(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

/// Where it comes from, for the details. Passwords in the address stay out of it,
/// and addresses that are the whole file are nothing to read.
fn source(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else { return String::new() };
    if !matches!(parsed.scheme(), "http" | "https" | "ftp" | "file") {
        return String::new();
    }
    let _ = parsed.set_password(None);
    let _ = parsed.set_username("");
    parsed.to_string().chars().take(500).collect()
}

/// How it ended.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum End {
    Complete,
    Cancelled,
    Failed,
}

/// What Plasma, or its absence, has to say about a download.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: i32,
    /// "cancel", "pause", "resume"; or "notShown": it's complete, and with nothing
    /// to show it in.
    pub event: &'static str,
}

/// Bytes per second, from one report to the next.
#[derive(Default)]
struct Speed {
    last: Option<(Instant, i64)>,
    speed: u64,
}

impl Speed {
    fn update(&mut self, received: i64, now: Instant) -> u64 {
        match self.last {
            None => self.last = Some((now, received)),
            Some((then, before)) => {
                let elapsed = now.duration_since(then);
                // (Reports can bunch up: over too short a time it means nothing.)
                if elapsed >= Duration::from_millis(250) {
                    let got = received.saturating_sub(before).max(0) as f64;
                    self.speed = (got / elapsed.as_secs_f64()) as u64;
                    self.last = Some((now, received));
                }
            }
        }
        self.speed
    }
}

type Properties<'a> = HashMap<&'static str, Value<'a>>;

/// Plasma offers Open when there's one file, named by the second description.
fn first_properties(path: &str, url: &str) -> Properties<'static> {
    let mut p = Properties::new();
    p.insert("title", Value::from("Downloading".to_owned()));
    p.insert("destUrl", Value::from(file_url(folder(path))));
    p.insert("totalFiles", Value::from(1u64));
    let from = source(url);
    if !from.is_empty() {
        p.insert("descriptionLabel1", Value::from("Source".to_owned()));
        p.insert("descriptionValue1", Value::from(from));
    }
    p.insert("descriptionLabel2", Value::from("Destination".to_owned()));
    p.insert("descriptionValue2", Value::from(file_url(path)));
    p
}

fn progress_properties(received: i64, total: i64, speed: u64) -> Properties<'static> {
    let mut p = Properties::new();
    p.insert("processedBytes", Value::from(received.max(0) as u64));
    p.insert("speed", Value::from(speed));
    if total > 0 {
        p.insert("totalBytes", Value::from(total as u64));
        p.insert("percent", Value::from((received.clamp(0, total) as f64 / total as f64 * 100.0) as u32));
    }
    p
}

fn end_properties(received: i64) -> Properties<'static> {
    let mut p = Properties::new();
    p.insert("processedBytes", Value::from(received.max(0) as u64));
    p.insert("totalBytes", Value::from(received.max(0) as u64));
    p.insert("processedFiles", Value::from(1u64));
    p.insert("percent", Value::from(100u32));
    p
}

enum ToBus {
    Start { id: i32, path: String, url: String },
    Progress { id: i32, received: i64, total: i64 },
    Paused { id: i32, paused: bool },
    Finish { id: i32, end: End, received: i64, error: String },
    ForgetAll(Sender<()>),
}

struct View {
    path: OwnedObjectPath,
    speed: Speed,
}

type Handler = Arc<dyn Fn(Event) + Send + Sync>;

/// Talking to the server, on its own thread.
struct Bus {
    connection: Connection,
    /// To tell whose job a signal is about.
    paths: Arc<Mutex<HashMap<OwnedObjectPath, i32>>>,
}

impl Bus {
    fn connect(handler: Handler) -> zbus::Result<Self> {
        let connection = Connection::session()?;
        let paths: Arc<Mutex<HashMap<OwnedObjectPath, i32>>> = Arc::default();
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(SERVER)?
            .interface(VIEW_INTERFACE)?
            .build();
        let messages = zbus::blocking::MessageIterator::for_match_rule(rule, &connection, None)?;
        let known = paths.clone();
        std::thread::Builder::new()
            .name("kite-jobs-signals".into())
            .spawn(move || {
                for message in messages.flatten() {
                    let header = message.header();
                    let (Some(path), Some(member)) = (header.path(), header.member()) else { continue };
                    let event = match member.as_str() {
                        "cancelRequested" => "cancel",
                        "suspendRequested" => "pause",
                        "resumeRequested" => "resume",
                        _ => continue,
                    };
                    let id = known.lock().unwrap_or_else(|e| e.into_inner()).get(&OwnedObjectPath::from(path.to_owned())).copied();
                    if let Some(id) = id {
                        handler(Event { id, event });
                    }
                }
            })
            .map_err(|e| zbus::Error::Failure(e.to_string()))?;
        Ok(Self { connection, paths })
    }

    fn request_view(&self, id: i32, path: &str, url: &str) -> zbus::Result<OwnedObjectPath> {
        let mut hints = first_properties(path, url);
        hints.insert("immediate", Value::from(true));
        // (Should it not be installed, its desktop file wouldn't say.)
        hints.insert("application-display-name", Value::from("Kite".to_owned()));
        hints.insert("application-icon-name", Value::from(DESKTOP_ENTRY.to_owned()));
        let reply = self.connection.call_method(
            Some(SERVER),
            SERVER_PATH,
            Some(SERVER_INTERFACE),
            "requestView",
            &(DESKTOP_ENTRY, KILLABLE | SUSPENDABLE, &hints),
        )?;
        let view: OwnedObjectPath = reply.body().deserialize()?;
        self.paths.lock().unwrap_or_else(|e| e.into_inner()).insert(view.clone(), id);
        Ok(view)
    }

    /// (Neither of these is answered: they're marked so in the interface.)
    fn send(&self, view: &OwnedObjectPath, method: &str, body: &(impl Serialize + zbus::zvariant::DynamicType)) -> zbus::Result<()> {
        let message = Message::method_call(view.as_str(), method)?
            .destination(SERVER)?
            .interface(VIEW_INTERFACE)?
            .with_flags(Flags::NoReplyExpected)?
            .build(body)?;
        self.connection.send(&message)
    }

    fn update(&self, view: &OwnedObjectPath, properties: &Properties<'_>) -> zbus::Result<()> {
        self.send(view, "update", &(properties,))
    }

    fn terminate(&self, view: &OwnedObjectPath, code: u32, message: &str) -> zbus::Result<()> {
        self.paths.lock().unwrap_or_else(|e| e.into_inner()).remove(view);
        self.send(view, "terminate", &(code, message, HashMap::<&str, Value<'_>>::new()))
    }
}

pub struct Jobs {
    to_bus: Sender<ToBus>,
}

impl Jobs {
    /// `handler` hears from the desktop, on a thread of its own.
    pub fn start(handler: impl Fn(Event) + Send + Sync + 'static) -> Self {
        let (to_bus, from_browser) = channel();
        let handler: Handler = Arc::new(handler);
        std::thread::Builder::new()
            .name("kite-jobs".into())
            .spawn(move || {
                let mut bus: Option<Bus> = None;
                let mut views: HashMap<i32, View> = HashMap::new();
                let check = |r: zbus::Result<()>| {
                    if let Err(e) = r {
                        log::debug!("download jobs: {e}");
                    }
                };
                for message in from_browser {
                    match message {
                        ToBus::Start { id, path, url } => {
                            if bus.is_none() {
                                match Bus::connect(handler.clone()) {
                                    Ok(b) => bus = Some(b),
                                    Err(e) => log::debug!("download jobs: {e}"),
                                }
                            }
                            let Some(b) = &bus else { continue };
                            match b.request_view(id, &path, &url) {
                                Ok(view) => {
                                    views.insert(id, View { path: view, speed: Speed::default() });
                                }
                                // No Plasma here, or none at the moment.
                                Err(e) => log::debug!("download jobs: {e}"),
                            }
                        }
                        ToBus::Progress { id, received, total } => {
                            let (Some(b), Some(view)) = (&bus, views.get_mut(&id)) else { continue };
                            let speed = view.speed.update(received, Instant::now());
                            check(b.update(&view.path, &progress_properties(received, total, speed)));
                        }
                        ToBus::Paused { id, paused } => {
                            let (Some(b), Some(view)) = (&bus, views.get(&id)) else { continue };
                            check(b.update(&view.path, &Properties::from([("suspended", Value::from(paused))])));
                        }
                        ToBus::Finish { id, end, received, error } => {
                            let view = views.remove(&id);
                            match (&bus, view) {
                                (Some(b), Some(view)) => check(match end {
                                    End::Complete => b.update(&view.path, &end_properties(received)).and_then(|_| b.terminate(&view.path, 0, "")),
                                    End::Cancelled => b.terminate(&view.path, CANCELLED, ""),
                                    End::Failed => b.terminate(&view.path, FAILED, &error),
                                }),
                                _ if end == End::Complete => handler(Event { id, event: "notShown" }),
                                _ => {}
                            }
                        }
                        ToBus::ForgetAll(done) => {
                            if let Some(b) = &bus {
                                for (_, view) in views.drain() {
                                    check(b.terminate(&view.path, CANCELLED, ""));
                                }
                            }
                            let _ = done.send(());
                        }
                    }
                }
            })
            .expect("download jobs thread");
        Self { to_bus }
    }

    pub fn started(&self, id: i32, path: String, url: String) {
        let _ = self.to_bus.send(ToBus::Start { id, path, url });
    }
    pub fn progress(&self, id: i32, received: i64, total: i64) {
        let _ = self.to_bus.send(ToBus::Progress { id, received, total });
    }
    pub fn paused(&self, id: i32, paused: bool) {
        let _ = self.to_bus.send(ToBus::Paused { id, paused });
    }
    pub fn finished(&self, id: i32, end: End, received: i64, error: String) {
        let _ = self.to_bus.send(ToBus::Finish { id, end, received, error });
    }
    /// Takes down what's running, and waits a moment for that to be done with: the
    /// plugin is going, or Kite is.
    pub fn forget_all(&self) {
        let (done, wait) = channel();
        if self.to_bus.send(ToBus::ForgetAll(done)).is_ok() {
            let _ = wait.recv_timeout(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_addresses_are_encoded() {
        assert_eq!(file_url("/home/a/plain.png"), "file:///home/a/plain.png");
        assert_eq!(file_url("/home/a/my file #1 100%?.zip"), "file:///home/a/my%20file%20%231%20100%25%3F.zip");
        assert_eq!(file_url("/home/ä/ü.txt"), "file:///home/%C3%A4/%C3%BC.txt");
    }

    #[test]
    fn the_folder_of_a_file() {
        assert_eq!(folder("/home/a/b.png"), "/home/a");
        assert_eq!(folder("/b.png"), "/");
    }

    #[test]
    fn the_source_keeps_no_passwords_or_whole_files() {
        assert_eq!(source("https://me:secret@example.com/a.zip"), "https://example.com/a.zip");
        assert_eq!(source("data:text/plain,hello"), "");
        assert_eq!(source("blob:https://example.com/1234"), "");
        assert_eq!(source("not an address"), "");
        assert!(source(&format!("https://example.com/{}", "a".repeat(2000))).chars().count() <= 500);
    }

    #[test]
    fn plasma_can_offer_to_open_it() {
        let p = first_properties("/home/a/my file.zip", "https://example.com/x");
        assert_eq!(p["totalFiles"], Value::from(1u64));
        assert_eq!(p["descriptionValue2"], Value::from("file:///home/a/my%20file.zip".to_owned()));
        assert_eq!(p["destUrl"], Value::from("file:///home/a".to_owned()));
    }

    #[test]
    fn progress_without_a_known_size_has_no_percentage() {
        assert!(!progress_properties(10, -1, 0).contains_key("percent"));
        assert!(!progress_properties(10, 0, 0).contains_key("totalBytes"));
        assert_eq!(progress_properties(50, 200, 0)["percent"], Value::from(25u32));
        // (Servers do lie about the size.)
        assert_eq!(progress_properties(500, 200, 0)["percent"], Value::from(100u32));
    }

    #[test]
    fn speed_over_time() {
        let mut s = Speed::default();
        let t = Instant::now();
        assert_eq!(s.update(0, t), 0);
        assert_eq!(s.update(500, t + Duration::from_millis(100)), 0, "too soon to tell");
        assert_eq!(s.update(1000, t + Duration::from_secs(1)), 1000);
        assert_eq!(s.update(1000, t + Duration::from_secs(2)), 0);
    }
}
