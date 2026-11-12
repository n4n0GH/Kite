//! The browser as a media player to the desktop: MPRIS, which Plasma's media
//! controls, media keys and KDE Connect all go through.
//! <https://specifications.freedesktop.org/mpris-spec/latest/>
//!
//! It's on the bus only while something is playing or paused, or the desktop would
//! show an empty player. D-Bus runs on its own thread, so that it can't hold up the
//! interface; what's asked of the player arrives on another one, through `Handler`.

use super::players::{Changes, Key, Players, Report};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use zbus::blocking::Connection;
use zbus::names::WellKnownName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";
const NAME: &str = "org.mpris.MediaPlayer2.kite";
const DESKTOP_ENTRY: &str = "io.github.n4n0gh.kite";

/// What the desktop asks of the player, for the page to do.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Command {
    pub tab: i32,
    pub frame: String,
    pub cmd: &'static str,
    /// Seconds for `seek` and `setPosition`, otherwise the volume or the rate.
    pub value: f64,
}

pub type Handler = Arc<dyn Fn(Command) + Send + Sync>;

#[derive(Clone)]
struct Shared {
    players: Arc<Mutex<Players>>,
    handler: Handler,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Players> {
        self.players.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn send(&self, cmd: &'static str, value: f64) {
        let key = self.lock().active_key().cloned();
        if let Some(Key { tab, frame }) = key {
            (self.handler)(Command { tab, frame, cmd, value });
        }
    }
}

struct Root(Shared);

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        self.0.send("raise", 0.0);
    }
    /// Not asked of a browser's media controls: `CanQuit` says so.
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Kite"
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        DESKTOP_ENTRY
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player(Shared);

fn value<'a>(v: impl Into<Value<'a>>) -> Option<OwnedValue> {
    v.into().try_to_owned().ok()
}

fn metadata(players: &Players) -> HashMap<String, OwnedValue> {
    let mut out = HashMap::new();
    let Some(r) = players.active() else { return out };
    let mut put = |key: &str, v: Option<OwnedValue>| {
        if let Some(v) = v {
            out.insert(key.to_owned(), v);
        }
    };
    let track = format!("/org/kde/kite/track/{}", players.track());
    put("mpris:trackid", ObjectPath::try_from(track.as_str()).ok().and_then(value));
    if let Some(d) = r.duration {
        put("mpris:length", value(super::players::micros(d)));
    }
    if !r.title.is_empty() {
        put("xesam:title", value(r.title.as_str()));
    }
    if !r.artist.is_empty() {
        put("xesam:artist", value(vec![r.artist.as_str()]));
    }
    if !r.album.is_empty() {
        put("xesam:album", value(r.album.as_str()));
    }
    if !r.art.is_empty() {
        put("mpris:artUrl", value(r.art.as_str()));
    }
    if !r.url.is_empty() {
        put("xesam:url", value(r.url.as_str()));
    }
    out
}

fn status(r: Option<&Report>) -> &'static str {
    match r {
        Some(r) if r.playing => "Playing",
        Some(_) => "Paused",
        None => "Stopped",
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.0.send("next", 0.0);
    }
    fn previous(&self) {
        self.0.send("previous", 0.0);
    }
    fn pause(&self) {
        self.0.send("pause", 0.0);
    }
    fn play_pause(&self) {
        self.0.send("playPause", 0.0);
    }
    fn stop(&self) {
        self.0.send("stop", 0.0);
    }
    fn play(&self) {
        self.0.send("play", 0.0);
    }
    /// By this far, in microseconds.
    fn seek(&self, offset: i64) {
        self.0.send("seek", offset as f64 / 1_000_000.0);
    }
    /// To there, unless the track has changed since the asker looked.
    fn set_position(&self, track: ObjectPath<'_>, position: i64) {
        let now = format!("/org/kde/kite/track/{}", self.0.lock().track());
        if track.as_str() == now {
            self.0.send("setPosition", position as f64 / 1_000_000.0);
        }
    }
    /// Pages play what they play.
    fn open_uri(&self, _uri: String) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> &str {
        status(self.0.lock().active())
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.0.lock())
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.0.lock().active().map_or(1.0, |r| r.rate)
    }
    #[zbus(property)]
    fn set_rate(&self, rate: f64) {
        // (0 means pause, as the specification has it.)
        if rate == 0.0 {
            self.0.send("pause", 0.0);
        } else if rate.is_finite() {
            self.0.send("setRate", rate.clamp(self.minimum_rate(), self.maximum_rate()));
        }
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        0.25
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        4.0
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.0.lock().active().map_or(1.0, |r| if r.muted { 0.0 } else { r.volume })
    }
    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        if volume.is_finite() {
            self.0.send("setVolume", volume.clamp(0.0, 1.0));
        }
    }
    /// Worked out when asked. It's up to the asker to keep it running, and `Seeked`
    /// tells it of jumps, so this one changes without a signal.
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.0.lock().position(Instant::now())
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.lock().active().is_some_and(|r| r.next)
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.lock().active().is_some_and(|r| r.previous)
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.0.lock().active().is_some()
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.0.lock().active().is_some()
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.lock().active().is_some_and(|r| r.duration.is_some())
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

