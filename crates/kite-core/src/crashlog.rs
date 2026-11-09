//! Kite's log file and crash reports. They stay on this computer: nothing is
//! sent anywhere.
//!
//! In `<data>/logs/<profile>/` (`private` for a `--private` Kite):
//! * `kite.log`: this run's log (warnings and errors, unless RUST_LOG says more);
//! * `kite.prev.log`: the run before's;
//! * `crash.txt`: written by a run as it crashes (kite's crash.rs);
//! * `last-crash.txt`: the report made of those two at the next start, kept
//!   until the next crash, for the dialog shown then and Help → Last Crash Report.

use std::fs::File;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

pub const LOG: &str = "kite.log";
pub const PREV_LOG: &str = "kite.prev.log";
pub const CRASH: &str = "crash.txt";
pub const REPORT: &str = "last-crash.txt";

/// The log grows to this, then starts over (a page flooding its console can't fill the disk).
pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
/// Lines of the crashed run's log kept in its report.
const REPORT_LOG_LINES: usize = 300;

/// Where a profile's logs go.
pub fn dir(profile: &str, private: bool) -> anyhow::Result<PathBuf> {
    Ok(crate::paths::base_dir()?.join("logs").join(if private { "private" } else { profile }))
}

/// A folder (and files) only the user can read: logs hold addresses.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    if let Some(parent) = dir.parent() {
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn create_private_file(path: &Path, append: bool) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).mode(0o600).open(path)
}

/// The log file of this run. Each line is written as it comes (no buffer), so
/// what led up to a crash is on disk.
pub struct LogFile {
    file: File,
    written: u64,
}

/// What opening the logs found.
pub struct Opened {
    pub log: LogFile,
    /// The last run crashed and left a report (`REPORT`), not yet seen.
    pub new_report: bool,
}

impl LogFile {
    /// Start this run's log: a crash of the last run becomes its report, then
    /// its log becomes the previous one.
    pub fn open(dir: &Path) -> std::io::Result<Opened> {
        create_private_dir(dir)?;
        let new_report = make_report(dir).unwrap_or(false);
        let _ = std::fs::rename(dir.join(LOG), dir.join(PREV_LOG));
        let file = create_private_file(&dir.join(LOG), true)?;
        Ok(Opened { log: LogFile { file, written: 0 }, new_report })
    }

    pub fn write_line(&mut self, line: &str) {
        if self.written + line.len() as u64 > MAX_LOG_BYTES {
            // Starts over, saying so (append mode writes at the new end).
            let _ = self.file.set_len(0);
            let _ = self.file.rewind();
            self.written = 0;
            let note = format!("(The log reached {} MB and started over.)\n", MAX_LOG_BYTES / 1024 / 1024);
            let _ = self.file.write_all(note.as_bytes());
            self.written += note.len() as u64;
        }
        let mut text = String::with_capacity(line.len() + 1);
        text.push_str(line);
        text.push('\n');
        if self.file.write_all(text.as_bytes()).is_ok() {
            self.written += text.len() as u64;
        }
    }
}

