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
    fn kite_ask_parent_password(argc: c_int, argv: *mut *mut c_char, profile: *const c_char, changed: bool) -> c_int;
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

/// The profile meant by `name`: "default" is the default one, also once it's renamed.
fn named_profile(name: &str) -> String {
    kite_core::profiles::Profiles::open().map_or_else(|_| name.to_owned(), |p| p.resolve(name))
}

/// A start without --profile, with several profiles: the user chooses. None
/// if the picker was closed, to end there; the default if it can't show. With the
/// name comes a pass, if the parent password was given in the picker.
fn ask_for_profile() -> Option<(String, Option<String>)> {
    let default = || Some((named_profile(kite_core::profiles::DEFAULT), None));
    let Some(exe) = own_executable() else { return default() };
    match std::process::Command::new(exe).arg("--pick-profile").stderr(std::process::Stdio::inherit()).output() {
        Ok(out) if out.status.success() => {
            let out = String::from_utf8_lossy(&out.stdout);
            let mut lines = out.lines();
            let name = lines.next().unwrap_or("").trim().to_owned();
            let pass = lines.next().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty());
            (!name.is_empty()).then_some((name, pass))
        }
        _ => default(),
    }
}

/// The dialog asking for the parent password, in a process of its own (as the
/// profile picker is): this one hasn't started Qt yet, and has to know before it does.
fn ask_parent_password(args: &[String], profile: &str, changed: bool) -> ! {
    let profile = CString::new(profile).unwrap_or_default();
    let args: Vec<CString> = args.iter().map(|a| CString::new(a.as_str()).unwrap_or_default()).collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut());
    std::mem::forget(args);
    std::process::exit(unsafe { kite_ask_parent_password((argv.len() - 1) as c_int, argv.as_mut_ptr(), profile.as_ptr(), changed) });
}

