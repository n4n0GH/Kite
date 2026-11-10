//! `Process` — runs a program for a plugin's QML, and hands back what it prints
//! line by line (the YouTube Download plugin runs yt-dlp with it).
//!
//! Plugins are fully trusted (docs/plugins.md), so this is no new power, only a
//! way to do from QML what they could do anyway. The program runs in a process
//! group of its own, so stopping it also stops what it started (yt-dlp's
//! ffmpeg); destroying the object stops it too.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
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
        /// Started and not yet finished.
        #[qproperty(bool, running)]
        type Process = super::ProcessRust;

        /// Runs `program` (a name looked up on PATH, or a path) with `args_json`, a
        /// JSON list of strings. False when it's running already or can't start.
        #[qinvokable]
        fn start(self: Pin<&mut Process>, program: &QString, args_json: &QString) -> bool;
        /// Asks it (and what it started) to stop; `finished` follows.
        #[qinvokable]
        fn stop(self: Pin<&mut Process>);
        /// Where `name` is on PATH ("" when it isn't there or can't be run).
        #[qinvokable]
        #[cxx_name = "findProgram"]
        fn find_program(self: &Process, name: &QString) -> QString;

        /// A line it printed, on standard output or standard error.
        #[qsignal]
        fn output(self: Pin<&mut Process>, line: QString);
        /// It ended: its exit code, or -1 when it was stopped or crashed.
        #[qsignal]
        fn finished(self: Pin<&mut Process>, code: i32);
    }

    impl cxx_qt::Threading for Process {}
}

#[derive(Default)]
pub struct ProcessRust {
    running: bool,
    pid: Option<u32>,
}

impl Drop for ProcessRust {
    fn drop(&mut self) {
        if let Some(pid) = self.pid.take() {
            stop_group(pid);
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

/// `name` as a path to run: as given when it has a slash, else the first on PATH.
pub fn find(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let p = PathBuf::from(name);
        return executable(&p).then_some(p);
    }
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| executable(p)))
}

/// Reads lines (ended by \n or by \r, as progress lines are) until the end.
fn read_lines(from: impl Read, mut each: impl FnMut(String)) {
    let mut r = BufReader::new(from);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                for part in String::from_utf8_lossy(&buf).split(['\n', '\r']) {
                    if !part.is_empty() {
                        each(part.to_owned());
                    }
                }
            }
        }
    }
}

impl qobject::Process {
    fn start(mut self: Pin<&mut Self>, program: &QString, args_json: &QString) -> bool {
        if *self.running() {
            return false;
        }
        let Ok(args) = serde_json::from_str::<Vec<String>>(&args_json.to_string()) else {
            log::warn!("Process.start: the arguments aren't a JSON list of strings");
            return false;
        };
        let Some(path) = find(&program.to_string()) else {
            log::warn!("Process.start: {program} wasn't found");
            return false;
        };
        let child = Command::new(&path)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Process.start: {} didn't start: {e}", path.display());
                return false;
            }
        };
        let pid = child.id();
        self.as_mut().rust_mut().pid = Some(pid);
        self.as_mut().set_running(true);

        let thread = self.qt_thread();
        let mut readers = Vec::new();
        for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)].into_iter().flatten() {
            let thread = self.qt_thread();
            readers.push(std::thread::spawn(move || {
                read_lines(stream, |line| {
                    let _ = thread.queue(move |o| o.output(QString::from(&line)));
                })
            }));
        }
        std::thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            // (Everything it printed comes before the end.)
            for r in readers {
                let _ = r.join();
            }
            let _ = thread.queue(move |mut o| {
                if o.as_ref().pid == Some(pid) {
                    o.as_mut().rust_mut().pid = None;
                    o.as_mut().set_running(false);
                }
                o.finished(code);
            });
        });
        true
    }

    fn stop(self: Pin<&mut Self>) {
        if let Some(pid) = self.pid {
            stop_group(pid);
        }
    }

    fn find_program(&self, name: &QString) -> QString {
        QString::from(&find(&name.to_string()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_programs_and_reads_lines() {
        assert!(super::find("sh").is_some());
        assert!(super::find("kite-no-such-program").is_none());
        assert!(super::find("").is_none());
        let mut lines = Vec::new();
        super::read_lines(&b"one\ntwo\r50%\r60%\n\nend"[..], |l| lines.push(l));
        assert_eq!(lines, ["one", "two", "50%", "60%", "end"]);
    }
}
