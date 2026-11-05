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

/// Host part of a URL string, or empty.
pub fn host_of(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default()
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
mod google_tests {
    #[test]
    fn only_google_s_own_sign_in() {
        assert!(super::is_google_sign_in("https://accounts.google.com/signin?x=1"));
        for other in ["http://accounts.google.com/", "https://accounts.google.com.evil.org/", "https://evil.org/accounts.google.com/", "https://mail.google.com/", "https://evil.org/?https://accounts.google.com/"] {
            assert!(!super::is_google_sign_in(other), "{other}");
        }
    }
}
