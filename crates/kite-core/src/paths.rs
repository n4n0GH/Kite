//! Profile directory layout.
//!
//! ```text
//! ~/.local/share/kite/profiles/<name>/
//!     settings.json        user preferences
//!     browsedata.db        history, icons, search engines, site settings (Falkon schema)
//!     bookmarks.json       bookmark tree (Falkon format)
//!     session.json         last session, closed tabs/windows
//!     downloads.json       download history
//!     adblock/             filter subscriptions
//!     extensions/          installed WebExtensions (unpacked) + registry
//!     webengine/           Chromium storage (cookies, cache, local storage)
//! ```

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub root: PathBuf,
}

/// What holds the `profiles` directory.
pub fn base_dir() -> anyhow::Result<PathBuf> {
    match std::env::var_os("KITE_DATA_DIR") {
        Some(d) => Ok(PathBuf::from(d)),
        None => directories::ProjectDirs::from("org", "kde", "kite").map(|d| d.data_dir().to_path_buf()).ok_or_else(|| anyhow::anyhow!("no home directory")),
    }
}

impl Profile {
    /// Open (and create if needed) a profile under the user data dir.
    pub fn open(name: &str) -> anyhow::Result<Self> {
        Self::open_at(base_dir()?.join("profiles").join(name), name)
    }

    pub fn open_at(root: PathBuf, name: &str) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&root)?;
        for sub in ["adblock", "extensions", "webengine"] {
            std::fs::create_dir_all(root.join(sub))?;
        }
        Ok(Self { name: name.to_owned(), root })
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn extensions_dir(&self) -> PathBuf {
        self.root.join("extensions")
    }
    pub fn webengine_dir(&self) -> PathBuf {
        self.root.join("webengine")
    }
}
