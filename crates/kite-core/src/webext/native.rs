//! Native messaging: extensions talking to programs on the computer, such as
//! the proxy of a password manager. As in Firefox, whose extensions these are:
//! the same manifests in the same places, the same framing and arguments.

use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A host may send this much at once (as in Firefox).
pub const MAX_FROM_HOST: usize = 1024 * 1024;
/// How long a host gets to leave once told to.
const GRACE: Duration = Duration::from_secs(2);
/// How long `sendNativeMessage` waits for the one answer.
const ONCE_TIMEOUT: Duration = Duration::from_secs(300);

/// What Kite asks the GUI layer to do with a host.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeOp {
    Connect { port: String, ext: String, host: String },
    Post { port: String, message: Value },
    Close { port: String },
}

/// What an extension is told about a host it can't use, whatever the reason:
/// it shouldn't learn which programs are installed.
pub fn denied(host: &str) -> String {
    format!("This extension does not have permission to use native application {host} (or the application is not installed)")
}

/// `org.keepassxc.keepassxc_browser`. (It names a file, so no `/` or `..`.)
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// Where manifests are looked for, first come first served: Kite's own, then
/// Firefox's. (Password managers offer to set up Firefox.)
pub fn search_dirs(kite_dir: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(kite_dir.map(|d| d.join("native-messaging-hosts")));
    dirs.extend(home.map(|h| h.join(".mozilla/native-messaging-hosts")));
    dirs.push("/usr/lib64/mozilla/native-messaging-hosts".into());
    dirs.push("/usr/lib/mozilla/native-messaging-hosts".into());
    dirs
}

pub fn default_search_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    search_dirs(crate::paths::base_dir().ok().as_deref(), home.as_deref())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub path: PathBuf,
    pub manifest_path: PathBuf,
}

/// The host `ext` may use by that name. The first manifest found decides.
pub fn find_host(name: &str, ext: &str, dirs: &[PathBuf]) -> Result<Host, String> {
    if !valid_name(name) {
        return Err(format!("not a host's name: {name:?}"));
    }
    for dir in dirs {
        let manifest_path = dir.join(format!("{name}.json"));
        let Ok(text) = std::fs::read_to_string(&manifest_path) else { continue };
        let m: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let bad = |why: &str| Err(format!("{}: {why}", manifest_path.display()));
        if m["name"].as_str() != Some(name) {
            return bad("its name is another one");
        }
        if m["type"].as_str() != Some("stdio") {
            return bad("its type isn't \"stdio\"");
        }
        let allowed = m["allowed_extensions"].as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(ext)));
        if !allowed {
            return bad(&format!("{ext} isn't in allowed_extensions"));
        }
        let path = PathBuf::from(m["path"].as_str().unwrap_or(""));
        if !path.is_absolute() {
            return bad("its path isn't absolute");
        }
        return Ok(Host { path, manifest_path });
    }
    Err(format!("no manifest for {name}"))
}

