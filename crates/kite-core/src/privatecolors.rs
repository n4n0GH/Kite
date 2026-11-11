//! The purple of private windows ("privacy.privateWindowColor"), so they're told
//! apart at a glance: the colors of their chrome (BrowserWindow.qml, by
//! PrivateColors.qml), and a color scheme that gives their titlebar the same.
//!
//! KWin colors a window's titlebar by the color scheme named in the application's
//! `KDE_COLOR_SCHEME_PATH` property as the window is created: Plasma's Qt
//! integration passes it on, by a window property on X11 and by the server
//! decoration palette protocol on Wayland. Kite sets it around the showing of a
//! private window (Main.qml). The scheme is the desktop's own with the titlebar
//! groups replaced, so the window matches the desktop in everything else. Other
//! desktops ignore the property: there, only the chrome is purple.

use std::path::PathBuf;

/// The chrome's colors, by Kirigami.Theme's names: a dark violet, readable with
/// light and dark desktop themes alike. (Not visitedLinkColor: Kirigami 6.31
/// sets the text color for it.)
const CHROME: &[(&str, &str)] = &[
    ("backgroundColor", "#35265c"),
    ("alternateBackgroundColor", "#3f2e6b"),
    ("textColor", "#efeaf9"),
    ("disabledTextColor", "#8f84ad"),
    ("highlightColor", "#a386ff"),
    ("highlightedTextColor", "#1a1030"),
    ("activeTextColor", "#c4afff"),
    ("linkColor", "#c4afff"),
    ("hoverColor", "#a386ff"),
    ("focusColor", "#a386ff"),
    ("negativeTextColor", "#ff6b7a"),
    ("neutralTextColor", "#ffb454"),
    ("positiveTextColor", "#5fd68a"),
];
/// The address bar's field, darker than the toolbar it's on.
const FIELD_BACKGROUND: &str = "#271c45";
/// The titlebar of a window that isn't active.
const INACTIVE_BACKGROUND: &str = "#2a1f49";
const INACTIVE_TEXT: &str = "#b3a8cf";
const VISITED: &str = "#e0a8ff";

const SCHEME_FILE: &str = "private-window.colors";

fn chrome(name: &str) -> &'static str {
    CHROME.iter().find(|(n, _)| *n == name).map(|(_, c)| *c).unwrap_or("#000000")
}

/// `{"chrome": {name: color}, "field": {…}}`: the chrome's colors, and the
/// address field's.
pub fn colors_json() -> String {
    let chrome: serde_json::Map<String, serde_json::Value> = CHROME.iter().map(|(n, c)| (n.to_string(), serde_json::Value::from(*c))).collect();
    let mut field = chrome.clone();
    field.insert("backgroundColor".into(), FIELD_BACKGROUND.into());
    serde_json::json!({ "chrome": chrome, "field": field }).to_string()
}

