//! browser.storage backend: `local` and `sync` persist to SQLite (sync is
//! device-local, like Firefox without a Mozilla account), `session` is
//! in-memory, `managed` is always empty.

use rusqlite::{params, Connection};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

pub struct Storage {
    db: Connection,
    session: HashMap<(String, String), Value>,
}

pub type Changes = Map<String, Value>;

impl Storage {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        Self::with_conn(Connection::open(path)?)
    }
    pub fn in_memory() -> anyhow::Result<Self> {
        Self::with_conn(Connection::open_in_memory()?)
    }
    fn with_conn(db: Connection) -> anyhow::Result<Self> {
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS storage (ext TEXT NOT NULL, area TEXT NOT NULL, key TEXT NOT NULL,
             value TEXT NOT NULL, PRIMARY KEY (ext, area, key));",
        )?;
        Ok(Self { db, session: HashMap::new() })
    }

    fn check_area(area: &str) -> anyhow::Result<()> {
        match area {
            "local" | "sync" | "session" | "managed" => Ok(()),
            _ => anyhow::bail!("unknown storage area {area}"),
        }
    }

    fn all(&self, ext: &str, area: &str) -> anyhow::Result<Map<String, Value>> {
        if area == "managed" {
            return Ok(Map::new());
        }
        if area == "session" {
            return Ok(self
                .session
                .iter()
                .filter(|((e, _), _)| e == ext)
                .map(|((_, k), v)| (k.clone(), v.clone()))
                .collect());
        }
        let mut stmt = self.db.prepare_cached("SELECT key, value FROM storage WHERE ext = ?1 AND area = ?2")?;
        let rows = stmt.query_map(params![ext, area], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Map::new();
        for r in rows {
            let (k, v) = r?;
            out.insert(k, serde_json::from_str(&v).unwrap_or(Value::Null));
        }
        Ok(out)
    }

    /// storage.get semantics: `keys` may be null, a string, an array of
    /// strings, or an object whose values are defaults.
    pub fn get(&self, ext: &str, area: &str, keys: &Value) -> anyhow::Result<Value> {
        Self::check_area(area)?;
        let all = self.all(ext, area)?;
        Ok(Value::Object(match keys {
            Value::Null => all,
            Value::String(k) => all.get(k).map(|v| [(k.clone(), v.clone())].into_iter().collect()).unwrap_or_default(),
            Value::Array(a) => a
                .iter()
                .filter_map(|k| k.as_str())
                .filter_map(|k| all.get(k).map(|v| (k.to_owned(), v.clone())))
                .collect(),
            Value::Object(defaults) => defaults
                .iter()
                .map(|(k, d)| (k.clone(), all.get(k).cloned().unwrap_or_else(|| d.clone())))
                .collect(),
            _ => anyhow::bail!("invalid argument for storage.get"),
        }))
    }

    pub fn set(&mut self, ext: &str, area: &str, items: &Value) -> anyhow::Result<Changes> {
        Self::check_area(area)?;
        if area == "managed" {
            anyhow::bail!("storage.managed is read-only");
        }
        let items = items.as_object().ok_or_else(|| anyhow::anyhow!("storage.set expects an object"))?;
        let old = self.all(ext, area)?;
        let mut changes = Map::new();
        let tx = self.db.transaction()?;
        for (k, v) in items {
            // `undefined` values are dropped by JSON serialization already.
            let mut c = Map::new();
            if let Some(o) = old.get(k) {
                c.insert("oldValue".into(), o.clone());
            }
            c.insert("newValue".into(), v.clone());
            changes.insert(k.clone(), Value::Object(c));
            if area == "session" {
                self.session.insert((ext.to_owned(), k.clone()), v.clone());
            } else {
                tx.execute(
                    "INSERT OR REPLACE INTO storage (ext, area, key, value) VALUES (?1, ?2, ?3, ?4)",
                    params![ext, area, k, v.to_string()],
                )?;
            }
        }
        tx.commit()?;
        Ok(changes)
    }

    pub fn remove(&mut self, ext: &str, area: &str, keys: &Value) -> anyhow::Result<Changes> {
        Self::check_area(area)?;
        let keys: Vec<String> = match keys {
            Value::String(k) => vec![k.clone()],
            Value::Array(a) => a.iter().filter_map(|k| k.as_str().map(str::to_owned)).collect(),
            _ => vec![],
        };
        let old = self.all(ext, area)?;
        let mut changes = Map::new();
        for k in keys {
            if let Some(o) = old.get(&k) {
                changes.insert(k.clone(), serde_json::json!({ "oldValue": o }));
            }
            if area == "session" {
                self.session.remove(&(ext.to_owned(), k));
            } else {
                self.db.execute("DELETE FROM storage WHERE ext = ?1 AND area = ?2 AND key = ?3", params![ext, area, k])?;
            }
        }
        Ok(changes)
    }

    pub fn clear(&mut self, ext: &str, area: &str) -> anyhow::Result<Changes> {
        let keys: Vec<Value> = self.all(ext, area)?.keys().map(|k| Value::String(k.clone())).collect();
        self.remove(ext, area, &Value::Array(keys))
    }

    pub fn bytes_in_use(&self, ext: &str, area: &str, keys: &Value) -> anyhow::Result<usize> {
        let v = self.get(ext, area, keys)?;
        Ok(v.as_object().map(|o| o.iter().map(|(k, v)| k.len() + v.to_string().len()).sum()).unwrap_or(0))
    }

    /// Remove all data of an extension (uninstall).
    pub fn wipe(&mut self, ext: &str) -> anyhow::Result<()> {
        self.db.execute("DELETE FROM storage WHERE ext = ?1", [ext])?;
        self.session.retain(|(e, _), _| e != ext);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn semantics() {
        let mut s = Storage::in_memory().unwrap();
        let c = s.set("e", "local", &json!({"a": 1, "b": {"x": [1, 2]}})).unwrap();
        assert_eq!(c["a"], json!({"newValue": 1}));
        let c = s.set("e", "local", &json!({"a": 2})).unwrap();
        assert_eq!(c["a"], json!({"oldValue": 1, "newValue": 2}));
        assert_eq!(s.get("e", "local", &json!("a")).unwrap(), json!({"a": 2}));
        assert_eq!(s.get("e", "local", &json!(["b", "zz"])).unwrap(), json!({"b": {"x": [1, 2]}}));
        assert_eq!(s.get("e", "local", &json!({"zz": 5, "a": 0})).unwrap(), json!({"zz": 5, "a": 2}));
        assert_eq!(s.get("other", "local", &Value::Null).unwrap(), json!({}));
        s.set("e", "session", &json!({"t": true})).unwrap();
        assert_eq!(s.get("e", "session", &Value::Null).unwrap(), json!({"t": true}));
        assert_eq!(s.remove("e", "local", &json!("a")).unwrap()["a"], json!({"oldValue": 2}));
        assert_eq!(s.clear("e", "local").unwrap().len(), 1);
        assert!(s.set("e", "managed", &json!({})).is_err());
        assert!(s.get("e", "bogus", &Value::Null).is_err());
    }
}