/// The look whether the parental controls are as Kite left them, once for this start
/// and before anything goes by them. Changed outside Kite, they're put back from their
/// copy (the windows say so).
fn parental_check() -> kite_core::parental::Integrity {
    use kite_core::parental::Integrity;
    let integrity = kite_core::parental::Parental::open().map_or(Integrity::Fine, |p| p.check());
    if integrity == Integrity::Restored {
        state::PARENTAL_RESTORED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    integrity
}

/// Parental controls: a profile without restrictions (or a private session, None)
/// opens only with the parent password, unless the Kite that started this one handed
/// it a pass. A child's profile opens as it is. Ends here if the password isn't given.
///
/// If the parental controls were changed outside Kite with nothing to put them back
/// from, no profile opens without the password, a child's neither; with it, they
/// count as they are now.
fn parental_gate(profile: Option<&str>, pass: Option<String>, integrity: kite_core::parental::Integrity) {
    let Ok(parental) = kite_core::parental::Parental::open() else { return };
    let changed = integrity == kite_core::parental::Integrity::Changed;
    if !changed && !parental.needs_password(profile) {
        return;
    }
    if !pass.is_some_and(|p| parental.redeem_pass(&p)) {
        let mut ask = vec!["--parent-password", profile.unwrap_or("")];
        if changed {
            ask.push("--parental-changed");
        }
        let asked = own_executable().and_then(|exe| std::process::Command::new(exe).args(ask).status().ok());
        if !asked.is_some_and(|s| s.success()) {
            std::process::exit(0);
        }
    }
    if changed {
        if let Err(e) = parental.accept() {
            eprintln!("kite: parental controls can't be saved: {e}");
        }
    }
}

const HELP: &str = "Usage: kite [options] [--] [URL...]

If the profile is open in a running Kite, the URLs open there as new tabs, and
without any, it opens a new window.

Options:
  -p, --profile <name>   Use (or create) the named profile. Without it, Kite asks
                         if there are several (or uses the default one, which
                         \"default\" always names, also once it's renamed)
      --pick-profile     Choose a profile, printing its name
      --parent-password <profile>
                         Ask for the parent password (parental controls); exits
                         with 0 if it was given
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
    let mut ask_password: Option<String> = None;
    let mut parental_changed = false;
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
            "--parent-password" => ask_password = Some(it.next().unwrap_or_default()),
            "--parental-changed" => parental_changed = true,
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
    // (Without --profile it's the default one; `--profile default` says that too.)
    profile = named_profile(&profile);
    // A profile is a folder by that name among the profiles: a name, not a path out of them.
    if profile.is_empty() || profile.starts_with('.') || profile.contains(['/', '\\', '\0']) {
        eprintln!("kite: “{profile}” isn't a profile's name");
        std::process::exit(2);
    }
    if pick {
        // (The picker goes by the parental controls too.)
        parental_check();
        pick_profile(&qt_args, pick_opens);
    }
    if let Some(profile) = ask_password {
        ask_parent_password(&qt_args, &profile, parental_changed);
    }
    // A pass from the Kite that started this one (parental controls). It's for this
    // start only: what this Kite starts doesn't get it.
    let mut pass = std::env::var(kite_core::parental::PASS_VARIABLE).ok();
    std::env::remove_var(kite_core::parental::PASS_VARIABLE);
    // A link handed to the only Kite that's open joins what's open already: nothing new opens.
    let mut joins = false;
    let mut integrity = None;
    if private {
        // A folder of its own, gone when this Kite ends. With something random in its
        // name: by the process id alone, a folder left by a Kite that crashed was the
        // next one's that got the same id (in a Flatpak, ids start over each time),
        // with what was allowed and stored in it. And what such Kites left is cleared.
        if let Ok(base) = kite_core::paths::base_dir() {
            kite_core::profiles::sweep_private(&base, std::time::Duration::from_secs(60));
        }
        profile = format!("private-{}-{}", std::process::id(), &kite_core::config::secret()[..8]);
    } else if !chosen {
        let profiles = kite_core::profiles::Profiles::open().ok();
        // A link clicked in another application goes to the Kite that's open,
        // rather than asking about it each time.
        let open = profiles.as_ref().filter(|_| !urls.is_empty()).and_then(|p| p.only_open());
        if let Some(name) = open {
            profile = name;
            joins = true;
        } else if profiles.is_some_and(|p| p.should_ask()) {
            integrity = Some(parental_check());
            match ask_for_profile() {
                Some((name, picked_pass)) => {
                    profile = name;
                    pass = picked_pass.or(pass);
                }
                None => return,
            }
        }
    }
    if !joins {
        parental_gate((!private).then_some(profile.as_str()), pass, integrity.unwrap_or_else(parental_check));
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
    // (A private run's folder is marked as in use while this Kite lives, so the next
    // one's clearing up leaves it: kept for good, the lock ends with the process.)
    if private {
        std::mem::forget(kite_core::profiles::lock(state::app().shared.profile.root()));
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
        // One route for the pages and for Kite's own requests (kite_core::net).
        let route = kite_core::net::Route::from_settings(&s.get_str("privacy.proxy"), &s.get_str("privacy.proxyType"), &s.get_str("privacy.proxyHost"), s.get_i64("privacy.proxyPort"));
        if route == kite_core::net::Route::Broken {
            eprintln!("kite: the proxy that is set can't be used: nothing is loaded until it is put right");
            let mut notice = state::STARTUP_NOTICE.lock().unwrap_or_else(|e| e.into_inner());
            if notice.is_empty() {
                *notice = "A proxy is switched on in Settings → Privacy, but its address or port can't be used. Nothing is loaded until that is put right, or the proxy switched off: Kite then needs a restart.".into();
            }
        }
        flags.extend(route.chromium_flags());
        kite_core::net::set_route(route);
        // A child's profile with safe search: the search engines' names lead to the
        // addresses they keep for safe search that can't be switched off.
        if state::app().parental.restrictions().is_some_and(|r| r.safe_search) {
            flags.push(format!("--host-resolver-rules=\"{}\"", kite_core::parental::safe_search_resolver_rules()));
        }
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_proxy_from_the_settings_adds_no_switch_of_its_own() {
        use kite_core::net::Route;
        assert_eq!(Route::from_settings("manual", "http", " proxy.lan ", 3128).chromium_flags(), vec!["--proxy-server=http://proxy.lan:3128"]);
        for bad in ["1.2.3.4 --no-sandbox", "a\t--remote-debugging-port=9222", "--no-sandbox", "a;b", "a/b"] {
            let flags = Route::from_settings("manual", "http", bad, 8080).chromium_flags();
            assert!(flags.len() == 1 && !flags[0].contains(' ') && !flags[0].contains("sandbox") && !flags[0].contains("debugging"), "{bad}: {flags:?}");
        }
    }
}
