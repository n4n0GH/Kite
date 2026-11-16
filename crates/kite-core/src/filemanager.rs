//! "Show in Folder": the file manager opened at a file's folder, with the
//! file selected.
//!
//! Opening the folder's address only shows the folder. The desktop's file
//! manager takes the file itself over D-Bus (`org.freedesktop.FileManager1`,
//! which Dolphin, Nautilus and others implement). A sandbox that doesn't let
//! Kite reach it has the portal (`OpenURI.OpenDirectory`), which is handed the
//! open file and asks the file manager on Kite's behalf.
//!
//! Asking the file manager directly is the better of the two, and Kite's
//! Flatpak is allowed to. The portal gives the file manager 25 seconds, then
//! opens the folder in the default one as well; one that takes longer to start
//! (Nautilus outside GNOME does) still shows up afterwards, so there are two.
//! And while it waits, xdg-desktop-portal (1.22) holds a lock that its main
//! loop wants as soon as any program leaves the bus: from then on it answers
//! nobody, and whatever asks it something in the thread of its interface
//! stands still. Kite leaving after its own request was a sure way to that,
//! hence `wait_for_response`.

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use zbus::zvariant::{Fd, OwnedObjectPath, Value};

/// How long the file manager has to answer. It may have to start first, and
/// Nautilus on a desktop that isn't GNOME takes 25 seconds over that.
const FILE_MANAGER_WAIT: Duration = Duration::from_secs(45);
/// How long the portal has to say how its request went: longer than it gives
/// the file manager.
const PORTAL_REQUEST_WAIT: Duration = Duration::from_secs(60);

/// Show `path` selected in its folder. `token`: a Wayland activation token
/// ("" for none), which lets the file manager's window come to the front.
/// An error means nobody took the request: the caller opens the folder.
///
/// Waits for the file manager, which may have to start, for most of a minute
/// if it must: not for the thread the interface runs on.
pub fn show_item(path: &Path, token: &str) -> anyhow::Result<()> {
    let uri = file_uri(path)?;
    // (A file manager that hangs mustn't keep the caller forever.)
    let conn = zbus::blocking::connection::Builder::session()?.method_timeout(FILE_MANAGER_WAIT).build()?;
    let direct = if portal_knows_better(path) {
        anyhow::anyhow!("a file of the document portal")
    } else {
        let direct = conn.call_method(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            Some("org.freedesktop.FileManager1"),
            "ShowItems",
            &(vec![uri.as_str()], token),
        );
        match direct {
            Ok(_) => return Ok(()),
            // It has the request and is busy with it. The portal would only
            // ask the same file manager again.
            Err(e) if timed_out(&e) => anyhow::bail!("the file manager doesn't answer"),
            Err(e) => e.into(),
        }
    };

    // The portal answers at once with a request, and says how that went in a
    // signal later: listened for before asking, so it can't be missed.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")?
        .member("Response")?
        .build();
    let responses = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, None)?;
    let file = std::fs::File::open(path)?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    if !token.is_empty() {
        options.insert("activation_token", Value::from(token));
    }
    let asked = conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.OpenURI"),
        "OpenDirectory",
        &("", Fd::from(file.as_fd()), options),
    );
    let request: OwnedObjectPath = match asked {
        Ok(reply) => reply.body().deserialize()?,
        // The portal has stopped answering. In a sandbox every other way to
        // open the folder goes through it too, and would only wait as well
        // (Qt's does so in the interface's thread).
        Err(e) if timed_out(&e) && sandboxed() => {
            log::warn!("Show in Folder: the portal doesn't answer");
            return Ok(());
        }
        Err(portal) => anyhow::bail!("file manager: {direct}; portal: {portal}"),
    };
    match wait_for_response(conn, responses, &request) {
        // (The portal opens the folder itself if the file manager can't select.)
        Some(0) | None => Ok(()),
        Some(code) => anyhow::bail!("file manager: {direct}; the portal's request ended with {code}"),
    }
}

/// How the portal's `request` went (0: well), or `None` if it didn't say in
/// time. The connection stays open that long and is closed after: see the top
/// of this file for what leaving earlier does to the portal.
fn wait_for_response(conn: zbus::blocking::Connection, responses: zbus::blocking::MessageIterator, request: &OwnedObjectPath) -> Option<u32> {
    let (tell, told) = mpsc::channel();
    let awaited = request.clone();
    std::thread::spawn(move || {
        for message in responses.flatten() {
            if message.header().path().is_some_and(|p| p.as_str() == awaited.as_str()) {
                let _ = tell.send(response_code(&message));
                return;
            }
        }
    });
    let code = told.recv_timeout(PORTAL_REQUEST_WAIT).ok().flatten();
    // (Which also ends the thread above, if it still listens.)
    let _ = conn.close();
    code
}

/// A `Response` signal's first argument: 0 for done, 1 for cancelled by the
/// user, 2 for ended some other way.
fn response_code(message: &zbus::Message) -> Option<u32> {
    let (code, _results): (u32, HashMap<String, zbus::zvariant::OwnedValue>) = message.body().deserialize().ok()?;
    Some(code)
}

fn timed_out(e: &zbus::Error) -> bool {
    matches!(e, zbus::Error::InputOutput(io) if io.kind() == std::io::ErrorKind::TimedOut)
}

fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists() || std::env::var_os("SNAP").is_some()
}

/// Whether `path` is one the document portal lent a sandboxed Kite (a folder
/// picked for downloads outside what the sandbox sees). The file manager
/// would show it where the portal mounts it; the portal knows the real place.
fn portal_knows_better(path: &Path) -> bool {
    sandboxed() && std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| in_document_portal(path, Path::new(&dir)))
}

fn in_document_portal(path: &Path, runtime_dir: &Path) -> bool {
    path.starts_with(runtime_dir.join("doc"))
}

/// The file's address as the file manager wants it, with what an address
/// can't hold as it is (spaces, #, ?) encoded.
fn file_uri(path: &Path) -> anyhow::Result<String> {
    url::Url::from_file_path(path).map(String::from).map_err(|()| anyhow::anyhow!("not an absolute path: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_keep_odd_file_names_whole() {
        assert_eq!(file_uri(Path::new("/home/u/Downloads/a.bin")).unwrap(), "file:///home/u/Downloads/a.bin");
        assert_eq!(file_uri(Path::new("/d/my file #1?.bin")).unwrap(), "file:///d/my%20file%20%231%3F.bin");
        assert_eq!(file_uri(Path::new("/d/über 100%.pdf")).unwrap(), "file:///d/%C3%BCber%20100%25.pdf");
        assert!(file_uri(Path::new("relative.bin")).is_err());
    }

    #[test]
    fn document_portal_files_are_told_apart() {
        let runtime = Path::new("/run/user/1000");
        assert!(in_document_portal(Path::new("/run/user/1000/doc/3f2a91bc/Videos/a.mkv"), runtime));
        assert!(!in_document_portal(Path::new("/home/u/Downloads/a.mkv"), runtime));
        assert!(!in_document_portal(Path::new("/run/user/1000/documents/a.mkv"), runtime));
    }

    #[test]
    fn only_timeouts_count_as_timed_out() {
        let slow = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        assert!(timed_out(&zbus::Error::InputOutput(std::sync::Arc::new(slow))));
        let gone = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "gone");
        assert!(!timed_out(&zbus::Error::InputOutput(std::sync::Arc::new(gone))));
        assert!(!timed_out(&zbus::Error::InvalidReply));
    }
}
