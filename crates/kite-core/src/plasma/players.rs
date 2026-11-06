//! What's playing in the browser's pages, and which of it the desktop controls.
//!
//! Every frame with media reports what it has. The desktop gets one player, as with
//! Plasma Integration in other browsers: what started playing last, or if nothing is
//! playing, what played last.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// A frame's report, from the page. Its text is whatever the page says it is.
#[derive(Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Report {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art: String,
    /// The address of the tab.
    pub url: String,
    /// What the media element is playing, to tell one track from the next.
    pub src: String,
    pub playing: bool,
    /// Seconds. None: not known, or a live stream.
    pub duration: Option<f64>,
    pub position: f64,
    pub rate: f64,
    pub volume: f64,
    pub muted: bool,
    pub next: bool,
    pub previous: bool,
}

const MAX_TEXT: usize = 1000;
const MAX_ADDRESS: usize = 8192;

fn cut(text: &mut String, max: usize) {
    if text.len() > max {
        let mut at = max;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
    }
}

fn finite(value: f64, otherwise: f64) -> f64 {
    if value.is_finite() { value } else { otherwise }
}

impl Report {
    /// Within what the desktop should be handed. In particular, artwork comes from the
    /// web only: the desktop would open any address it's given, as a `file:` one.
    pub fn clean(mut self) -> Self {
        cut(&mut self.title, MAX_TEXT);
        cut(&mut self.artist, MAX_TEXT);
        cut(&mut self.album, MAX_TEXT);
        cut(&mut self.url, MAX_ADDRESS);
        cut(&mut self.src, MAX_ADDRESS);
        if !(self.art.starts_with("https://") || self.art.starts_with("http://")) || self.art.len() > MAX_ADDRESS {
            self.art.clear();
        }
        self.position = finite(self.position, 0.0).max(0.0);
        self.duration = self.duration.filter(|d| d.is_finite() && *d > 0.0);
        self.rate = finite(self.rate, 1.0);
        if self.rate <= 0.0 {
            self.rate = 1.0;
        }
        self.volume = finite(self.volume, 1.0).clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Key {
    pub tab: i32,
    pub frame: String,
}

struct Entry {
    key: Key,
    report: Report,
    at: Instant,
    /// When it last started or stopped playing, in order of those.
    played: u64,
}

/// What changed for the desktop.
#[derive(Default, PartialEq, Debug)]
pub struct Changes {
    pub status: bool,
    pub metadata: bool,
    pub rate: bool,
    pub volume: bool,
    pub controls: bool,
    /// Jumped to here (microseconds), not getting there by playing.
    pub seeked: Option<i64>,
    /// There's a player now, or there no longer is.
    pub appeared: bool,
    pub gone: bool,
}

#[derive(Default)]
pub struct Players {
    entries: Vec<Entry>,
    order: u64,
    track: u64,
}

// How far off counts as a jump: reports aren't exact, and buffering holds it back.
const JUMP_SECONDS: f64 = 1.5;

pub(super) fn micros(seconds: f64) -> i64 {
    (seconds * 1_000_000.0) as i64
}

fn position_at(e: &Entry, now: Instant) -> f64 {
    let r = &e.report;
    let played = if r.playing { now.saturating_duration_since(e.at).as_secs_f64() * r.rate } else { 0.0 };
    let at = r.position + played;
    r.duration.map_or(at, |d| at.min(d))
}

impl Players {
    fn active_entry(&self) -> Option<&Entry> {
        self.entries.iter().max_by_key(|e| (e.report.playing, e.played))
    }
    pub fn active(&self) -> Option<&Report> {
        self.active_entry().map(|e| &e.report)
    }
    pub fn active_key(&self) -> Option<&Key> {
        self.active_entry().map(|e| &e.key)
    }
    /// Changes whenever the track does, so that the desktop can tell.
    pub fn track(&self) -> u64 {
        self.track
    }
    pub fn position(&self, now: Instant) -> i64 {
        self.active_entry().map_or(0, |e| micros(position_at(e, now)))
    }

    /// A frame's report, or None: it has no media any more.
    pub fn report(&mut self, key: Key, report: Option<Report>, now: Instant) -> Changes {
        self.apply(now, |p| {
            let found = p.entries.iter().position(|e| e.key == key);
            match (report, found) {
                (Some(r), Some(i)) => {
                    let e = &mut p.entries[i];
                    // (Stopping too: what was just paused stays the player, so that
                    // the button to start it again is still there.)
                    if r.playing != e.report.playing {
                        p.order += 1;
                        e.played = p.order;
                    }
                    e.report = r;
                    e.at = now;
                }
                (Some(r), None) => {
                    p.order += 1;
                    p.entries.push(Entry { key, report: r, at: now, played: p.order });
                }
                (None, Some(i)) => {
                    p.entries.remove(i);
                }
                (None, None) => {}
            }
        })
    }

    /// The tab closed.
    pub fn forget_tab(&mut self, tab: i32, now: Instant) -> Changes {
        self.apply(now, |p| p.entries.retain(|e| e.key.tab != tab))
    }

    fn apply(&mut self, now: Instant, change: impl FnOnce(&mut Self)) -> Changes {
        let before = self.active_entry().map(|e| (e.key.clone(), e.report.clone(), position_at(e, now)));
        change(self);
        let Some(entry) = self.active_entry() else {
            return Changes { gone: before.is_some(), ..Default::default() };
        };
        let (key, new, position) = (entry.key.clone(), entry.report.clone(), position_at(entry, now));
        let Some((old_key, old, expected)) = before else {
            self.track += 1;
            return Changes { appeared: true, status: true, metadata: true, rate: true, volume: true, controls: true, ..Default::default() };
        };
        let other_track = old_key != key || old.src != new.src;
        if other_track {
            self.track += 1;
        }
        Changes {
            status: old.playing != new.playing,
            metadata: other_track || old.title != new.title || old.artist != new.artist || old.album != new.album || old.art != new.art
                || old.url != new.url || old.duration != new.duration,
            rate: old.rate != new.rate,
            volume: old.volume != new.volume || old.muted != new.muted,
            controls: old.next != new.next || old.previous != new.previous || (old.duration.is_some() != new.duration.is_some()),
            seeked: (other_track || (position - expected).abs() > JUMP_SECONDS).then(|| micros(position)),
            appeared: false,
            gone: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key(tab: i32) -> Key {
        Key { tab, frame: "f".into() }
    }
    fn playing(title: &str) -> Report {
        Report { title: title.into(), playing: true, rate: 1.0, volume: 1.0, duration: Some(100.0), ..Default::default() }
    }
    fn paused(title: &str) -> Report {
        Report { playing: false, ..playing(title) }
    }

    #[test]
    fn what_started_playing_last_is_the_player() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(key(1), Some(playing("a")), t);
        p.report(key(2), Some(playing("b")), t);
        assert_eq!(p.active().unwrap().title, "b");
        // It started again later than b did.
        p.report(key(1), Some(paused("a")), t);
        p.report(key(1), Some(playing("a")), t);
        assert_eq!(p.active().unwrap().title, "a");
    }

    #[test]
    fn something_playing_goes_before_what_was_paused() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(key(1), Some(playing("a")), t);
        p.report(key(2), Some(playing("b")), t);
        p.report(key(2), Some(paused("b")), t);
        assert_eq!(p.active().unwrap().title, "a");
        p.report(key(1), Some(paused("a")), t);
        assert_eq!(p.active().unwrap().title, "a", "of the paused, the one playing last");
    }

    #[test]
    fn a_player_appears_and_goes() {
        let (mut p, t) = (Players::default(), Instant::now());
        let c = p.report(key(1), Some(playing("a")), t);
        assert!(c.appeared && c.metadata && c.status);
        assert!(!p.report(key(1), Some(playing("a")), t).appeared);
        assert_eq!(p.report(key(1), None, t), Changes { gone: true, ..Default::default() });
        assert!(p.active().is_none());
        assert_eq!(p.report(key(1), None, t), Changes::default(), "nothing to go");
    }

    #[test]
    fn a_closed_tab_goes_with_all_its_frames() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(Key { tab: 1, frame: "x".into() }, Some(playing("a")), t);
        p.report(Key { tab: 1, frame: "y".into() }, Some(playing("b")), t);
        p.report(key(2), Some(paused("c")), t);
        let c = p.forget_tab(1, t);
        assert!(!c.gone && c.metadata);
        assert_eq!(p.active().unwrap().title, "c");
        assert!(p.forget_tab(2, t).gone);
    }

    #[test]
    fn nothing_changed_is_no_change() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(key(1), Some(paused("a")), t);
        assert_eq!(p.report(key(1), Some(paused("a")), t + Duration::from_secs(5)), Changes::default());
    }

