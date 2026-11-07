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

/// A current Firefox's user agent, for the sites that only work for known browsers.
pub const FIREFOX_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// Is this where Google signs people in? It refuses browsers it doesn't know.
pub fn is_google_sign_in(url: &str) -> bool {
    url.starts_with("https://") && host_of(url) == "accounts.google.com"
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
