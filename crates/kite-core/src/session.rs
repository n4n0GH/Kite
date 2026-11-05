//! Session store: open windows/tabs, plus recently closed tabs and windows
//! (Falkon's ClosedTabsManager / ClosedWindowsManager / RestoreManager).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Container tab id ("" = none).
    #[serde(default)]
    pub container: String,
    /// Split view: 0 = left, 1 = right.
    #[serde(default)]
    pub pane: i64,
    /// Tab group id ("" = none); see `SavedWindow::groups`.
    #[serde(default)]
    pub group: String,
    /// Tree-style tabs: the index of the parent tab in the window (-1 = none).
    #[serde(default = "no_parent", rename = "treeParent")]
    pub tree_parent: i64,
    #[serde(default, rename = "treeCollapsed")]
    pub tree_collapsed: bool,
}

fn no_parent() -> i64 {
    -1
}

impl Default for SavedTab {
    fn default() -> Self {
        Self {
            url: String::new(),
            title: String::new(),
            pinned: false,
            zoom: 0.0,
            history: Vec::new(),
            history_index: 0,
            container: String::new(),
            pane: 0,
            group: String::new(),
            tree_parent: no_parent(),
            tree_collapsed: false,
        }
    }
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
    /// Split view: each pane's current tab (index, -1 = none).
    #[serde(default, rename = "paneCurrent")]
    pub pane_current: Vec<i64>,
    /// Split view: the right pane's width as dragged (0 = half).
    #[serde(default, rename = "splitWidth")]
    pub split_width: f64,
    /// Tab groups: id → {name, color, collapsed}.
    #[serde(default)]
    pub groups: serde_json::Map<String, serde_json::Value>,
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

#[cfg(test)]
mod tests {
    use super::*;

    // What BrowserWindow.saveState() sends: everything it holds has to come
    // back after a restart.
    #[test]
    fn window_state_survives_saving() {
        let json = r##"[{"tabs":[
            {"url":"https://a.example/","title":"A","pinned":false,"zoom":1.25,"history":[],"history_index":0,"container":"c1","pane":0,"group":"g1","treeParent":-1,"treeCollapsed":true},
            {"url":"https://b.example/","title":"B","pinned":false,"zoom":1,"history":[],"history_index":0,"container":"","pane":1,"group":"","treeParent":0,"treeCollapsed":false}],
            "current":1,"paneCurrent":[0,1],"splitWidth":640.5,"groups":{"g1":{"name":"Work","color":"#3daee9","collapsed":false}},
            "geometry":[1,2,3,4],"maximized":true}]"##;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mut store = SessionStore::load(path.clone());
        store.set_windows(serde_json::from_str(json).unwrap()).unwrap();

        let w = &SessionStore::load(path).data.windows[0];
        assert_eq!(w.pane_current, [0, 1]);
        assert_eq!(w.split_width, 640.5);
        assert_eq!(w.groups["g1"]["name"], "Work");
        assert_eq!((w.tabs[0].container.as_str(), w.tabs[0].group.as_str(), w.tabs[0].tree_collapsed), ("c1", "g1", true));
        assert_eq!((w.tabs[1].pane, w.tabs[1].tree_parent), (1, 0));
        // QML reads these names back.
        let out = serde_json::to_value(w).unwrap();
        assert_eq!(out["paneCurrent"], serde_json::json!([0, 1]));
        assert_eq!(out["tabs"][1]["treeParent"], 0);
    }

    #[test]
    fn older_session_files_still_load() {
        let w: SavedWindow = serde_json::from_str(r#"{"tabs":[{"url":"https://a.example/"}]}"#).unwrap();
        assert_eq!((w.tabs[0].tree_parent, w.tabs[0].pane), (-1, 0));
    }
}
