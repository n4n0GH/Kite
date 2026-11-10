//! `YtDownloader` — the only thing the YouTube Download plugin can run, and all
//! it can run is yt-dlp. Narrowed on purpose: plugins are fully trusted
//! (docs/plugins.md), so a general "run a program" door would hand a malicious
//! plugin a shell. Here Rust picks the program (yt-dlp on PATH, never a path a
//! plugin chose), validates the URL to a YouTube video it rebuilds itself, and
//! builds every argument, so the worst a plugin can do is save a YouTube video.
//!
//! The program runs in a process group of its own, so stopping it stops what it
//! started (yt-dlp's ffmpeg); dropping the object stops everything.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type YtDownloader = super::YtDownloaderRust;

        /// Looks for yt-dlp, ffmpeg and a JavaScript runtime; `tools` follows.
        #[qinvokable]
        #[cxx_name = "checkTools"]
        fn check_tools(self: Pin<&mut YtDownloader>);

        /// Starts saving the YouTube video at `url` in `quality` (one of
        /// best|1080|720|480|audio) to `folder`. False when yt-dlp is missing,
        /// the URL isn't a YouTube video, or one is already running for it.
        #[qinvokable]
        fn start(self: Pin<&mut YtDownloader>, url: &QString, quality: &QString, folder: &QString) -> bool;
        /// Stops the download of `url` (and what it started).
        #[qinvokable]
        fn stop(self: Pin<&mut YtDownloader>, url: &QString);

        /// yt-dlp (its path, or "" when missing), its version (or ""), and
        /// whether ffmpeg is installed.
        #[qsignal]
        fn tools(self: Pin<&mut YtDownloader>, found: QString, version: QString, ffmpeg: bool);
        /// Bytes so far and in total (0 when not yet known), for `url`.
        #[qsignal]
        fn progress(self: Pin<&mut YtDownloader>, url: QString, done: f64, total: f64);
        /// The file `url` was saved to.
        #[qsignal]
        fn saved(self: Pin<&mut YtDownloader>, url: QString, path: QString);
        /// Something yt-dlp reported going wrong for `url`.
        #[qsignal]
        fn failed(self: Pin<&mut YtDownloader>, url: QString, message: QString);
        /// The download of `url` ended: yt-dlp's exit code, or -1 when it was
        /// stopped or crashed.
        #[qsignal]
        fn finished(self: Pin<&mut YtDownloader>, url: QString, code: i32);
    }

    impl cxx_qt::Threading for YtDownloader {}
}

#[derive(Default)]
pub struct YtDownloaderRust {
    // url (canonical) -> the process group leader's pid, while it runs.
    active: HashMap<String, u32>,
}

impl Drop for YtDownloaderRust {
    fn drop(&mut self) {
        for pid in self.active.values() {
            stop_group(*pid);
        }
    }
}

fn stop_group(pid: u32) {
    // SAFETY: a signal to the process group the child leads (made in start()).
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// The first `name` on PATH, by name only: a plugin never chooses a path here.
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| executable(p)))
}

/// The canonical address of the one video a YouTube URL shows (its id is 11 of
/// `A-Za-z0-9_-`), rebuilt by us so only ever `https://www.youtube.com/watch?v=<id>`
/// reaches yt-dlp — never whatever a page or plugin passed. None for anything else.
pub fn canonical_video(url: &str) -> Option<String> {
    fn id_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '-'
    }
    // The 11 characters after a marker, if exactly 11 id characters are there.
    fn take_id(s: &str) -> Option<&str> {
        let end = s.char_indices().take_while(|(_, c)| id_char(*c)).map(|(i, c)| i + c.len_utf8()).last().unwrap_or(0);
        let id = &s[..end];
        (id.len() == 11).then_some(id)
    }
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    // host/...
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).or_else(|| host.strip_prefix("music.")).unwrap_or(&host);

    let id = if host == "youtube.com" {
        if let Some(q) = path.strip_prefix("watch?") {
            // v=<id> among &-separated pairs.
            q.split('&').find_map(|kv| kv.strip_prefix("v=")).and_then(take_id)
        } else if let Some(r) = path.strip_prefix("shorts/").or_else(|| path.strip_prefix("live/")) {
            take_id(r)
        } else {
            None
        }
    } else if host == "youtu.be" {
        take_id(path)
    } else {
        None
    }?;
    Some(format!("https://www.youtube.com/watch?v={id}"))
}

const PROGRESS: &str = "kite-progress";
const SAVED: &str = "kite-saved";