pub fn write_frame(w: &mut impl Write, body: &[u8]) -> std::io::Result<()> {
    w.write_all(&(body.len() as u32).to_ne_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// The next message, or None when the host has closed its end.
pub fn read_frame(r: &mut impl Read) -> Result<Option<Value>, String> {
    let mut head = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut head[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err("the host stopped within a message".into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let len = u32::from_ne_bytes(head) as usize;
    if len > MAX_FROM_HOST {
        return Err(format!("the host sent {len} bytes at once"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map(Some).map_err(|e| format!("the host sent something other than JSON: {e}"))
}

/// The host's command: its manifest and the extension as arguments, run in
/// its own folder, with Kite's environment and files kept from it (external.rs).
fn command(host: &Host, ext: &str) -> Command {
    let mut c = crate::external::command(&host.path);
    c.arg(&host.manifest_path).arg(ext);
    if let Some(dir) = host.path.parent() {
        c.current_dir(dir);
    }
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    c
}

/// A running host.
pub struct Session {
    to_host: Sender<Vec<u8>>,
    child: Arc<Mutex<Child>>,
}

impl Session {
    /// Starts the host. `on_message` and `on_close` run on a thread of its own,
    /// in order; `on_close` gets the error, if it ended in one.
    pub fn start(
        host: &Host,
        ext: &str,
        mut on_message: impl FnMut(Value) + Send + 'static,
        on_close: impl FnOnce(Option<String>) + Send + 'static,
    ) -> Result<Session, String> {
        let mut child = command(host, ext).spawn().map_err(|e| format!("{}: {e}", host.path.display()))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let mut stdout = child.stdout.take().ok_or("no stdout")?;
        let child = Arc::new(Mutex::new(child));
        let (to_host, queue) = channel::<Vec<u8>>();
        // (A slow host can't hold up the caller. Its stdin closes with the queue.)
        std::thread::spawn(move || {
            for body in queue {
                if write_frame(&mut stdin, &body).is_err() {
                    break;
                }
            }
        });
        let reaped = child.clone();
        let shown = host.path.clone();
        std::thread::spawn(move || {
            let error = loop {
                match read_frame(&mut stdout) {
                    Ok(Some(message)) => on_message(message),
                    Ok(None) => break None,
                    Err(e) => break Some(e),
                }
            };
            let status = reap(&reaped);
            let failed = status.filter(|s| !s.success()).map(|s| format!("the host ended with {s}"));
            let failure = error.or(failed);
            if let Some(e) = &failure {
                log::warn!("native messaging host {}: {e}", shown.display());
            }
            on_close(failure.map(|_| "An unexpected error occurred".to_owned()));
        });
        Ok(Session { to_host, child })
    }

    pub fn send(&self, message: &Value) {
        let _ = self.to_host.send(message.to_string().into_bytes());
    }

    /// Closes its stdin, and ends it if it doesn't leave by itself.
    pub fn close(self) {
        let Session { to_host, child } = self;
        drop(to_host);
        std::thread::spawn(move || {
            std::thread::sleep(GRACE);
            let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
    }
}

/// Its exit status, once it has closed its stdout. One that stays has to go.
fn reap(child: &Mutex<Child>) -> Option<std::process::ExitStatus> {
    let until = Instant::now() + GRACE;
    loop {
        {
            let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
            match c.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() >= until => {
                    let _ = c.kill();
                    return c.wait().ok();
                }
                Ok(None) => {}
                Err(_) => return None,
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `runtime.sendNativeMessage`: a host started for one message and its answer.
pub fn send_once(host: &Host, ext: &str, message: &Value) -> Result<Value, String> {
    let (done, result) = channel();
    let closed = done.clone();
    let session = Session::start(
        host,
        ext,
        move |answer| {
            let _ = done.send(Ok(answer));
        },
        move |error| {
            let _ = closed.send(Err(error.unwrap_or_else(|| "The native application didn't answer".to_owned())));
        },
    )?;
    session.send(message);
    let answer = result.recv_timeout(ONCE_TIMEOUT).unwrap_or_else(|_| Err("The native application didn't answer".to_owned()));
    session.close();
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    // A shell script for a host. It's read rather than executed (`$0` is what would be
    // the manifest): executing a file just written fails now and then, when another
    // test's fork still holds it open for writing.
    fn script(dir: &Path, name: &str, body: &str) -> Host {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        Host { path: "/bin/sh".into(), manifest_path: path }
    }
    fn manifest(dir: &Path, file: &str, m: Value) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{file}.json")), m.to_string()).unwrap();
    }
    fn good(name: &str, path: &str, ext: &str) -> Value {
        json!({"name": name, "description": "x", "path": path, "type": "stdio", "allowed_extensions": [ext]})
    }

    #[test]
    fn names_cant_leave_the_folder() {
        assert!(valid_name("org.keepassxc.keepassxc_browser"));
        assert!(valid_name("a"));
        for bad in ["", ".", "a.", ".a", "a..b", "../a", "a/b", "a b", "a-b", "a\0b", "é"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn looks_in_kites_folder_then_firefoxs() {
        let d = search_dirs(Some(Path::new("/k")), Some(Path::new("/h")));
        assert_eq!(d[0], Path::new("/k/native-messaging-hosts"));
        assert_eq!(d[1], Path::new("/h/.mozilla/native-messaging-hosts"));
        assert!(d.contains(&PathBuf::from("/usr/lib64/mozilla/native-messaging-hosts")));
    }

    #[test]
    fn a_host_has_to_allow_the_extension() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        let dirs = vec![a.clone(), b.clone()];
        assert!(find_host("h", "e@x", &dirs).is_err(), "nothing there");
        manifest(&b, "h", good("h", "/bin/b", "e@x"));
        assert_eq!(find_host("h", "e@x", &dirs).unwrap().path, Path::new("/bin/b"));
        assert!(find_host("h", "other@x", &dirs).unwrap_err().contains("allowed_extensions"));
        // The first one found decides: it can't be got around by the next.
        manifest(&a, "h", good("h", "/bin/a", "other@x"));
        assert!(find_host("h", "e@x", &dirs).is_err());
        assert_eq!(find_host("h", "other@x", &dirs).unwrap().path, Path::new("/bin/a"));
        manifest(&a, "wrongname", good("another", "/bin/a", "e@x"));
        assert!(find_host("wrongname", "e@x", &dirs).is_err());
        manifest(&a, "relative", good("relative", "bin/a", "e@x"));
        assert!(find_host("relative", "e@x", &dirs).is_err());
        let mut pkcs = good("pk", "/bin/a", "e@x");
        pkcs["type"] = json!("pkcs11");
        manifest(&a, "pk", pkcs);
        assert!(find_host("pk", "e@x", &dirs).is_err());
        // Chromium's kind of manifest allows no Firefox extension.
        manifest(&a, "cr", json!({"name": "cr", "path": "/bin/a", "type": "stdio", "allowed_origins": ["chrome-extension://abc/"]}));
        assert!(find_host("cr", "e@x", &dirs).is_err());
        std::fs::write(a.join("broken.json"), "{").unwrap();
        assert!(find_host("broken", "e@x", &dirs).is_err());
    }

    #[test]
    fn frames() {
        let mut buf = Vec::new();
        write_frame(&mut buf, br#"{"a":1}"#).unwrap();
        assert_eq!(&buf[..4], &7u32.to_ne_bytes());
        let mut r = &buf[..];
        assert_eq!(read_frame(&mut r).unwrap(), Some(json!({"a": 1})));
        assert_eq!(read_frame(&mut r).unwrap(), None);
        let mut short = &buf[..2];
        assert!(read_frame(&mut short).is_err());
        let mut cut = &buf[..6];
        assert!(read_frame(&mut cut).is_err());
        let mut huge = Vec::new();
        huge.extend_from_slice(&((MAX_FROM_HOST + 1) as u32).to_ne_bytes());
        assert!(read_frame(&mut &huge[..]).unwrap_err().contains("at once"));
        let mut notjson = Vec::new();
        write_frame(&mut notjson, b"nope").unwrap();
        assert!(read_frame(&mut &notjson[..]).is_err());
    }

    #[test]
    fn talks_to_a_host_until_closed() {
        let t = tempfile::tempdir().unwrap();
        // (Frames are echoed back as they are.)
        let h = script(t.path(), "echo", "exec cat");
        let (tx, rx) = channel();
        let tx2 = tx.clone();
        let s = Session::start(&h, "e@x", move |m| tx.send(Ok(m)).unwrap(), move |e| tx2.send(Err(e)).unwrap()).unwrap();
        s.send(&json!({"n": 1}));
        s.send(&json!({"n": 2}));
        let wait = Duration::from_secs(5);
        assert_eq!(rx.recv_timeout(wait).unwrap(), Ok(json!({"n": 1})));
        assert_eq!(rx.recv_timeout(wait).unwrap(), Ok(json!({"n": 2})));
        s.close();
        assert_eq!(rx.recv_timeout(wait).unwrap(), Err(None), "closing isn't an error");
    }

    #[test]
    fn gets_the_manifest_and_extension_as_arguments_in_its_folder() {
        let t = tempfile::tempdir().unwrap();
        let out = t.path().join("args");
        let h = script(t.path(), "args", &format!("echo \"$0 $1 $PWD\" > {}", out.display()));
        let (tx, rx) = channel();
        Session::start(&h, "e@x", |_| {}, move |e| tx.send(e).unwrap()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), None);
        let cwd = Path::new("/bin").canonicalize().unwrap();
        assert_eq!(std::fs::read_to_string(out).unwrap().trim(), format!("{} e@x {}", h.manifest_path.display(), cwd.display()));
    }

    #[test]
    fn a_failing_host_is_an_error_without_details() {
        let t = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let h = script(t.path(), "fail", "exit 3");
        Session::start(&h, "e@x", |_| {}, move |e| tx.send(e).unwrap()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Some("An unexpected error occurred".to_owned()));
        let gone = Host { path: "/nonexistent/host".into(), manifest_path: "/m.json".into() };
        assert!(Session::start(&gone, "e@x", |_| {}, |_| {}).is_err());
    }

    #[test]
    fn a_host_ignoring_the_end_is_ended() {
        let t = tempfile::tempdir().unwrap();
        // (Its own sleep is what keeps its stdout open: `exec`, so killing it ends that.)
        let h = script(t.path(), "stay", "exec sleep 60");
        let (tx, rx) = channel();
        let started = Instant::now();
        let s = Session::start(&h, "e@x", |_| {}, move |e| tx.send(e).unwrap()).unwrap();
        s.close();
        rx.recv_timeout(Duration::from_secs(10)).expect("gone");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_host_gets_nothing_kite_has_open() {
        use std::os::fd::AsRawFd;
        let t = tempfile::tempdir().unwrap();
        let open = std::fs::File::create(t.path().join("secret")).unwrap();
        // (The kind Rust's own files never are: not closed when a program starts.)
        const LEAK: i32 = 437;
        assert_eq!(unsafe { libc::dup2(open.as_raw_fd(), LEAK) }, LEAK);
        let out = t.path().join("fds");
        let h = script(t.path(), "fds", &format!("ls /proc/$$/fd > {}", out.display()));
        let (tx, rx) = channel();
        Session::start(&h, "e@x", |_| {}, move |e| tx.send(e).unwrap()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), None);
        unsafe { libc::close(LEAK) };
        let fds = std::fs::read_to_string(out).unwrap();
        assert!(!fds.lines().any(|l| l == LEAK.to_string()), "{fds}");
    }

    #[test]
    fn one_message_one_answer() {
        let t = tempfile::tempdir().unwrap();
        let h = script(t.path(), "echo", "exec cat");
        assert_eq!(send_once(&h, "e@x", &json!({"q": 1})), Ok(json!({"q": 1})));
        let silent = script(t.path(), "silent", "exit 0");
        assert!(send_once(&silent, "e@x", &json!({})).is_err());
    }
}
