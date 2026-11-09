use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch (Falkon stores history dates this way).
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Write `data` to `path` atomically (write temp file, then rename).
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp~");
    std::fs::write(&tmp, data)?;
    std::fs::rename(tmp, path)
}

/// The files in a folder and its subfolders, without following links, up to `limit`.
/// More than that gives `limit + 1`: it stops there, so a huge folder doesn't take long.
pub fn count_files(folder: &std::path::Path, limit: usize) -> usize {
    let (mut files, mut seen) = (0, 0);
    let mut folders = vec![folder.to_owned()];
    while let Some(dir) = folders.pop() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            // (Counted too: a tree of nothing but folders can be huge as well.)
            seen += 1;
            if files > limit || seen > limit.saturating_mul(10) {
                return limit + 1;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => folders.push(entry.path()),
                Ok(_) => files += 1,
                Err(_) => {}
            }
        }
    }
    files.min(limit + 1)
}

/// Host part of a URL string, or empty.
pub fn host_of(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default()
}

/// An address as lists show it: without https:// (http:// stays, it's worth knowing)
/// and without the slash of a main page.
pub fn display_url(url: &str) -> String {
    let shown = url.strip_prefix("https://").unwrap_or(url);
    let scheme = shown.find("://").map_or(0, |i| i + 3);
    match shown.strip_suffix('/') {
        Some(main) if !main[scheme..].contains('/') => main.to_owned(),
        _ => shown.to_owned(),
    }
}

/// A local path from what a file dialog gives: a `file:` URL (with its %20s) or a path.
pub fn local_path(text: &str) -> std::path::PathBuf {
    url::Url::parse(text).ok().filter(|u| u.scheme() == "file").and_then(|u| u.to_file_path().ok()).unwrap_or_else(|| text.into())
}

/// A time as local date and time ("2026-11-09 08:51:02"), `with_ms` adding milliseconds.
pub fn local_time_fmt(t: std::time::SystemTime, with_date: bool, with_ms: bool) -> String {
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return String::new();
    }
    let mut s = String::new();
    if with_date {
        s += &format!("{:04}-{:02}-{:02} ", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday);
    }
    s += &format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec);
    if with_ms {
        s += &format!(".{:03}", d.subsec_millis());
    }
    s
}

/// A time as local date and time ("2026-11-09 08:51:02").
pub fn local_time(t: std::time::SystemTime) -> String {
    local_time_fmt(t, true, false)
}

/// A current Firefox's user agent, for the sites that only work for known browsers.
pub const FIREFOX_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// A site's user agent (Site Settings) meaning Kite's own, naming
/// "QtWebEngine/x.y" (what "privacy.genericUserAgent" leaves out). Some sites
/// refuse anything that claims to be Chrome but isn't, like invent.kde.org.
pub const QTWEBENGINE_USER_AGENT: &str = "kite:qtwebengine";

/// Common browsers' user agents, offered for a site (name, user agent).
pub const USER_AGENT_PRESETS: &[(&str, &str)] = &[
    ("Firefox on Linux", FIREFOX_USER_AGENT),
    ("Firefox on Windows", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0"),
    ("Firefox on macOS", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:140.0) Gecko/20100101 Firefox/140.0"),
    ("Chrome on Linux", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36"),
    ("Chrome on Windows", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36"),
    ("Edge on Windows", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0"),
    ("Safari on macOS", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15"),
    ("Chrome on Android", "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Mobile Safari/537.36"),
    ("Safari on iPhone", "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1"),
];

/// What a site's chosen user agent stands for, given QtWebEngine's own.
pub fn resolve_user_agent(chosen: &str, qtwebengine: &str) -> String {
    if chosen == QTWEBENGINE_USER_AGENT { qtwebengine.to_owned() } else { chosen.trim().to_owned() }
}

/// Is this where Google signs people in? It refuses browsers it doesn't know.
pub fn is_google_sign_in(url: &str) -> bool {
    url.starts_with("https://") && host_of(url) == "accounts.google.com"
}

/// This process's own folder for temporary files: a fresh, randomly named one
/// (mode 0700) in the user's runtime directory, or the temp directory without
/// one. A predictable name in a shared /tmp could be made in advance by another
/// user, who would then see or swap what Kite puts there.
static TEMP_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn private_temp_dir() -> std::io::Result<&'static std::path::Path> {
    if let Some(d) = TEMP_DIR.get() {
        return Ok(d);
    }
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    let made = tempfile::Builder::new().prefix("kite-").tempdir_in(base)?.keep();
    Ok(TEMP_DIR.get_or_init(|| made))
}

/// Remove this process's temporary folder (on quitting).
pub fn remove_private_temp_dir() {
    if let Some(d) = TEMP_DIR.get() {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[cfg(test)]
mod display_url_tests {
    use super::display_url;
    #[test]
    fn leaves_out_what_says_nothing() {
        assert_eq!(display_url("https://www.youtube.com/"), "www.youtube.com");
        assert_eq!(display_url("https://www.youtube.com/watch?v=abc"), "www.youtube.com/watch?v=abc");
        assert_eq!(display_url("https://kde.org/news/"), "kde.org/news/");
        assert_eq!(display_url("http://kde.org/"), "http://kde.org");
        assert_eq!(display_url("file:///home/me/a.html"), "file:///home/me/a.html");
    }
}

#[cfg(test)]
mod google_tests {
    #[test]
    fn only_google_s_own_sign_in() {
        assert!(super::is_google_sign_in("https://accounts.google.com/signin?x=1"));
        for other in ["http://accounts.google.com/", "https://accounts.google.com.evil.org/", "https://evil.org/accounts.google.com/", "https://mail.google.com/", "https://evil.org/?https://accounts.google.com/"] {
            assert!(!super::is_google_sign_in(other), "{other}");
        }
    }
}

#[cfg(test)]
mod count_files_tests {
    use super::count_files;

    #[test]
    fn counts_files_in_subfolders_without_following_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
        for f in ["a", "sub/b", "sub/deeper/.hidden"] {
            std::fs::write(dir.path().join(f), "x").unwrap();
        }
        assert_eq!(count_files(dir.path(), 100), 3);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("c"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        assert_eq!(count_files(dir.path(), 100), 4, "the link itself, not what's behind it");
        assert_eq!(count_files(&dir.path().join("missing"), 100), 0);
    }

    #[test]
    fn counting_files_stops_at_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            std::fs::write(dir.path().join(i.to_string()), "x").unwrap();
        }
        assert_eq!(count_files(dir.path(), 20), 20);
        assert_eq!(count_files(dir.path(), 5), 6);
    }
}
