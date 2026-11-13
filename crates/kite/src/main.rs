//! Kite — a KDE web browser in Rust with native Firefox WebExtension support.
//!
//! Architecture:
//! * `kite-core` (Rust): data, content blocking and the WebExtension runtime.
//! * this crate: QML singletons (`Browser`, `Extensions`) built with cxx-qt,
//!   plus a thin C++ layer for QtWebEngine hooks (cpp/).
//! * qml/: the user interface (Kirigami / QtQuick, QtWebEngine views).

mod browser;
mod crash;
mod extensions;
mod ffi;
mod formfill;
mod native_hosts;
mod passwords;
mod ytdlp;
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
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Some(appimage.into());
    }
    let exe = std::env::current_exe().ok()?;
    // A binary replaced while Kite runs — a package upgrade, or a rebuild —
    // leaves /proc/self/exe, and so current_exe(), pointing at "<path> (deleted)",
    // which can't be started: the Open Profile and Manage Profiles buttons, and a
    // new window in another profile, each do nothing. The new binary is at the
    // original path, so the suffix is dropped when a real file is there again.
    if !exe.exists() {
        if let Some(real) = exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")).map(std::path::PathBuf::from) {
            if real.exists() {
                return Some(real);
            }
        }
    }
    Some(exe)
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

const HELP: &str = "Usage: kite [options] [--] [URL...]

If the profile is open in a running Kite, the URLs open there as new tabs, and
without any, it opens a new window.

Options:
  -p, --profile <name>   Use (or create) the named profile. Without it, Kite asks
                         if there are several (or uses \"default\")
      --pick-profile     Choose a profile, printing its name
      --private          Start a private browsing session (nothing is saved)
      --factory          Go back to the default settings, saving the current ones
                         to a file in the profile first (bookmarks, logins and
                         extensions stay)
      --no-plugins       Run no plugins this time, for troubleshooting (they stay
                         listed in Settings, where they can be removed)
      --debug            Show tools for looking into Kite: the lists Dangerous
                         Sites checks against (Settings → Browsing)
  -h, --help             Show this help
  -v, --version          Show version
  --                     What follows are URLs or files, even if they start with -

Environment:
  KITE_DATA_DIR          Base directory for profiles
  RUST_LOG               Log filter (default: warn,console::page=error). Web page
                         consoles log to console::page, extension pages to
                         console::extension, Qt to qt::<category>; e.g.
                         RUST_LOG=console::page=info shows all page messages.
                         The log also goes to <data>/logs/<profile>/kite.log,
                         beside the last crash report (last-crash.txt)";

/// Start in the home folder, wherever Kite was started from: file dialogs that aren't
/// told where to open do so in the working directory, and an AppImage keeps the
/// one it was launched in.
fn work_in_home() {
    let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_owned()) else { return };
    if let Some(dir) = std::env::var_os("KITE_DATA_DIR").and_then(|d| std::path::absolute(d).ok()) {
        std::env::set_var("KITE_DATA_DIR", dir);
    }
    let _ = std::env::set_current_dir(home);
}

/// In a Flatpak, the settings of Plasma's global theme (the icon theme, say), as on the
/// host: they're in `kdedefaults` in the configuration folder, which Plasma lists in
/// `XDG_CONFIG_DIRS`. Flatpak sets that variable anew, so KDE's libraries see only
/// `kdeglobals`, and without an icon theme named there, they draw Breeze's. (The
/// manifest lets the folder be read.)
fn plasma_defaults_in_flatpak() {
    if !std::path::Path::new("/.flatpak-info").exists() {
        return;
    }
    let Some(defaults) = directories::BaseDirs::new().map(|d| d.config_dir().join("kdedefaults")) else { return };
    if !defaults.is_dir() {
        return;
    }
    let dirs = std::env::var_os("XDG_CONFIG_DIRS").filter(|d| !d.is_empty()).unwrap_or_else(|| "/etc/xdg".into());
    let mut all = vec![defaults.clone()];
    all.extend(std::env::split_paths(&dirs).filter(|d| *d != defaults));
    if let Ok(joined) = std::env::join_paths(all) {
        std::env::set_var("XDG_CONFIG_DIRS", joined);
    }
}

