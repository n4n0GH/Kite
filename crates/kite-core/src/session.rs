//! Session store: open windows/tabs, plus recently closed tabs and windows
//! (Falkon's ClosedTabsManager / ClosedWindowsManager / RestoreManager).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SavedTab {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub zoom: f64,
    /// Back/forward history as URLs, oldest first, and the current index.
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(default)]
    pub history_index: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SavedWindow {
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub current: usize,
    #[serde(default)]
    pub geometry: Option<[i32; 4]>,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionFile {
    #[serde(default)]
    pub windows: Vec<SavedWindow>,
    #[serde(default)]
    pub closed_tabs: Vec<SavedTab>,
    #[serde(default)]
    pub closed_windows: Vec<SavedWindow>,
    /// Set while running; if still set at startup the previous run crashed.
    #[serde(default)]
    pub running: bool,
}

pub const MAX_CLOSED: usize = 25;

pub struct SessionStore {
    path: PathBuf,
    pub data: SessionFile,
}

impl SessionStore {
    pub fn load(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { path, data }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        crate::util::write_atomic(&self.path, serde_json::to_string(&self.data)?.as_bytes())?;
        Ok(())
    }

    pub fn crashed_last_time(&self) -> bool {
        self.data.running
    }

    pub fn set_windows(&mut self, windows: Vec<SavedWindow>) -> anyhow::Result<()> {
        self.data.windows = windows;
        self.save()
    }

    pub fn push_closed_tab(&mut self, tab: SavedTab) {
        if !crate::history::is_recordable(&tab.url) && tab.history.len() <= 1 {
            return;
        }
        self.data.closed_tabs.insert(0, tab);
        self.data.closed_tabs.truncate(MAX_CLOSED);
        let _ = self.save();
    }

    pub fn pop_closed_tab(&mut self, index: usize) -> Option<SavedTab> {
        if index < self.data.closed_tabs.len() {
            let t = self.data.closed_tabs.remove(index);
            let _ = self.save();
            Some(t)
        } else {
            None
        }
    }

    pub fn push_closed_window(&mut self, w: SavedWindow) {
        if w.tabs.is_empty() {
            return;
        }
        self.data.closed_windows.insert(0, w);
        self.data.closed_windows.truncate(MAX_CLOSED);
        let _ = self.save();
    }

    pub fn pop_closed_window(&mut self, index: usize) -> Option<SavedWindow> {
        if index < self.data.closed_windows.len() {
            let w = self.data.closed_windows.remove(index);
            let _ = self.save();
            Some(w)
        } else {
            None
        }
    }
}
