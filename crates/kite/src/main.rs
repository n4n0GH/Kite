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
mod passwords;
mod rpc;
mod startpage;
mod state;

use std::ffi::CString;
use std::os::raw::{c_char, c_int};

extern "C" {
    fn kite_bootstrap(argc: c_int, argv: *mut *mut c_char) -> c_int;
}

const HELP: &str = "Usage: kite [options] [URL...]

Options:
  -p, --profile <name>   Use (or create) the named profile (default: \"default\")
      --private          Start a private browsing session (nothing is saved)
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
    let mut profile = "default".to_owned();
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
            "-p" | "--profile" => profile = it.next().unwrap_or(profile),
            "--private" => private = true,
            s if s.starts_with("--profile=") => profile = s["--profile=".len()..].to_owned(),
            // Pass Qt / Chromium options through.
            s if s.starts_with('-') => qt_args.push(a.clone()),
            _ => urls.push(a.clone()),
        }
    }
    if private {
        profile = format!("private-{}", std::process::id());
    }
    if let Err(e) = state::init(&profile, urls, private) {
        eprintln!("kite: cannot open profile {profile}: {e}");
        std::process::exit(1);
    }

    // Chromium flags (unless given): GPU rasterization, and overlay scrollbars
    // ("appearance.overlayScrollbars"; js/scrollbars.js extends them to pages
    // that style their own).
    if std::env::var_os("QTWEBENGINE_CHROMIUM_FLAGS").is_none_or(|v| v.is_empty()) {
        let mut flags = String::from("--enable-gpu-rasterization");
        if state::app().shared.settings().get_bool("appearance.overlayScrollbars") {
            flags.push_str(" --enable-features=FluentOverlayScrollbar");
        }
        std::env::set_var("QTWEBENGINE_CHROMIUM_FLAGS", flags);
    }

    let args: Vec<CString> = qt_args.into_iter().map(|a| CString::new(a).unwrap_or_default()).collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut());
    // The strings must outlive the QApplication.
    std::mem::forget(args);
    let code = unsafe { kite_bootstrap((argv.len() - 1) as c_int, argv.as_mut_ptr()) };
    if private {
        let _ = std::fs::remove_dir_all(state::app().shared.profile.root());
    }
    std::process::exit(code);
}