enum Message {
    Report(Key, Option<Report>),
    ForgetTab(i32),
}

/// Talking to the bus, on its own thread.
struct Bus {
    connection: Connection,
    /// The name it's on the bus by, while it is.
    name: Option<WellKnownName<'static>>,
}

impl Bus {
    fn connect(shared: &Shared) -> zbus::Result<Self> {
        let connection = zbus::blocking::connection::Builder::session()?
            .serve_at(PATH, Root(shared.clone()))?
            .serve_at(PATH, Player(shared.clone()))?
            .build()?;
        Ok(Self { connection, name: None })
    }

    fn appear(&mut self) -> zbus::Result<()> {
        use zbus::fdo::{RequestNameFlags, RequestNameReply};
        if self.name.is_some() {
            return Ok(());
        }
        // Another Kite, say on another profile, has the plain name.
        for name in [NAME.to_owned(), format!("{NAME}.instance{}", std::process::id())] {
            let name = WellKnownName::try_from(name)?;
            let got = self.connection.request_name_with_flags(name.clone(), RequestNameFlags::DoNotQueue.into());
            match got {
                Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
                    self.name = Some(name);
                    return Ok(());
                }
                Ok(_) | Err(zbus::Error::NameTaken) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(zbus::Error::NameTaken)
    }

    fn go(&mut self) -> zbus::Result<()> {
        if let Some(name) = self.name.take() {
            self.connection.release_name(name)?;
        }
        Ok(())
    }

    fn tell(&self, changes: &Changes) -> zbus::Result<()> {
        let player = self.connection.object_server().interface::<_, Player>(PATH)?;
        zbus::block_on(async {
            let emitter = player.signal_emitter();
            let iface = player.get();
            if changes.status {
                iface.playback_status_changed(emitter).await?;
            }
            if changes.metadata {
                iface.metadata_changed(emitter).await?;
                iface.can_seek_changed(emitter).await?;
            }
            if changes.rate {
                iface.rate_changed(emitter).await?;
            }
            if changes.volume {
                iface.volume_changed(emitter).await?;
            }
            if changes.controls {
                iface.can_go_next_changed(emitter).await?;
                iface.can_go_previous_changed(emitter).await?;
                iface.can_seek_changed(emitter).await?;
            }
            if let Some(position) = changes.seeked {
                Player::seeked(emitter, position).await?;
            }
            Ok(())
        })
    }
}

pub struct Mpris {
    to_bus: Sender<Message>,
}

impl Mpris {
    /// `handler` gets what the desktop asks for, on a thread of its own.
    pub fn start(handler: impl Fn(Command) + Send + Sync + 'static) -> Self {
        let (to_bus, from_browser) = channel();
        let shared = Shared { players: Arc::default(), handler: Arc::new(handler) };
        std::thread::Builder::new()
            .name("kite-mpris".into())
            .spawn(move || {
                let mut bus = None;
                let mut warned = false;
                for message in from_browser {
                    let now = Instant::now();
                    let changes = match message {
                        Message::Report(key, report) => shared.lock().report(key, report.map(Report::clean), now),
                        Message::ForgetTab(tab) => shared.lock().forget_tab(tab, now),
                    };
                    if changes == Changes::default() {
                        continue;
                    }
                    if bus.is_none() {
                        match Bus::connect(&shared) {
                            Ok(b) => bus = Some(b),
                            Err(e) => {
                                if !warned {
                                    log::warn!("media controls: no session bus: {e}");
                                }
                                warned = true;
                                continue;
                            }
                        }
                    }
                    let Some(b) = bus.as_mut() else { continue };
                    // On the bus with all it has to say, off it without being asked.
                    let result = if changes.appeared {
                        b.appear()
                    } else if changes.gone {
                        b.go()
                    } else {
                        b.tell(&changes)
                    };
                    if let Err(e) = result {
                        log::warn!("media controls: {e}");
                    }
                }
            })
            .expect("media controls thread");
        Self { to_bus }
    }

    pub fn report(&self, tab: i32, frame: String, report: Option<Report>) {
        let _ = self.to_bus.send(Message::Report(Key { tab, frame }, report));
    }

    pub fn forget_tab(&self, tab: i32) {
        let _ = self.to_bus.send(Message::ForgetTab(tab));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `kde:pid` means "I stand in for that process, hide its players". Plasma would
    /// hide this one, which is that process. The taskbar goes by the owner's pid anyway.
    #[test]
    fn does_not_claim_to_stand_in_for_itself() {
        let mut players = Players::default();
        let report = Report { title: "t".into(), playing: true, ..Default::default() };
        players.report(Key { tab: 1, frame: "f".into() }, Some(report), Instant::now());
        let metadata = metadata(&players);
        assert!(metadata.contains_key("xesam:title"));
        assert!(!metadata.contains_key("kde:pid"));
    }
}
