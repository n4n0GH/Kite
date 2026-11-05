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

/// A session kept under a name: a snapshot of windows, changed only on request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedSession {
    pub name: String,
    /// When it was saved (milliseconds since the epoch).
    pub saved: i64,
    pub windows: Vec<SavedWindow>,
}

/// What lists show of one.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NamedSessionInfo {
    pub name: String,
    pub saved: i64,
    pub windows: usize,
    pub tabs: usize,
}

pub struct NamedSessions {
    path: PathBuf,
    items: Vec<NamedSession>,
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

impl NamedSessions {
    pub fn load(path: PathBuf) -> Self {
        let items = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { path, items }
    }

    fn store(&self) -> anyhow::Result<()> {
        crate::util::write_atomic(&self.path, serde_json::to_string(&self.items)?.as_bytes())?;
        Ok(())
    }

    fn clean_name(name: &str) -> anyhow::Result<String> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("A session needs a name.");
        }
        if name.chars().count() > 80 {
            anyhow::bail!("That name is too long.");
        }
        Ok(name.to_owned())
    }

    /// Most recently saved first.
    pub fn list(&self) -> Vec<NamedSessionInfo> {
        let mut out: Vec<NamedSessionInfo> =
            self.items.iter().map(|s| NamedSessionInfo { name: s.name.clone(), saved: s.saved, windows: s.windows.len(), tabs: s.windows.iter().map(|w| w.tabs.len()).sum() }).collect();
        out.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| a.name.cmp(&b.name)));
        out
    }

    pub fn get(&self, name: &str) -> Option<&NamedSession> {
        self.items.iter().find(|s| same_name(&s.name, name))
    }

    /// Under a name in use, it replaces that session.
    pub fn save(&mut self, name: &str, windows: Vec<SavedWindow>, now: i64) -> anyhow::Result<()> {
        let name = Self::clean_name(name)?;
        if windows.iter().all(|w| w.tabs.is_empty()) {
            anyhow::bail!("There are no windows to save: private windows aren't kept.");
        }
        self.items.retain(|s| !same_name(&s.name, &name));
        self.items.push(NamedSession { name, saved: now, windows });
        self.store()
    }

    pub fn rename(&mut self, from: &str, to: &str) -> anyhow::Result<()> {
        let to = Self::clean_name(to)?;
        if !same_name(from, &to) && self.get(&to).is_some() {
            anyhow::bail!("There is already a session named “{to}”.");
        }
        let Some(s) = self.items.iter_mut().find(|s| same_name(&s.name, from)) else {
            anyhow::bail!("There is no session named “{from}”.");
        };
        s.name = to;
        self.store()
    }

    pub fn delete(&mut self, name: &str) -> anyhow::Result<()> {
        let before = self.items.len();
        self.items.retain(|s| !same_name(&s.name, name));
        if self.items.len() == before {
            anyhow::bail!("There is no session named “{name}”.");
        }
        self.store()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(urls: &[&str]) -> SavedWindow {
        SavedWindow { tabs: urls.iter().map(|u| SavedTab { url: (*u).into(), ..Default::default() }).collect(), ..Default::default() }
    }

    #[test]
    fn named_sessions_are_kept_with_everything_in_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let mut all = NamedSessions::load(path.clone());
        let mut w = window(&["https://a.example/", "https://b.example/"]);
        w.tabs[1].tree_parent = 0;
        w.tabs[1].group = "g1".into();
        w.tabs[0].container = "c1".into();
        w.pane_current = vec![1, -1];
        w.groups.insert("g1".into(), serde_json::json!({"name": "Work", "color": "#3daee9", "collapsed": true}));
        all.save("Work", vec![w.clone(), window(&["https://c.example/"])], 100).unwrap();

        let again = NamedSessions::load(path);
        let s = again.get("work").expect("found whatever the case");
        assert_eq!(s.windows[0], w);
        assert_eq!(again.list(), [NamedSessionInfo { name: "Work".into(), saved: 100, windows: 2, tabs: 3 }]);
    }

    #[test]
    fn named_sessions_list_newest_first_and_replace_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut all = NamedSessions::load(dir.path().join("s.json"));
        all.save("Old", vec![window(&["https://a.example/"])], 1).unwrap();
        all.save("New", vec![window(&["https://b.example/"])], 2).unwrap();
        assert_eq!(all.list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["New", "Old"]);
        // Updating it: the new contents, and first again.
        all.save("OLD", vec![window(&["https://c.example/", "https://d.example/"])], 3).unwrap();
        let list = all.list();
        assert_eq!((list.len(), list[0].name.as_str(), list[0].tabs), (2, "OLD", 2));
    }

    #[test]
    fn named_sessions_refuse_what_makes_no_sense() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        let mut all = NamedSessions::load(path.clone());
        let w = || vec![window(&["https://a.example/"])];
        assert!(all.save("   ", w(), 1).is_err());
        assert!(all.save(&"x".repeat(81), w(), 1).is_err());
        assert!(all.save("Nothing", vec![], 1).is_err());
        assert!(all.save("Empty", vec![window(&[])], 1).is_err());
        assert!(!path.exists(), "nothing written");
        all.save("One", w(), 1).unwrap();
        all.save("Two", w(), 2).unwrap();
        assert!(all.rename("One", "two").is_err());
        assert!(all.rename("Missing", "Three").is_err());
        assert!(all.delete("Missing").is_err());
        all.rename("One", "ONE").unwrap();
        all.rename("ONE", "  Three ").unwrap();
        assert!(all.get("Three").is_some() && all.get("One").is_none());
        all.delete("three").unwrap();
        assert_eq!(all.list().len(), 1);
        assert_eq!(NamedSessions::load(path).list().len(), 1);
    }

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