fn main() {
    crash::init_logging();
    work_in_home();
    let mut profile = kite_core::profiles::DEFAULT.to_owned();
    let mut chosen = false;
    let mut pick = false;
    let mut pick_opens = false;
    let mut private = false;
    let mut factory = false;
    let mut urls = vec![];
    let mut qt_args: Vec<String> = vec![std::env::args().next().unwrap_or_else(|| "kite".into())];
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            // Everything after "--" is an address or a file, even if it starts
            // with "-": what another app hands over can't become an option.
            "--" => {
                urls.extend(it.by_ref());
                break;
            }
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
            "--factory" => factory = true,
            "--no-plugins" => state::NO_PLUGINS.store(true, std::sync::atomic::Ordering::Relaxed),
            "--debug" => state::DEBUG.store(true, std::sync::atomic::Ordering::Relaxed),
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
    } else if !chosen {
        let profiles = kite_core::profiles::Profiles::open().ok();
        // A link clicked in another application goes to the Kite that's open,
        // rather than asking about it each time.
        let open = profiles.as_ref().filter(|_| !urls.is_empty()).and_then(|p| p.only_open());
        if let Some(name) = open {
            profile = name;
        } else if profiles.is_some_and(|p| p.should_ask()) {
            match ask_for_profile() {
                Some(name) => profile = name,
                None => return,
            }
        }
    }
    if factory {
        factory_reset(&profile, private);
    }
    if !private {
        let options = state::NO_PLUGINS.load(std::sync::atomic::Ordering::Relaxed) || state::DEBUG.load(std::sync::atomic::Ordering::Relaxed);
        let instance = claim_profile(&profile, &urls, qt_args.len() > 1 || options);
        *state::INSTANCE.lock().unwrap_or_else(|e| e.into_inner()) = instance;
    }
    crash::start(&profile, private);
    if let Err(e) = state::init(&profile, urls, private) {
        eprintln!("kite: cannot open profile {profile}: {e}");
        std::process::exit(1);
    }

    // Chromium flags: GPU rasterization, overlay scrollbars
    // ("appearance.overlayScrollbars"; js/scrollbars.js extends them to pages
    // that style their own) and the proxy ("privacy.proxy"). Flags the user
    // gives in QTWEBENGINE_CHROMIUM_FLAGS come last, so they win.
    //
    // Chromium's own media player on the bus stays off: it would be a second entry
    // beside the Plasma Integration plugin's, one the desktop can't connect to Kite's
    // window, and it knows nothing of private windows. So with the plugin off, nothing
    // of what plays reaches the desktop.
    {
        let s = state::app().shared.settings();
        let mut flags = vec!["--enable-gpu-rasterization".to_owned(), "--disable-features=HardwareMediaKeyHandling".to_owned()];
        if s.get_bool("appearance.overlayScrollbars") {
            flags.push("--enable-features=FluentOverlayScrollbar".into());
        }
        flags.extend(proxy_flags(&s.get_str("privacy.proxy"), &s.get_str("privacy.proxyType"), &s.get_str("privacy.proxyHost"), s.get_i64("privacy.proxyPort")));
        if let Ok(user) = std::env::var("QTWEBENGINE_CHROMIUM_FLAGS") {
            flags.push(user);
        }
        std::env::set_var("QTWEBENGINE_CHROMIUM_FLAGS", flags.join(" "));
    }
    // Spelling dictionaries: QtWebEngine looks beside its own Qt, which in an
    // AppImage has none; the system's are found (util.rs) and named to it.
    if std::env::var_os("QTWEBENGINE_DICTIONARIES_PATH").is_none() {
        if let Some(dir) = kite_core::util::spell_dictionary_dir() {
            std::env::set_var("QTWEBENGINE_DICTIONARIES_PATH", dir);
        }
    }

    plasma_defaults_in_flatpak();
    kite_core::desktopcolors::watch(ffi::set_desktop_color_scheme);

    let args: Vec<CString> = qt_args.into_iter().map(|a| CString::new(a).unwrap_or_default()).collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut());
    // The strings must outlive the QApplication.
    std::mem::forget(args);
    ffi::set_wheel_pixels(state::app().shared.settings().get_i64("web.wheelScrollPixels"));
    let version = concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast();
    let code = unsafe { kite_bootstrap((argv.len() - 1) as c_int, argv.as_mut_ptr(), version) };
    crash::clean_exit();
    if private {
        let _ = std::fs::remove_dir_all(state::app().shared.profile.root());
    }
    kite_core::util::remove_private_temp_dir();
    std::process::exit(code);
}