    #[test]
    fn position_runs_while_playing_at_its_rate_up_to_the_end() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(key(1), Some(Report { position: 10.0, rate: 2.0, ..playing("a") }), t);
        assert_eq!(p.position(t + Duration::from_secs(3)), 16_000_000);
        assert_eq!(p.position(t + Duration::from_secs(1000)), 100_000_000);
        p.report(key(1), Some(Report { position: 16.0, ..paused("a") }), t + Duration::from_secs(3));
        assert_eq!(p.position(t + Duration::from_secs(50)), 16_000_000);
    }

    #[test]
    fn a_jump_is_told_and_playing_on_is_not() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(key(1), Some(Report { position: 10.0, ..playing("a") }), t);
        let later = t + Duration::from_secs(5);
        assert_eq!(p.report(key(1), Some(Report { position: 15.2, ..playing("a") }), later).seeked, None);
        assert_eq!(p.report(key(1), Some(Report { position: 60.0, ..playing("a") }), later).seeked, Some(60_000_000));
    }

    #[test]
    fn another_track_is_a_new_one_for_the_desktop() {
        let (mut p, t) = (Players::default(), Instant::now());
        p.report(key(1), Some(Report { src: "one".into(), ..playing("a") }), t);
        let first = p.track();
        p.report(key(1), Some(Report { src: "one".into(), ..paused("a") }), t);
        assert_eq!(p.track(), first);
        let c = p.report(key(1), Some(Report { src: "two".into(), position: 0.0, ..playing("a") }), t);
        assert!(c.metadata && c.seeked.is_some());
        assert_ne!(p.track(), first);
        // Going to another tab's is one too.
        let before = p.track();
        p.report(key(2), Some(playing("b")), t);
        assert_ne!(p.track(), before);
    }

    #[test]
    fn what_a_page_says_is_kept_in_bounds() {
        let r = Report { title: "é".repeat(MAX_TEXT), rate: -1.0, volume: 7.0, position: f64::NAN, duration: Some(f64::INFINITY), ..Default::default() }.clean();
        assert!(r.title.len() <= MAX_TEXT && r.title.is_char_boundary(r.title.len()));
        assert_eq!((r.rate, r.volume, r.position, r.duration), (1.0, 1.0, 0.0, None));
    }

    #[test]
    fn artwork_comes_from_the_web_only() {
        let art = |a: &str| Report { art: a.into(), ..Default::default() }.clean().art;
        assert_eq!(art("https://x.example/a.png"), "https://x.example/a.png");
        assert_eq!(art("http://x.example/a.png"), "http://x.example/a.png");
        for bad in ["file:///etc/passwd", "kite:config", "data:image/png;base64,AA", "/a.png", "smb://x/y"] {
            assert_eq!(art(bad), "", "{bad}");
        }
    }
}
