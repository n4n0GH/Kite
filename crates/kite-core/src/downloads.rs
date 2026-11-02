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
        crate::util::write_atomic(&self.path, serde_json::to_string_pretty(&self.items)?.as_bytes())?;
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

    pub fn clear_finished(&mut self) {
        self.items.retain(|i| i.state == "in_progress");
        let _ = self.save();
    }
}
