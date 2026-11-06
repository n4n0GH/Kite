//! Kite — a KDE web browser in Rust with native Firefox WebExtension support.
//!
//! Architecture:
//! * `kite-core` (Rust): data, content blocking and the WebExtension runtime.
//! * this crate: QML singletons (`Browser`, `Extensions`) built with cxx-qt,
//!   plus a thin C++ layer for QtWebEngine hooks (cpp/).
//! * qml/: the user interface (Kirigami / QtQuick, QtWebEngine views).

mod browser;
mod extensions;
mod ffi;
mod formfill;
mod native_hosts;
mod passwords;
mod rpc;
mod startpage;
mod state;

use std::ffi::CString;
use std::os::raw::{c_char, c_int};

extern "C" {
    fn kite_bootstrap(argc: c_int, argv: *mut *mut c_char, version: *const c_char) -> c_int;
    fn kite_pick_profile(argc: c_int, argv: *mut *mut c_char) -> c_int;
}

/// How to start Kite again (an AppImage's own file, not the mounted one).
pub fn own_executable() -> Option<std::path::PathBuf> {
    std::env::var_os("APPIMAGE").map(Into::into).or_else(|| std::env::current_exe().ok())
}

/// The profile picker's own process: the chosen profile goes to stdout, or
/// with --open, is started.
fn pick_profile(args: &[String], open: bool) -> ! {
    ffi::set_pick_profile_opens(open);
    let args: Vec<CString> = args.iter().map(|a| CString::new(a.as_str()).unwrap_or_default()).collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut());
    std::mem::forget(args);
    std::process::exit(unsafe { kite_pick_profile((argv.len() - 1) as c_int, argv.as_mut_ptr()) });
}

/// A start without --profile, with several profiles: the user chooses. None
/// if the picker was closed, to end there; the default if it can't show.
fn ask_for_profile() -> Option<String> {
    let default = || Some(kite_core::profiles::DEFAULT.to_owned());
    let Some(exe) = own_executable() else { return default() };
    match std::process::Command::new(exe).arg("--pick-profile").stderr(std::process::Stdio::inherit()).output() {
        Ok(out) if out.status.success() => {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            (!name.is_empty()).then_some(name)
        }
        _ => default(),
    }
}

const HELP: &str = "Usage: kite [options] [URL...]

Options:
  -p, --profile <name>   Use (or create) the named profile. Without it, Kite asks
                         if there are several (or uses \"default\")
      --pick-profile     Choose a profile, printing its name
      --private          Start a private browsing session (nothing is saved)
      --no-plugins       Run no plugins this time, for troubleshooting (they stay
                         listed in Settings, where they can be removed)
  -h, --help             Show this help
  -v, --version          Show version

Environment:
  KITE_DATA_DIR          Base directory for profiles
  RUST_LOG               Log filter (default: warn,console::page=error). Web page
                         consoles log to console::page, extension pages to
                         console::extension, Qt to qt::<category>; e.g.
                         RUST_LOG=console::page=info shows all page messages";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,console::page=error")).init();
    let mut profile = kite_core::profiles::DEFAULT.to_owned();
    let mut chosen = false;
    let mut pick = false;
    let mut pick_opens = false;
    let mut private = false;
    let mut urls = vec![];
    let mut qt_args: Vec<String> = vec![std::env::args().next().unwrap_or_else(|| "kite".into())];
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return;
            }
            "-v" | "--version" => {
                println!("Kite {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "-p" | "--profile" => {
                profile = it.next().unwrap_or(profile);
                chosen = true;
            }
            "--pick-profile" => pick = true,
            "--pick-profile-open" => {
                pick = true;
                pick_opens = true;
            }
            "--private" => private = true,
            "--no-plugins" => state::NO_PLUGINS.store(true, std::sync::atomic::Ordering::Relaxed),
            s if s.starts_with("--profile=") => {
                profile = s["--profile=".len()..].to_owned();
                chosen = true;
            }
            // Pass Qt / Chromium options through.
            s if s.starts_with('-') => qt_args.push(a.clone()),
            _ => urls.push(a.clone()),
        }
    }
    if pick {
        pick_profile(&qt_args, pick_opens);
    }
    if private {
        profile = format!("private-{}", std::process::id());
    } else if !chosen && kite_core::profiles::Profiles::open().is_ok_and(|p| p.should_ask()) {
        match ask_for_profile() {
            Some(name) => profile = name,
            None => return,
        }
    }
    if let Err(e) = state::init(&profile, urls, private) {
        eprintln!("kite: cannot open profile {profile}: {e}");
        std::process::exit(1);
    }

    // Chromium flags: GPU rasterization, overlay scrollbars
    // ("appearance.overlayScrollbars"; js/scrollbars.js extends them to pages
    // that style their own) and the proxy ("privacy.proxy"). Flags the user
    // gives in QTWEBENGINE_CHROMIUM_FLAGS come last, so they win.
    {
        let s = state::app().shared.settings();
        let mut flags = vec!["--enable-gpu-rasterization".to_owned()];
        if s.get_bool("appearance.overlayScrollbars") {
            flags.push("--enable-features=FluentOverlayScrollbar".into());
        }
        flags.extend(proxy_flags(&s.get_str("privacy.proxy"), &s.get_str("privacy.proxyType"), &s.get_str("privacy.proxyHost"), s.get_i64("privacy.proxyPort")));
        if let Ok(user) = std::env::var("QTWEBENGINE_CHROMIUM_FLAGS") {
            flags.push(user);
        }
        std::env::set_var("QTWEBENGINE_CHROMIUM_FLAGS", flags.join(" "));
    }

    let args: Vec<CString> = qt_args.into_iter().map(|a| CString::new(a).unwrap_or_default()).collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut());
    // The strings must outlive the QApplication.
    std::mem::forget(args);
    ffi::set_wheel_pixels(state::app().shared.settings().get_i64("web.wheelScrollPixels"));
    let version = concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast();
    let code = unsafe { kite_bootstrap((argv.len() - 1) as c_int, argv.as_mut_ptr(), version) };
    if private {
        let _ = std::fs::remove_dir_all(state::app().shared.profile.root());
    }
    std::process::exit(code);
}

/// Chromium's proxy switches for "privacy.proxy": "none" connects directly,
/// "manual" uses the given HTTP or SOCKS5 proxy (SOCKS5 also resolves host
/// names through the proxy), "system" leaves it to the desktop settings.
fn proxy_flags(mode: &str, kind: &str, host: &str, port: i64) -> Vec<String> {
    match mode {
        "none" => vec!["--no-proxy-server".into()],
        "manual" if !host.trim().is_empty() && (1..=65535).contains(&port) => {
            let scheme = if kind == "http" { "http" } else { "socks5" };
            vec![format!("--proxy-server={scheme}://{}:{port}", host.trim())]
        }
        _ => vec![],
    }
}
