//! "Show in Folder": the file manager opened at a file's folder, with the
//! file selected.
//!
//! Opening the folder's address only shows the folder. The desktop's file
//! manager takes the file itself over D-Bus (`org.freedesktop.FileManager1`,
//! which Dolphin, Nautilus and others implement). A sandbox can't reach that:
//! there the portal is asked (`OpenURI.OpenDirectory`), which is handed the
//! open file and asks the file manager on Kite's behalf.

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::path::Path;
use std::time::Duration;
use zbus::zvariant::{Fd, Value};

/// Show `path` selected in its folder. `token`: a Wayland activation token
/// ("" for none), which lets the file manager's window come to the front.
/// An error means nobody took the request: the caller opens the folder.
///
/// Waits for the file manager, which may have to start: not for the thread
/// the interface runs on.
pub fn show_item(path: &Path, token: &str) -> anyhow::Result<()> {
    let uri = file_uri(path)?;
    // (A file manager that hangs mustn't keep the caller forever.)
    let conn = zbus::blocking::connection::Builder::session()?.method_timeout(Duration::from_secs(15)).build()?;
    let direct = conn.call_method(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        Some("org.freedesktop.FileManager1"),
        "ShowItems",
        &(vec![uri.as_str()], token),
    );
    let Err(direct) = direct else { return Ok(()) };

    let file = std::fs::File::open(path)?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    if !token.is_empty() {
        options.insert("activation_token", Value::from(token));
    }
    // (Answers with a request's path; how it went comes later, and the portal
    // opens the folder itself if the file manager can't select.)
    conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.OpenURI"),
        "OpenDirectory",
        &("", Fd::from(file.as_fd()), options),
    )
    .map_err(|portal| anyhow::anyhow!("file manager: {direct}; portal: {portal}"))?;
    Ok(())
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
}