/// `#rrggbb` as a scheme file has colors: `r,g,b`.
fn rgb(hex: &str) -> String {
    let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    format!("{},{},{}", (n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff)
}

/// The scheme for private windows: `desktop_scheme` (the text of the desktop's
/// scheme file, or nothing) with the titlebar's groups replaced. Breeze colors a
/// titlebar by `Colors:Header` when a scheme has it, other decorations by `WM`.
pub fn scheme_text(desktop_scheme: &str) -> String {
    let mut out = String::new();
    let mut dropping = false;
    for line in desktop_scheme.lines() {
        if line.starts_with('[') {
            dropping = line.starts_with("[WM]") || line.starts_with("[Colors:Header]");
        }
        if !dropping {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    let (bg, alt, text) = (rgb(chrome("backgroundColor")), rgb(chrome("alternateBackgroundColor")), rgb(chrome("textColor")));
    let (inactive_bg, inactive_text) = (rgb(INACTIVE_BACKGROUND), rgb(INACTIVE_TEXT));
    let accent = rgb(chrome("highlightColor"));
    let header = |bg: &str, alt: &str, text: &str| {
        format!(
            "BackgroundAlternate={alt}\nBackgroundNormal={bg}\nDecorationFocus={accent}\nDecorationHover={accent}\nForegroundActive={active}\nForegroundInactive={inactive_text}\nForegroundLink={link}\nForegroundNegative={negative}\nForegroundNeutral={neutral}\nForegroundNormal={text}\nForegroundPositive={positive}\nForegroundVisited={visited}\n",
            active = rgb(chrome("activeTextColor")),
            link = rgb(chrome("linkColor")),
            negative = rgb(chrome("negativeTextColor")),
            neutral = rgb(chrome("neutralTextColor")),
            positive = rgb(chrome("positiveTextColor")),
            visited = rgb(VISITED),
        )
    };
    out.push_str(&format!(
        "[WM]\nactiveBackground={bg}\nactiveBlend={bg}\nactiveForeground={text}\ninactiveBackground={inactive_bg}\ninactiveBlend={inactive_bg}\ninactiveForeground={inactive_text}\n\n[Colors:Header]\n{}\n[Colors:Header][Inactive]\n{}",
        header(&bg, &alt, &text),
        header(&inactive_bg, &bg, &inactive_text)
    ));
    out
}

/// The text of the desktop's color scheme file: the one `kdeglobals` names
/// (Breeze when it names none), from the data directories.
fn desktop_scheme() -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    let globals = std::fs::read_to_string(config.join("kdeglobals")).unwrap_or_default();
    let mut in_general = false;
    let mut name = "BreezeLight".to_owned();
    for line in globals.lines() {
        if line.starts_with('[') {
            in_general = line.trim() == "[General]";
        } else if in_general {
            if let Some(v) = line.strip_prefix("ColorScheme=") {
                if !v.trim().is_empty() {
                    name = v.trim().to_owned();
                }
            }
        }
    }
    let mut dirs = vec![std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"))];
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.iter().find_map(|d| std::fs::read_to_string(d.join("color-schemes").join(format!("{name}.colors"))).ok())
}

/// The scheme file for private windows, made (again) from the desktop's current
/// scheme: the path to give Plasma.
pub fn scheme_path() -> anyhow::Result<PathBuf> {
    let base = crate::paths::base_dir()?;
    let path = base.join(SCHEME_FILE);
    let text = scheme_text(&desktop_scheme().unwrap_or_default());
    // (KWin watches the file: only a change is written.)
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(&base)?;
        crate::util::write_atomic(&path, text.as_bytes())?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_named_for_kirigami_and_the_field_is_darker() {
        let v: serde_json::Value = serde_json::from_str(&colors_json()).unwrap();
        assert_eq!(v["chrome"]["backgroundColor"], "#35265c");
        assert_eq!(v["chrome"]["textColor"], v["field"]["textColor"]);
        assert_eq!(v["field"]["backgroundColor"], FIELD_BACKGROUND);
        assert!(v["chrome"].get("visitedLinkColor").is_none(), "Kirigami 6.31 would set the text color");
        assert_eq!(rgb("#35265c"), "53,38,92");
    }

    #[test]
    fn scheme_keeps_the_desktops_colors_but_the_titlebars() {
        let desktop = "[ColorEffects:Inactive]\nEnable=false\n\n[Colors:Header]\nBackgroundNormal=41,44,48\nForegroundNormal=252,252,252\n\n[Colors:Header][Inactive]\nBackgroundNormal=32,35,38\n\n[Colors:Window]\nBackgroundNormal=42,46,50\n\n[General]\nName=Breeze Dark\n\n[WM]\nactiveBackground=39,44,49\nactiveForeground=252,252,252\n";
        let s = scheme_text(desktop);
        assert!(s.contains("[Colors:Window]\nBackgroundNormal=42,46,50\n"), "kept");
        assert!(s.contains("Name=Breeze Dark"));
        assert!(!s.contains("41,44,48") && !s.contains("39,44,49") && !s.contains("32,35,38"), "the old titlebar colors are gone:\n{s}");
        assert_eq!(s.matches("[WM]").count(), 1);
        assert_eq!(s.matches("[Colors:Header]\n").count(), 1);
        assert_eq!(s.matches("[Colors:Header][Inactive]\n").count(), 1);
        assert!(s.contains("[WM]\nactiveBackground=53,38,92\n"));
        assert!(s.contains("[Colors:Header]\nBackgroundAlternate=63,46,107\nBackgroundNormal=53,38,92\n"));
        assert!(s.contains("[Colors:Header][Inactive]\nBackgroundAlternate=53,38,92\nBackgroundNormal=42,31,73\n"));
        // Without a desktop scheme, the titlebar groups alone.
        let bare = scheme_text("");
        assert!(bare.starts_with("[WM]\n"));
        assert!(bare.contains("[Colors:Header]\n"));
    }

    #[test]
    fn scheme_file_is_written_once_per_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SCHEME_FILE);
        crate::util::write_atomic(&path, scheme_text("").as_bytes()).unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        // (As scheme_path does, given the same text.)
        if std::fs::read_to_string(&path).ok().as_deref() != Some(scheme_text("").as_str()) {
            crate::util::write_atomic(&path, b"changed").unwrap();
        }
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
    }
}