/// yt-dlp's arguments for `url` (already canonical) to `folder` in `quality`.
/// Without ffmpeg, video and audio can't be joined (or audio converted), so it
/// takes the best single file. `js_runtime` is a runtime other than deno for it
/// to use on YouTube ("node"), or "". Every argument is built here.
fn build_args(url: &str, folder: &str, quality: &str, ffmpeg: bool, js_runtime: &str) -> Vec<String> {
    let s = |x: &str| x.to_owned();
    let mut a = vec![
        s("--newline"),
        s("--no-playlist"),
        s("--progress"),
        s("--progress-template"),
        format!("download:{PROGRESS} %(progress.downloaded_bytes)s %(progress.total_bytes,progress.total_bytes_estimate)s"),
        s("--print"),
        format!("after_move:{SAVED} %(filepath)s"),
        s("-P"),
        s(folder),
        s("-o"),
        s("%(title)s [%(id)s].%(ext)s"),
    ];
    if !js_runtime.is_empty() {
        a.push(s("--js-runtimes"));
        a.push(s(js_runtime));
    }
    let height = matches!(quality, "1080" | "720" | "480").then_some(quality);
    if quality == "audio" {
        a.extend(if ffmpeg { ["-f", "ba/b", "-x"].iter() } else { ["-f", "ba[ext=m4a]/ba/b"].iter() }.map(|x| s(x)));
    } else if let Some(h) = height {
        a.push(s("-f"));
        a.push(if ffmpeg { format!("bv*[height<={h}]+ba/b[height<={h}]/b") } else { format!("b[height<={h}]/b") });
    } else if !ffmpeg {
        a.push(s("-f"));
        a.push(s("b"));
    }
    // The URL is positional and built by us, but keep it past `--` all the same.
    a.push(s("--"));
    a.push(s(url));
    a
}

/// What a line yt-dlp printed means, for the queued readers below.
enum Line {
    Progress(f64, f64),
    Saved(String),
    Error(String),
}
fn read_line(line: &str) -> Option<Line> {
    if let Some(rest) = line.strip_prefix(&format!("{PROGRESS} ")) {
        let mut it = rest.split(' ').map(|n| n.parse::<f64>().unwrap_or(0.0));
        return Some(Line::Progress(it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)));
    }
    if let Some(rest) = line.strip_prefix(&format!("{SAVED} ")) {
        return Some(Line::Saved(rest.to_owned()));
    }
    if let Some(rest) = line.strip_prefix("ERROR: ") {
        // Drop yt-dlp's "[youtube] <id>: " lead-in.
        let msg = rest.splitn(2, ": ").last().unwrap_or(rest);
        return Some(Line::Error(msg.to_owned()));
    }
    None
}

/// Reads lines (ended by \n or by \r, as progress lines are) until the end.
fn each_line(from: impl Read, mut each: impl FnMut(&str)) {
    let mut r = BufReader::new(from);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                for part in String::from_utf8_lossy(&buf).split(['\n', '\r']) {
                    if !part.is_empty() {
                        each(part);
                    }
                }
            }
        }
    }
}

