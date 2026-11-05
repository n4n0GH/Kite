//! Download history (Falkon's DownloadManager list).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DownloadRecord {
    pub id: u64,
    pub url: String,
    pub path: String,
    pub mime: String,
    pub total: i64,
    pub received: i64,
    /// "in_progress" | "complete" | "interrupted" | "cancelled"
    pub state: String,
    pub started: i64,
    #[serde(default)]
    pub finished: i64,
    /// From a private window: listed while Kite runs, never saved, and
    /// not shown to extensions.
    #[serde(default)]
    pub private: bool,
}

pub struct Downloads {
    path: PathBuf,
    pub items: Vec<DownloadRecord>,
}

impl Downloads {
    pub fn load(path: PathBuf) -> Self {
        let mut items: Vec<DownloadRecord> =
            std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        // Anything still "in progress" from a previous run was interrupted.
        for i in &mut items {
            if i.state == "in_progress" {
                i.state = "interrupted".into();
            }
        }
        Self { path, items }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let saved: Vec<&DownloadRecord> = self.items.iter().filter(|i| !i.private).collect();
        crate::util::write_atomic(&self.path, serde_json::to_string_pretty(&saved)?.as_bytes())?;
        Ok(())
    }

    pub fn next_id(&self) -> u64 {
        self.items.iter().map(|i| i.id).max().unwrap_or(0) + 1
    }

    pub fn upsert(&mut self, rec: DownloadRecord) {
        match self.items.iter_mut().find(|i| i.id == rec.id) {
            Some(i) => *i = rec,
            None => self.items.insert(0, rec),
        }
        let _ = self.save();
    }

    pub fn remove(&mut self, id: u64) {
        self.items.retain(|i| i.id != id);
        let _ = self.save();
    }

    /// Finished downloads of private windows leave the list (running ones
    /// stay until they finish, to show their progress).
    pub fn forget_private(&mut self) {
        self.items.retain(|i| !i.private || i.state == "in_progress");
    }

    pub fn clear_finished(&mut self) {
        self.items.retain(|i| i.state == "in_progress");
        let _ = self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: u64, url: &str, private: bool, state: &str) -> DownloadRecord {
        DownloadRecord { id, url: url.into(), path: "/tmp/x".into(), mime: String::new(), total: 0, received: 0, state: state.into(), started: 0, finished: 0, private }
    }

    #[test]
    fn private_downloads_are_listed_but_never_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("downloads.json");
        let mut d = Downloads::load(path.clone());
        d.upsert(rec(1, "https://regular.example/a", false, "complete"));
        d.upsert(rec(2, "https://secret.example/b", true, "complete"));
        d.upsert(rec(3, "https://secret.example/c", true, "in_progress"));
        assert_eq!(d.items.len(), 3);

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("regular.example"));
        assert!(!text.contains("secret.example"), "{text}");
        assert_eq!(Downloads::load(path).items.len(), 1);

        d.forget_private();
        assert_eq!(d.items.iter().map(|i| i.id).collect::<Vec<_>>(), [3, 1]);
    }
}
