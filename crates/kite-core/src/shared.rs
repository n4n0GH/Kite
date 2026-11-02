//! Shared browser data, used by both the GUI layer and the WebExtension
//! runtime (history, bookmarks, downloads, sessions, cookies mirror).

use crate::bookmarks::Bookmarks;
use crate::downloads::Downloads;
use crate::session::SessionStore;
use crate::settings::Settings;
use crate::Profile;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub session: bool,
    /// Seconds since epoch, 0 for session cookies.
    pub expiration_date: f64,
    #[serde(default)]
    pub same_site: String,
}

impl Cookie {
    pub fn host_only(&self) -> bool {
        !self.domain.starts_with('.')
    }
    pub fn matches_url(&self, url: &str) -> bool {
        let Ok(u) = url::Url::parse(url) else { return false };
        let host = u.host_str().unwrap_or("");
        let d = self.domain.trim_start_matches('.');
        let domain_ok = if self.host_only() { host == d } else { host == d || host.ends_with(&format!(".{d}")) };
        domain_ok && u.path().starts_with(&self.path) && (!self.secure || u.scheme() == "https")
    }
    pub fn to_api(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "name": self.name, "value": self.value, "domain": self.domain, "path": self.path,
            "secure": self.secure, "httpOnly": self.http_only, "session": self.session,
            "hostOnly": self.host_only(), "storeId": "firefox-default",
            "sameSite": if self.same_site.is_empty() { "no_restriction" } else { &self.same_site },
            "firstPartyDomain": "", "partitionKey": null,
        });
        if !self.session {
            v["expirationDate"] = self.expiration_date.into();
        }
        v
    }
}

#[derive(Clone)]
pub struct Shared {
    pub profile: Profile,
    pub db: Arc<Mutex<Connection>>,
    pub bookmarks: Arc<Mutex<Bookmarks>>,
    pub downloads: Arc<Mutex<Downloads>>,
    pub session: Arc<Mutex<SessionStore>>,
    pub settings: Arc<Mutex<Settings>>,
    pub cookies: Arc<Mutex<Vec<Cookie>>>,
}

impl Shared {
    pub fn open(profile: Profile) -> anyhow::Result<Self> {
        Ok(Shared {
            db: Arc::new(Mutex::new(crate::db::open(&profile.path("browsedata.db"))?)),
            bookmarks: Arc::new(Mutex::new(Bookmarks::load(profile.path("bookmarks.json")))),
            downloads: Arc::new(Mutex::new(Downloads::load(profile.path("downloads.json")))),
            session: Arc::new(Mutex::new(SessionStore::load(profile.path("session.json")))),
            settings: Arc::new(Mutex::new(Settings::load(profile.path("settings.json")))),
            cookies: Arc::new(Mutex::new(vec![])),
            profile,
        })
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn bookmarks(&self) -> MutexGuard<'_, Bookmarks> {
        self.bookmarks.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn downloads(&self) -> MutexGuard<'_, Downloads> {
        self.downloads.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn session(&self) -> MutexGuard<'_, SessionStore> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn cookies(&self) -> MutexGuard<'_, Vec<Cookie>> {
        self.cookies.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// For tests: everything in a temp dir.
    pub fn temporary(dir: &std::path::Path) -> anyhow::Result<Self> {
        Self::open(Profile::open_at(dir.to_path_buf(), "test")?)
    }
}