/// The last lines of a file.
fn tail(path: &Path, lines: usize) -> String {
    let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// `crash.txt` and the end of that run's log as `REPORT`. Whether there was one.
fn make_report(dir: &Path) -> std::io::Result<bool> {
    let crash = dir.join(CRASH);
    let Ok(what) = std::fs::read(&crash) else { return Ok(false) };
    let when = std::fs::metadata(&crash).and_then(|m| m.modified()).ok().map(crate::util::local_time).unwrap_or_default();
    let mut report = format!("Crash report of Kite, {when}\n\n{}\n", String::from_utf8_lossy(&what).trim_end());
    let log = tail(&dir.join(LOG), REPORT_LOG_LINES);
    report.push_str("\n--- The end of that run's log ---\n");
    report.push_str(if log.is_empty() { "(empty)" } else { &log });
    report.push('\n');
    create_private_file(&dir.join(REPORT), false)?.write_all(report.as_bytes())?;
    std::fs::remove_file(&crash)?;
    Ok(true)
}

/// The report of the last crash, if there is one.
pub fn report(dir: &Path) -> Option<String> {
    std::fs::read(dir.join(REPORT)).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Add a section to the report (the system's view of a native crash).
pub fn append_to_report(dir: &Path, title: &str, text: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().append(true).open(dir.join(REPORT))?;
    f.write_all(format!("\n--- {title} ---\n{}\n", text.trim_end()).as_bytes())
}

/// The process ID a native crash report names ("pid: N"), for asking the
/// system's core dump handler about it.
pub fn crashed_pid(report: &str) -> Option<u32> {
    report.lines().take(20).find_map(|l| l.strip_prefix("pid: ")).and_then(|p| p.trim().parse().ok())
}

/// Whether a report is of a native crash (a signal), not a Rust panic.
pub fn is_native(report: &str) -> bool {
    report.lines().take(6).any(|l| l.starts_with("signal: "))
}

/// Addresses in a log line of a private window, replaced by random text of a
/// random length (not the address's length, which could tell it apart).
pub fn scramble_urls(text: &str) -> String {
    static URL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = URL.get_or_init(|| regex::Regex::new(r#"(?i)\b(?:[a-z][a-z0-9+.\-]*:)?[a-z][a-z0-9+.\-]*://[^\s"'<>()\[\]{}`]+|\b(?:data|blob|about|javascript|mailto):[^\s"'<>()\[\]{}`]+"#).unwrap());
    re.replace_all(text, |c: &regex::Captures| {
        // (Punctuation ending a sentence isn't part of the address.)
        let m = &c[0];
        let kept = m.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        random_word() + &m[kept.len()..]
    })
    .into_owned()
}

fn random_word() -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 25];
    let _ = getrandom::getrandom(&mut bytes);
    let len = 6 + (bytes[0] as usize % 19);
    let word: String = bytes[1..=len].iter().map(|b| CHARS[*b as usize % CHARS.len()] as char).collect();
    format!("url:{word}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_becomes_a_report_once_and_logs_rotate() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("logs/default");
        let mut first = LogFile::open(&d).unwrap();
        assert!(!first.new_report);
        first.log.write_line("run 1 line");
        std::fs::write(d.join(CRASH), "signal: 11 (SIGSEGV)\npid: 1234\n").unwrap();

        let mut second = LogFile::open(&d).unwrap();
        assert!(second.new_report);
        let r = report(&d).unwrap();
        assert!(r.contains("SIGSEGV") && r.contains("run 1 line"), "{r}");
        assert_eq!(crashed_pid(&r), Some(1234));
        assert!(is_native(&r));
        assert!(!d.join(CRASH).exists());
        assert_eq!(std::fs::read_to_string(d.join(PREV_LOG)).unwrap(), "run 1 line\n");
        second.log.write_line("run 2 line");
        append_to_report(&d, "coredumpctl", "stack").unwrap();
        assert!(report(&d).unwrap().ends_with("--- coredumpctl ---\nstack\n"));

        let third = LogFile::open(&d).unwrap();
        assert!(!third.new_report, "shown once");
        assert!(report(&d).is_some(), "kept until the next crash");
        assert_eq!(std::fs::read_to_string(d.join(PREV_LOG)).unwrap(), "run 2 line\n");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(d.join(LOG)).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn a_panic_report_is_not_native() {
        assert!(!is_native("Kite 0.8.2 panicked\nthread: main\nat: src/x.rs:1:2\n"));
        assert_eq!(crashed_pid("Kite panicked\nno pid here"), None);
    }

    #[test]
    fn the_log_starts_over_when_too_big() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = LogFile::open(dir.path()).unwrap().log;
        let line = "x".repeat(1000);
        for _ in 0..(MAX_LOG_BYTES / 1001 + 10) {
            l.write_line(&line);
        }
        let size = std::fs::metadata(dir.path().join(LOG)).unwrap().len();
        assert!(size < 20_000, "{size}");
        assert!(std::fs::read_to_string(dir.path().join(LOG)).unwrap().starts_with("(The log reached 2 MB"));
    }

    #[test]
    fn addresses_are_scrambled_to_other_lengths() {
        let line = "Refused 'https://secret.example/path?q=1' from blob:https://a.b/x (data:text/html,hi) at ws://h:80/x, said ok.";
        let out = scramble_urls(line);
        for leak in ["secret", "example", "a.b", "text/html", "h:80"] {
            assert!(!out.contains(leak), "{out}");
        }
        assert!(out.starts_with("Refused 'url:") && out.ends_with(", said ok."), "{out}");
        assert_eq!(out.matches("url:").count(), 4, "{out}");
        assert_eq!(scramble_urls("no address here: 1.2"), "no address here: 1.2");
        // Random each time, and not tied to the address's length.
        let lens: std::collections::HashSet<usize> = (0..40).map(|_| scramble_urls("https://example.org/").len()).collect();
        assert!(lens.len() > 3, "{lens:?}");
    }
}
