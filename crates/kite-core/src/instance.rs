//! One Kite to a profile: what's started on a profile that's already open hands its
//! URLs to the Kite that has it, and ends. Two on one profile would overwrite each
//! other's data, and every link clicked in another application would start a browser.
//!
//! Holding the profile's lock (`profiles::lock`) decides who that is, so of two started
//! at once, one wins. The winner listens on a socket in the profile from then on.
//! Connections wait there until it's ready, so a link clicked during startup isn't lost.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

const SOCKET: &str = "instance.sock";
/// More than any command line could hold.
const MAX_REQUEST: u64 = 4 << 20;
/// A connection that says nothing shouldn't keep the next one waiting for long.
const READ_TIMEOUT: Duration = Duration::from_secs(2);
/// Long enough for a Kite that's still starting up.
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);
/// The winner binds right after taking the lock, so it's only a moment.
const CONNECT_TRIES: u32 = 5;
const CONNECT_WAIT: Duration = Duration::from_millis(100);
const REPLY: &[u8] = b"ok";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Request {
    /// As given on the command line. None asks for a new window.
    pub urls: Vec<String>,
    /// What lets a window take the focus on Wayland, from whatever started this one.
    pub token: String,
}

/// This process's hold on a profile, for as long as it's kept.
pub struct Instance {
    _lock: File,
    listener: Option<UnixListener>,
}

/// None if another process has the profile, or there is no telling.
pub fn claim(root: &Path) -> Option<Instance> {
    let lock = crate::profiles::lock(root)?;
    let path = root.join(SOCKET);
    // (Left by a Kite that ended: with the lock, no one else is using it.)
    let _ = std::fs::remove_file(&path);
    // Only fails for something unusual, like a path too long for a socket. Then what's
    // started later runs beside this one, which is better than not opening at all.
    let listener = UnixListener::bind(&path).map_err(|e| log::warn!("can't listen at {}: {e}", path.display())).ok();
    Some(Instance { _lock: lock, listener })
}

impl Instance {
    /// Calls `on_request` for each request, for as long as the process runs.
    pub fn serve(self, on_request: impl Fn(Request)) {
        let Some(listener) = &self.listener else { return };
        for stream in listener.incoming().flatten() {
            if let Err(e) = answer(stream, &on_request) {
                log::debug!("a request to open something failed: {e}");
            }
        }
    }
}

fn answer(mut stream: UnixStream, on_request: &impl Fn(Request)) -> anyhow::Result<()> {
    // The profile's directory doesn't have to be private. Opening a URL here is
    // for this user only.
    anyhow::ensure!(peer_uid(&stream) == Some(unsafe { libc::geteuid() }), "from another user");
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut text = Vec::new();
    (&mut stream).take(MAX_REQUEST).read_to_end(&mut text)?;
    on_request(serde_json::from_slice(&text)?);
    stream.write_all(REPLY)?;
    Ok(())
}

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let got = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut cred).cast(), &mut len) };
    (got == 0).then_some(cred.uid)
}

/// Gives the request to the Kite that has the profile. An error means it didn't
/// confirm taking it, as with a version from before this existed.
pub fn hand_off(root: &Path, request: &Request) -> anyhow::Result<()> {
    let mut stream = connect(root)?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
    stream.write_all(&serde_json::to_vec(request)?)?;
    stream.shutdown(Shutdown::Write)?;
    let mut reply = Vec::new();
    (&mut stream).take(REPLY.len() as u64).read_to_end(&mut reply)?;
    anyhow::ensure!(reply == REPLY, "no answer");
    Ok(())
}

fn connect(root: &Path) -> std::io::Result<UnixStream> {
    let path = root.join(SOCKET);
    let mut tries = CONNECT_TRIES;
    loop {
        match UnixStream::connect(&path) {
            Err(_) if tries > 1 => {
                tries -= 1;
                std::thread::sleep(CONNECT_WAIT);
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn request(url: &str) -> Request {
        Request { urls: vec![url.to_owned()], token: "token".into() }
    }

    #[test]
    fn only_one_claims_a_profile() {
        let dir = tempfile::tempdir().unwrap();
        let first = claim(dir.path()).expect("free");
        assert!(claim(dir.path()).is_none());
        drop(first);
        assert!(claim(dir.path()).is_some(), "free again, with the old socket file there");
    }

    #[test]
    fn hands_requests_over_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let instance = claim(dir.path()).unwrap();
        let (sent, got) = mpsc::channel();
        std::thread::spawn(move || instance.serve(move |r| sent.send(r).unwrap()));
        hand_off(dir.path(), &request("https://a.example/")).unwrap();
        hand_off(dir.path(), &Request::default()).unwrap();
        assert_eq!(got.recv().unwrap(), request("https://a.example/"));
        assert_eq!(got.recv().unwrap(), Request::default());
    }

    #[test]
    fn waits_for_a_kite_still_starting_up() {
        let dir = tempfile::tempdir().unwrap();
        let instance = claim(dir.path()).unwrap();
        let root = dir.path().to_owned();
        let client = std::thread::spawn(move || hand_off(&root, &request("https://a.example/")));
        std::thread::sleep(Duration::from_millis(300));
        let (sent, got) = mpsc::channel();
        std::thread::spawn(move || instance.serve(move |r| sent.send(r).unwrap()));
        client.join().unwrap().unwrap();
        assert_eq!(got.recv().unwrap(), request("https://a.example/"));
    }

    #[test]
    fn fails_with_no_one_listening() {
        let dir = tempfile::tempdir().unwrap();
        assert!(hand_off(dir.path(), &Request::default()).is_err());
        // A Kite gone, its socket file left.
        drop(claim(dir.path()).unwrap());
        assert!(dir.path().join(SOCKET).exists());
        assert!(hand_off(dir.path(), &Request::default()).is_err());
    }

    #[test]
    fn nonsense_is_not_a_request_and_the_next_one_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let instance = claim(dir.path()).unwrap();
        let (sent, got) = mpsc::channel();
        std::thread::spawn(move || instance.serve(move |r| sent.send(r).unwrap()));
        let mut stream = UnixStream::connect(dir.path().join(SOCKET)).unwrap();
        stream.write_all(b"not json").unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).unwrap();
        assert!(reply.is_empty(), "no confirmation");
        hand_off(dir.path(), &request("https://a.example/")).unwrap();
        assert_eq!(got.recv().unwrap(), request("https://a.example/"));
    }
}
