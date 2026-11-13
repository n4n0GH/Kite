//! The desktop's colors inside a Flatpak, when they change while Kite runs
//! (a switch between a light and a dark theme, say).
//!
//! The sandbox has `kdeglobals` as a single file, which stays the file it was
//! at the start: Plasma writes a new one in its place, and the sandbox keeps the
//! old. Plasma's Qt integration knows, and takes a changed palette from the
//! settings portal; the style of Kite's chrome (Kirigami's, by KColorScheme)
//! reads the colors from the file, and so stayed with the old ones.
//!
//! So on a change, the colors the portal has are written as a color scheme
//! file, and the application is given it (`KDE_COLOR_SCHEME_PATH`, glue.cpp):
//! KColorScheme reads that one in place of `kdeglobals`. Outside a Flatpak
//! nothing of this is needed, and nothing is done.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
/// What Plasma's portal has `kdeglobals` under: a namespace for each group.
const NAMESPACE: &str = "org.kde.kdeglobals.";
const FILE_PREFIX: &str = "desktop-colors-";
const FILE_SUFFIX: &str = ".colors";

type Groups = BTreeMap<String, BTreeMap<String, String>>;

/// The groups of `kdeglobals` a color scheme is made of.
fn of_a_scheme(group: &str) -> bool {
    group.starts_with("Colors:") || group.starts_with("ColorEffects:") || matches!(group, "General" | "KDE" | "WM")
}

/// The text of a color scheme file with the groups the portal gave.
///
/// The portal has no groups within groups, so not the titlebar's colors in a
/// window that isn't active (`[Colors:Header][Inactive]`): those are put back
/// from the `WM` group, which has the two that tell such a titlebar apart.
pub fn scheme_text(groups: &Groups) -> String {
    let mut out = String::new();
    let mut group_text = |name: &str, entries: &BTreeMap<String, String>| {
        out.push_str(&format!("[{name}]\n"));
        for (key, value) in entries {
            // (As KConfig writes a line break in a value.)
            out.push_str(&format!("{key}={}\n", value.replace('\\', "\\\\").replace('\n', "\\n")));
        }
        out.push('\n');
    };
    for (group, entries) in groups.iter().filter(|(g, e)| of_a_scheme(g) && !e.is_empty()) {
        group_text(group, entries);
    }
    if let (Some(header), Some(wm)) = (groups.get("Colors:Header"), groups.get("WM")) {
        if let (Some(background), Some(foreground)) = (wm.get("inactiveBackground"), wm.get("inactiveForeground")) {
            let mut inactive = header.clone();
            if let Some(active) = header.get("BackgroundNormal") {
                inactive.insert("BackgroundAlternate".into(), active.clone());
            }
            inactive.insert("BackgroundNormal".into(), background.clone());
            inactive.insert("ForegroundNormal".into(), foreground.clone());
            group_text("Colors:Header][Inactive", &inactive);
        }
    }
    out
}

fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// The scheme file written last, if the colors changed since Kite started.
pub fn current() -> Option<PathBuf> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

static CURRENT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

fn files() -> Vec<PathBuf> {
    let Ok(base) = crate::paths::base_dir() else { return vec![] };
    let Ok(dir) = std::fs::read_dir(base) else { return vec![] };
    dir.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(FILE_PREFIX) && n.ends_with(FILE_SUFFIX))).collect()
}

fn read(connection: &Connection) -> anyhow::Result<Groups> {
    let reply = connection.call_method(Some(PORTAL), PORTAL_PATH, Some(SETTINGS), "ReadAll", &(vec![format!("{NAMESPACE}*")],))?;
    let all: HashMap<String, HashMap<String, OwnedValue>> = reply.body().deserialize()?;
    let mut groups = Groups::new();
    for (namespace, entries) in all {
        let Some(group) = namespace.strip_prefix(NAMESPACE) else { continue };
        let entries = entries.into_iter().filter_map(|(k, v)| Some((k, String::try_from(v).ok()?))).collect();
        groups.insert(group.to_owned(), entries);
    }
    Ok(groups)
}