/// Takes the profile, or if a Kite has it, gives it the URLs and ends here. If that
/// Kite is from before it could be told (no socket), this one runs beside it, so that
/// what was asked to open still opens. If it listens but didn't answer, this one ends
/// instead: two Kites on one profile overwrite each other's data, and that's worse
/// than an address not opening.
fn claim_profile(profile: &str, urls: &[String], ignored_options: bool) -> Option<kite_core::instance::Instance> {
    use kite_core::instance::{claim, hand_off, has_listener, Request};
    let root = match kite_core::Profile::open(profile) {
        Ok(p) => p.root().to_owned(),
        Err(e) => {
            eprintln!("kite: cannot open profile {profile}: {e}");
            std::process::exit(1);
        }
    };
    if let Some(instance) = claim(&root) {
        return Some(instance);
    }
    let token = std::env::var("XDG_ACTIVATION_TOKEN").unwrap_or_default();
    match hand_off(&root, &Request { urls: urls.to_vec(), token }) {
        Ok(()) => {
            if ignored_options {
                eprintln!("kite: profile {profile} is open in a running Kite, which went on as it was: close it to start with other options");
            }
            std::process::exit(0);
        }
        Err(e) => {
            // (What looked at the lock only for a moment may have been in the way.)
            if let Some(instance) = claim(&root) {
                return Some(instance);
            }
            if has_listener(&root) {
                eprintln!("kite: profile {profile} is open in a running Kite, which didn't take this ({e}): try again, or close it first");
                std::process::exit(1);
            }
            log::warn!("profile {profile} is in use by a Kite that can't be told to open anything (an older version?), so this one runs beside it");
            None
        }
    }
}

/// `--factory`, before anything reads the settings: they may be what's broken.
fn factory_reset(profile: &str, private: bool) {
    if private {
        eprintln!("kite: --factory does nothing with --private, which starts from the defaults anyway");
        return;
    }
    if kite_core::profiles::Profiles::open().is_ok_and(|p| p.in_use(profile)) {
        eprintln!("kite: profile {profile} is in use by a running Kite: close it first, then use --factory");
        std::process::exit(1);
    }
    let saved = kite_core::Profile::open(profile).and_then(|p| kite_core::backup::factory_reset(&p));
    let notice = match saved {
        Ok(Some((import, original))) => {
            eprintln!("kite: default settings. Yours are saved in {}, to bring back with Settings → General → Import Settings (and all of them, as they were, in {}).", import.display(), original.display());
            format!("Default settings. Yours are saved in {}", import.display())
        }
        Ok(None) => {
            eprintln!("kite: profile {profile} already has the default settings");
            "Kite already had its default settings".to_owned()
        }
        Err(e) => {
            eprintln!("kite: couldn't save the settings of profile {profile}, so they weren't changed: {e}");
            std::process::exit(1);
        }
    };
    *state::STARTUP_NOTICE.lock().unwrap_or_else(|e| e.into_inner()) = notice;
}

/// Chromium's proxy switches for "privacy.proxy": "none" connects directly,
/// "manual" uses the given HTTP or SOCKS5 proxy (SOCKS5 also resolves host
/// names through the proxy), "system" leaves it to the desktop settings.
/// A host name, an IPv4 address or a bracketed IPv6 one, and nothing else:
/// the flags are joined with spaces into QTWEBENGINE_CHROMIUM_FLAGS, so a
/// space here (say, from an imported settings file) would add flags of its
/// own, such as --no-sandbox.
fn valid_proxy_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('-')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-_:[]".contains(c))
}

fn proxy_flags(mode: &str, kind: &str, host: &str, port: i64) -> Vec<String> {
    match mode {
        "none" => vec!["--no-proxy-server".into()],
        "manual" if valid_proxy_host(host.trim()) && (1..=65535).contains(&port) => {
            let scheme = if kind == "http" { "http" } else { "socks5" };
            vec![format!("--proxy-server={scheme}://{}:{port}", host.trim())]
        }
        "manual" if !valid_proxy_host(host.trim()) && !host.trim().is_empty() => {
            eprintln!("kite: ignoring the proxy setting: {host:?} isn't a host name or address");
            vec![]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_host_cannot_add_flags() {
        assert_eq!(proxy_flags("manual", "http", " proxy.lan ", 3128), vec!["--proxy-server=http://proxy.lan:3128"]);
        assert_eq!(proxy_flags("manual", "socks", "[::1]", 1080), vec!["--proxy-server=socks5://[::1]:1080"]);
        for bad in ["1.2.3.4 --no-sandbox", "a\t--remote-debugging-port=9222", "--no-sandbox", "a;b", "a/b"] {
            assert!(proxy_flags("manual", "http", bad, 8080).is_empty(), "{bad}");
        }
    }
}