impl qobject::YtDownloader {
    fn check_tools(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let yt = on_path("yt-dlp");
            let ffmpeg = on_path("ffmpeg").is_some();
            let version = yt
                .as_ref()
                .and_then(|p| Command::new(p).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().ok())
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .unwrap_or_default();
            let found = yt.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = thread.queue(move |o| o.tools(QString::from(&found), QString::from(&version), ffmpeg));
        });
    }

    fn start(mut self: Pin<&mut Self>, url: &QString, quality: &QString, folder: &QString) -> bool {
        let Some(url) = canonical_video(&url.to_string()) else {
            log::warn!("YtDownloader.start: not a YouTube video URL");
            return false;
        };
        if self.active.contains_key(&url) {
            return false;
        }
        let Some(yt_dlp) = on_path("yt-dlp") else {
            log::warn!("YtDownloader.start: yt-dlp isn't installed");
            return false;
        };
        let quality = quality.to_string();
        if !matches!(quality.as_str(), "best" | "1080" | "720" | "480" | "audio") {
            log::warn!("YtDownloader.start: unknown quality {quality}");
            return false;
        }
        let ffmpeg = on_path("ffmpeg").is_some();
        // yt-dlp finds deno itself; node it has to be told of.
        let js_runtime = if on_path("deno").is_none() && on_path("node").is_some() { "node" } else { "" };
        let args = build_args(&url, &folder.to_string(), &quality, ffmpeg, js_runtime);

        let mut child = match Command::new(&yt_dlp)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("YtDownloader.start: yt-dlp didn't start: {e}");
                return false;
            }
        };
        let pid = child.id();
        self.as_mut().rust_mut().active.insert(url.clone(), pid);

        let mut readers = Vec::new();
        for stream in [
            child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
            child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let thread = self.qt_thread();
            let url = url.clone();
            readers.push(std::thread::spawn(move || {
                each_line(stream, |line| match read_line(line) {
                    Some(Line::Progress(done, total)) => {
                        let url = url.clone();
                        let _ = thread.queue(move |o| o.progress(QString::from(&url), done, total));
                    }
                    Some(Line::Saved(path)) => {
                        let url = url.clone();
                        let _ = thread.queue(move |o| o.saved(QString::from(&url), QString::from(&path)));
                    }
                    Some(Line::Error(msg)) => {
                        let url = url.clone();
                        let _ = thread.queue(move |o| o.failed(QString::from(&url), QString::from(&msg)));
                    }
                    None => {}
                })
            }));
        }
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            for r in readers {
                let _ = r.join();
            }
            let _ = thread.queue(move |mut o| {
                o.as_mut().rust_mut().active.remove(&url);
                o.finished(QString::from(&url), code);
            });
        });
        true
    }

    fn stop(self: Pin<&mut Self>, url: &QString) {
        if let Some(url) = canonical_video(&url.to_string()) {
            if let Some(pid) = self.active.get(&url) {
                stop_group(*pid);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_youtube_video_urls_survive() {
        let v = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        assert_eq!(canonical_video("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(), Some(v));
        assert_eq!(canonical_video("https://www.youtube.com/watch?list=PL1&v=dQw4w9WgXcQ&t=2").as_deref(), Some(v));
        assert_eq!(canonical_video("https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=42s").as_deref(), Some(v));
        assert_eq!(canonical_video("https://music.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(), Some(v));
        assert_eq!(canonical_video("https://youtube.com/shorts/dQw4w9WgXcQ").as_deref(), Some(v));
        assert_eq!(canonical_video("https://www.youtube.com/live/dQw4w9WgXcQ?si=x").as_deref(), Some(v));
        assert_eq!(canonical_video("https://youtu.be/dQw4w9WgXcQ?t=1").as_deref(), Some(v));
        for no in [
            "https://www.youtube.com/",
            "https://www.youtube.com/@someone",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQx",
            "https://notyoutube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            // No shell could ever reach these: they don't survive at all.
            "https://youtu.be/a;rm -rf ~",
            "https://www.youtube.com/watch?v=$(id)",
            "file:///etc/passwd",
            "",
        ] {
            assert_eq!(canonical_video(no), None, "{no}");
        }
    }

    #[test]
    fn built_args_put_the_url_last_and_pick_formats() {
        let a = build_args("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "/home/u/Downloads", "best", true, "");
        assert_eq!(&a[a.len() - 2..], &["--".to_owned(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_owned()]);
        assert!(a.iter().any(|x| x == "--no-playlist"));
        assert_eq!(a[a.iter().position(|x| x == "-P").unwrap() + 1], "/home/u/Downloads");
        assert!(!a.iter().any(|x| x == "-f"));
        let fmt = |q, ff| {
            let a = build_args("u", "d", q, ff, "");
            a.iter().position(|x| x == "-f").map(|i| a[i + 1].clone())
        };
        assert_eq!(fmt("best", false).as_deref(), Some("b"));
        assert_eq!(fmt("720", true).as_deref(), Some("bv*[height<=720]+ba/b[height<=720]/b"));
        assert_eq!(fmt("720", false).as_deref(), Some("b[height<=720]/b"));
        assert!(build_args("u", "d", "audio", true, "").iter().any(|x| x == "-x"));
        assert!(!build_args("u", "d", "audio", false, "").iter().any(|x| x == "-x"));
        assert_eq!(build_args("u", "d", "best", true, "node")[build_args("u", "d", "best", true, "node").iter().position(|x| x == "--js-runtimes").unwrap() + 1], "node");
    }

    #[test]
    fn reads_what_yt_dlp_prints() {
        assert!(matches!(read_line(&format!("{PROGRESS} 1024 4096")), Some(Line::Progress(d, t)) if d == 1024.0 && t == 4096.0));
        assert!(matches!(read_line(&format!("{PROGRESS} 1024 NA")), Some(Line::Progress(d, t)) if d == 1024.0 && t == 0.0));
        assert!(matches!(read_line(&format!("{SAVED} /home/u/A [b].mp4")), Some(Line::Saved(p)) if p == "/home/u/A [b].mp4"));
        assert!(matches!(read_line("ERROR: [youtube] dQw4w9WgXcQ: Video unavailable"), Some(Line::Error(m)) if m == "Video unavailable"));
        assert!(read_line("[download] Destination: x").is_none());
        let mut lines = Vec::new();
        each_line(&b"one\ntwo\r50%\r60%\n\nend"[..], |l| lines.push(l.to_owned()));
        assert_eq!(lines, ["one", "two", "50%", "60%", "end"]);
    }
}
