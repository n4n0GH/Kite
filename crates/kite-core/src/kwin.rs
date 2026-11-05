//! KWin on Wayland ignores what a window asks for itself: to stay on top, to be at a
//! place, on all desktops. No Wayland protocol for these reaches ordinary apps. KWin's
//! scripting can set the same things the user could choose in a window's menu.

use std::time::Duration;
use zbus::blocking::Connection;

const SERVICE: &str = "org.kde.KWin";

/// Keep this process's picture-in-picture window above the others, in the corner of
/// the screen, if the desktop is KWin on Wayland (elsewhere the window's own flags do
/// it, or nothing can). The window is told by its title, as KWin shows it, and its
/// narrow minimum width: a page can give the browser's window any title, but that
/// one can't get this narrow. (Its minimum height is no use: to KWin it includes the
/// title bar, which differs per theme.) It may have only just been shown, so this
/// waits a moment, on its own thread.
pub fn place_pip_window(title: &str, min_width: i32, all_desktops: bool) {
    if std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("wayland") {
        return;
    }
    let script = pip_script(std::process::id(), title, min_width, all_desktops);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        if let Err(e) = run_script(&script) {
            log::debug!("couldn't ask KWin to place the window: {e}");
        }
    });
}

fn pip_script(pid: u32, title: &str, min_width: i32, all_desktops: bool) -> String {
    // (What KWin calls it: Qt adds the application's name.)
    let caption = serde_json::to_string(&format!("{title} \u{2014} Kite")).unwrap_or_default();
    // Room for side borders, whatever the theme; the browser's window starts at 480.
    let widest = min_width + 64;
    format!(
        r#"const MARGIN = 24;
for (const w of workspace.windowList()) {{
    if (w.pid !== {pid} || w.caption !== {caption} || w.minSize.width > {widest}) continue;
    w.keepAbove = true;
    w.onAllDesktops = {all_desktops};
    const area = workspace.clientArea(KWin.PlacementArea, w);
    const g = w.frameGeometry;
    w.frameGeometry = {{x: area.x + area.width - g.width - MARGIN, y: area.y + area.height - g.height - MARGIN, width: g.width, height: g.height}};
}}
"#
    )
}

fn run_script(source: &str) -> anyhow::Result<()> {
    // KWin reads the file. Only in the user's own runtime directory: a file
    // others could replace in a shared one would run code in the window manager.
    let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| anyhow::anyhow!("no XDG_RUNTIME_DIR"))?;
    let name = format!("kite-pip-{}", std::process::id());
    let file = std::path::Path::new(&dir).join(format!("{name}.js"));
    std::fs::write(&file, source)?;
    let result = (|| -> anyhow::Result<()> {
        let conn = Connection::session()?;
        let id: i32 = conn.call_method(Some(SERVICE), "/Scripting", Some("org.kde.kwin.Scripting"), "loadScript", &(file.to_string_lossy().as_ref(), name.as_str()))?.body().deserialize()?;
        anyhow::ensure!(id >= 0, "KWin refused the script");
        let path = format!("/Scripting/Script{id}");
        conn.call_method(Some(SERVICE), path.as_str(), Some("org.kde.kwin.Script"), "run", &())?;
        // It runs by itself; it's done long before this.
        std::thread::sleep(Duration::from_millis(500));
        conn.call_method(Some(SERVICE), "/Scripting", Some("org.kde.kwin.Scripting"), "unloadScript", &(name.as_str(),))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&file);
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_script_names_this_process_and_a_window_this_narrow() {
        let s = super::pip_script(42, "Picture-in-Picture", 200, true);
        assert!(s.contains("w.pid !== 42"));
        assert!(s.contains("w.caption !== \"Picture-in-Picture \u{2014} Kite\""), "{s}");
        assert!(s.contains("w.minSize.width > 264"));
        assert!(s.contains("w.keepAbove = true"));
        assert!(s.contains("w.onAllDesktops = true"));
        assert!(super::pip_script(42, "T", 200, false).contains("w.onAllDesktops = false"));
    }

    #[test]
    fn a_title_cannot_become_code() {
        let s = super::pip_script(1, "a\" || evil() || \"", 200, true);
        assert!(s.contains(r#"w.caption !== "a\" || evil() || \" \u{2014} Kite""#) || s.contains("a\\\" || evil()"), "{s}");
    }
}
