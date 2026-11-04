//! Custom start pages: local HTML files used as the home page and the new
//! tab page (each its own, "startpage.<target>.*" settings), either in place
//! (a file: URL) or copied into the profile (`<profile>/startpage/<target>/`,
//! served as `kite:startpage/<target>/`), where a page can sit next to its
//! images and style sheets.

use std::path::{Path, PathBuf};

/// The pages that can be custom: "home" and "newtab".
pub const TARGETS: [&str; 2] = ["home", "newtab"];

fn check(target: &str) -> std::io::Result<()> {
    if TARGETS.contains(&target) { Ok(()) } else { Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "unknown start page")) }
}

/// A target's folder in the profile.
pub fn dir(profile_root: &Path, target: &str) -> PathBuf {
    profile_root.join("startpage").join(target)
}

/// The main file of a target's copy.
pub fn index(profile_root: &Path, target: &str) -> PathBuf {
    dir(profile_root, target).join("index.html")
}

/// Copy `src` into the profile as a target's page (replacing the copy's
/// index.html; other files in the folder stay). Copying the copy onto
/// itself does nothing.
pub fn import(profile_root: &Path, target: &str, src: &Path) -> std::io::Result<()> {
    check(target)?;
    let dst = index(profile_root, target);
    if dst.exists() && std::fs::canonicalize(src)? == std::fs::canonicalize(&dst)? {
        return Ok(());
    }
    std::fs::create_dir_all(dir(profile_root, target))?;
    std::fs::copy(src, dst).map(|_| ())
}

/// Write a starter page as a target's copy, unless it has one already; its path.
pub fn create(profile_root: &Path, target: &str) -> std::io::Result<PathBuf> {
    check(target)?;
    let path = index(profile_root, target);
    if !path.exists() {
        std::fs::create_dir_all(dir(profile_root, target))?;
        crate::util::write_atomic(&path, TEMPLATE.as_bytes())?;
    }
    Ok(path)
}

/// The file for a `kite:startpage/<target>/<path>` request, if it is inside
/// that target's folder.
pub fn resolve(profile_root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.split(['?', '#']).next().unwrap_or("");
    let rel = percent_encoding::percent_decode_str(rel).decode_utf8().ok()?;
    let mut parts = rel.trim_start_matches('/').split('/');
    let target = parts.next()?;
    check(target).ok()?;
    let mut path = dir(profile_root, target);
    let mut any = false;
    for part in parts {
        if part.is_empty() && !any {
            continue;
        }
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        path.push(part);
        any = true;
    }
    if !any {
        path.push("index.html");
    }
    path.is_file().then_some(path)
}

const TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Start</title>
<meta name="color-scheme" content="light dark">
<style>
  /* Your start page: edit it ("Edit" in Kite's settings opens it with the
     app your system uses for HTML files); Kite reloads it when you save.
     Images and style sheets can go in the same folder ("Open Folder"). */
  body { font: 16px/1.5 system-ui, sans-serif; display: grid; place-items: center; min-height: 90vh; margin: 0; }
  main { width: min(36em, 90vw); text-align: center; }
  h1 { font-weight: 300; font-size: 3em; margin: 0 0 .5em; }
  input { width: 100%; box-sizing: border-box; font: inherit; padding: .6em 1em; border-radius: 2em; border: 1px solid #8888; }
  nav { margin-top: 2em; display: flex; gap: 1.5em; justify-content: center; flex-wrap: wrap; }
  a { color: inherit; }
</style>
</head>
<body>
<main>
  <h1 id="clock"></h1>
  <form action="kite:search"><input name="q" placeholder="Search or enter address" autofocus></form>
  <nav>
    <a href="https://kde.org/">KDE</a>
    <a href="https://en.wikipedia.org/">Wikipedia</a>
    <a href="https://www.openstreetmap.org/">OpenStreetMap</a>
  </nav>
</main>
<script>
  const tick = () => document.getElementById('clock').textContent = new Date().toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'});
  tick(); setInterval(tick, 1000);
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_create_resolve() {
        let root = std::env::temp_dir().join(format!("kite-startpage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // A starter page, kept when it exists.
        let p = create(&root, "home").unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().contains("kite:search"));
        std::fs::write(&p, "mine").unwrap();
        create(&root, "home").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "mine");
        // Importing the copy onto itself keeps it.
        import(&root, "home", &p).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "mine");
        // Importing replaces index.html, per target.
        let src = root.join("page.html");
        std::fs::write(&src, "<h1>hi</h1>").unwrap();
        import(&root, "newtab", &src).unwrap();
        assert_eq!(std::fs::read_to_string(index(&root, "newtab")).unwrap(), "<h1>hi</h1>");
        assert_eq!(std::fs::read_to_string(index(&root, "home")).unwrap(), "mine");
        std::fs::write(dir(&root, "newtab").join("logo.png"), "png").unwrap();
        assert_eq!(resolve(&root, "newtab/"), Some(index(&root, "newtab")));
        assert_eq!(resolve(&root, "newtab"), Some(index(&root, "newtab")));
        assert_eq!(resolve(&root, "newtab/logo.png?x=1"), Some(dir(&root, "newtab").join("logo.png")));
        assert_eq!(resolve(&root, "home/logo.png"), None);
        assert_eq!(resolve(&root, "newtab/../home/index.html"), None);
        assert_eq!(resolve(&root, "newtab/%2e%2e/home/index.html"), None);
        assert_eq!(resolve(&root, "other/index.html"), None);
        assert!(import(&root, "other", &src).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