/// Writes the scheme under a name not used before: KDE's libraries keep a file
/// they have read by its name.
fn write(groups: &Groups, serial: u32) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(groups.contains_key("Colors:View"), "the settings portal has no colors");
    let path = crate::paths::base_dir()?.join(format!("{FILE_PREFIX}{}-{serial}{FILE_SUFFIX}", std::process::id()));
    crate::util::write_atomic(&path, scheme_text(groups).as_bytes())?;
    Ok(path)
}

/// In a Flatpak: from now on, calls `changed` with a color scheme file of the
/// desktop's colors each time they change. (On a thread of its own.)
pub fn watch(changed: impl Fn(&std::path::Path) + Send + 'static) {
    if !in_flatpak() {
        return;
    }
    let run = move || -> anyhow::Result<()> {
        // What an earlier Kite left behind.
        for old in files() {
            let _ = std::fs::remove_file(old);
        }
        let connection = Connection::session()?;
        let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface(SETTINGS)?.member("SettingChanged")?.path(PORTAL_PATH)?.build();
        let messages = zbus::blocking::MessageIterator::for_match_rule(rule, &connection, None)?;
        let mut serial = 0;
        let mut written: Vec<PathBuf> = vec![];
        for message in messages.flatten() {
            // Plasma's portal says so for every change of the colors, whatever changed them.
            let Ok((namespace, key, _)) = message.body().deserialize::<(String, String, OwnedValue)>() else { continue };
            if namespace != format!("{NAMESPACE}General") || key != "ColorScheme" {
                continue;
            }
            serial += 1;
            match read(&connection).and_then(|groups| write(&groups, serial)) {
                Ok(path) => {
                    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.clone());
                    changed(&path);
                    // The one before stays until the next change: the windows are
                    // given the new one a moment from now, on Kite's main thread.
                    written.push(path);
                    if written.len() > 2 {
                        let _ = std::fs::remove_file(written.remove(0));
                    }
                }
                Err(e) => log::warn!("the desktop's changed colors: {e}"),
            }
        }
        Ok(())
    };
    let spawned = std::thread::Builder::new().name("kite-desktop-colors".into()).spawn(move || {
        if let Err(e) = run() {
            log::warn!("watching the desktop's colors: {e}");
        }
    });
    if let Err(e) = spawned {
        log::warn!("watching the desktop's colors: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scheme_has_the_color_groups_only() {
        let mut groups = Groups::new();
        groups.insert("Colors:View".into(), BTreeMap::from([("BackgroundNormal".into(), "27,30,32".into())]));
        groups.insert("WM".into(), BTreeMap::from([("activeBackground".into(), "39,44,49".into())]));
        groups.insert("KFileDialog Settings".into(), BTreeMap::from([("Recent Files".into(), "a".into())]));
        groups.insert("ColorEffects:Inactive".into(), BTreeMap::new());
        assert_eq!(scheme_text(&groups), "[Colors:View]\nBackgroundNormal=27,30,32\n\n[WM]\nactiveBackground=39,44,49\n\n");
    }

    #[test]
    fn the_inactive_titlebar_is_put_back_from_the_wm_group() {
        let mut groups = Groups::new();
        groups.insert("Colors:Header".into(), BTreeMap::from([("BackgroundNormal".into(), "1,1,1".into()), ("ForegroundNormal".into(), "2,2,2".into()), ("ForegroundLink".into(), "3,3,3".into())]));
        groups.insert("WM".into(), BTreeMap::from([("inactiveBackground".into(), "4,4,4".into()), ("inactiveForeground".into(), "5,5,5".into())]));
        let text = scheme_text(&groups);
        assert!(text.ends_with("[Colors:Header][Inactive]\nBackgroundAlternate=1,1,1\nBackgroundNormal=4,4,4\nForegroundLink=3,3,3\nForegroundNormal=5,5,5\n\n"), "{text}");
    }
}
