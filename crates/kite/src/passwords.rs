//! `Passwords` — QML singleton for the password manager (Falkon's AutoFill).
//!
//! Security rules:
//! * Every page-facing call takes the URL **from the WebEngineView**, never
//!   from page JavaScript, and only http(s) top-level documents qualify.
//! * Pending "save this password?" decisions are kept here; QML only sees an
//!   id, host and username.
//! * Nothing is ever saved from private windows / sessions.

use crate::state::app;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use kite_core::passwords::{create_host, Backend, Decision, Entry, PasswordStore};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, locked)]
        #[qproperty(bool, encrypted)]
        #[qproperty(QString, backend)]
        #[qproperty(bool, kwallet_available, cxx_name = "kwalletAvailable")]
        #[qproperty(bool, wallet_opening, cxx_name = "walletOpening")]
        type Passwords = super::PasswordsRust;

        // store state
        #[qinvokable]
        fn refresh(self: Pin<&mut Passwords>);
        #[qinvokable]
        fn unlock(self: Pin<&mut Passwords>, password: &QString) -> QString;
        #[qinvokable]
        fn lock(self: Pin<&mut Passwords>);
        #[qinvokable]
        #[cxx_name = "setMasterPassword"]
        fn set_master_password(self: Pin<&mut Passwords>, current: &QString, new_password: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "openWallet"]
        fn open_wallet(self: Pin<&mut Passwords>);
        #[qsignal]
        #[cxx_name = "walletOpened"]
        fn wallet_opened(self: Pin<&mut Passwords>, error: QString);
        #[qinvokable]
        #[cxx_name = "switchBackend"]
        fn switch_backend(self: Pin<&mut Passwords>, target: &QString, master: &QString);
        #[qsignal]
        #[cxx_name = "backendSwitched"]
        fn backend_switched(self: Pin<&mut Passwords>, error: QString);
        #[qinvokable]
        #[cxx_name = "removeMasterPassword"]
        fn remove_master_password(self: Pin<&mut Passwords>, current: &QString) -> QString;

        // page integration (URLs come from the view, not the page)
        #[qinvokable]
        #[cxx_name = "formSubmitted"]
        fn form_submitted(self: Pin<&mut Passwords>, page_url: &QString, private_mode: bool, json: &QString) -> QString;
        #[qinvokable]
        fn answer(self: Pin<&mut Passwords>, request: i32, choice: &QString) -> QString;
        #[qinvokable]
        fn logins(self: &Passwords, page_url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "fillData"]
        fn fill_data(self: &Passwords, page_url: &QString, id: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "autofillEnabled"]
        fn autofill_enabled(self: &Passwords) -> bool;
        #[qinvokable]
        #[cxx_name = "httpAuth"]
        fn http_auth(self: &Passwords, url: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "saveHttpAuth"]
        fn save_http_auth(self: Pin<&mut Passwords>, url: &QString, private_mode: bool, username: &QString, password: &QString);

        // manager UI
        #[qinvokable]
        fn list(self: &Passwords) -> QString;
        #[qinvokable]
        fn update(self: Pin<&mut Passwords>, json: &QString) -> QString;
        #[qinvokable]
        fn add(self: Pin<&mut Passwords>, json: &QString) -> QString;
        #[qinvokable]
        fn remove(self: Pin<&mut Passwords>, id: i32);
        #[qinvokable]
        #[cxx_name = "removeAll"]
        fn remove_all(self: Pin<&mut Passwords>);
        #[qinvokable]
        fn exceptions(self: &Passwords) -> QString;
        #[qinvokable]
        #[cxx_name = "removeException"]
        fn remove_exception(self: Pin<&mut Passwords>, host: &QString);
        #[qinvokable]
        #[cxx_name = "importFile"]
        fn import_file(self: Pin<&mut Passwords>, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "exportFile"]
        fn export_file(self: &Passwords, path: &QString) -> QString;

        #[qsignal]
        fn changed(self: Pin<&mut Passwords>);
    }

    impl cxx_qt::Initialize for Passwords {}
    impl cxx_qt::Threading for Passwords {}
}

#[derive(Default)]
pub struct PasswordsRust {
    locked: bool,
    encrypted: bool,
    backend: QString,
    kwallet_available: bool,
    wallet_opening: bool,
    pending: HashMap<i32, Decision>,
    next: i32,
}

fn q(s: impl AsRef<str>) -> QString {
    QString::from(s.as_ref())
}
fn js(v: &Value) -> QString {
    q(v.to_string())
}

/// Only real web pages take part in autofill.
fn eligible(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn with_store<T>(f: impl FnOnce(&mut PasswordStore, &mut kite_core::rusqlite::Connection) -> T) -> T {
    let a = app();
    let mut store = a.passwords.lock().unwrap_or_else(|e| e.into_inner());
    let mut db = a.shared.db();
    f(&mut store, &mut db)
}

fn entry_json(e: &Entry, with_password: bool) -> Value {
    let mut v = json!({"id": e.id, "host": e.host, "username": e.username, "lastUsed": e.last_used});
    if with_password {
        v["password"] = e.password.clone().into();
    }
    v
}

impl cxx_qt::Initialize for qobject::Passwords {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().refresh();
        // Detect KWallet without blocking startup.
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let ok = kite_core::kwallet::KWallet::available();
            let _ = thread.queue(move |mut o| o.as_mut().set_kwallet_available(ok));
        });
        // Using KWallet: open it right away so autofill works (as Falkon does).
        if *self.backend() == q("kwallet") {
            self.open_wallet();
        }
    }
}

fn backend_name(b: Backend) -> &'static str {
    match b {
        Backend::Plain => "plain",
        Backend::Encrypted => "encrypted",
        Backend::KWallet => "kwallet",
    }
}

/// Open KWallet off the UI thread (it may show its own password prompt),
/// then hand it to the store on the Qt thread.
fn open_wallet_then(thread: cxx_qt::CxxQtThread<qobject::Passwords>, then: impl FnOnce(Pin<&mut qobject::Passwords>, Result<(), String>) + Send + 'static) {
    std::thread::spawn(move || {
        log::info!("opening KDE Wallet");
        let r = kite_core::kwallet::KWallet::open(0);
        match &r {
            Ok(_) => log::info!("KDE Wallet opened"),
            Err(e) => log::warn!("KDE Wallet could not be opened: {e}"),
        }
        let _ = thread.queue(move |mut o| {
            let res = r.map_err(|e| e.to_string()).and_then(|kw| with_store(|s, _| s.attach_wallet(kw)).map_err(|e| e.to_string()));
            o.as_mut().set_wallet_opening(false);
            then(o, res);
        });
    });
}

impl qobject::Passwords {
    fn refresh(mut self: Pin<&mut Self>) {
        let (locked, backend) = with_store(|s, db| (s.is_locked(db), s.backend(db)));
        self.as_mut().set_locked(locked);
        self.as_mut().set_encrypted(backend == Backend::Encrypted);
        self.as_mut().set_backend(q(backend_name(backend)));
    }

    fn open_wallet(mut self: Pin<&mut Self>) {
        if *self.wallet_opening() {
            return;
        }
        self.as_mut().set_wallet_opening(true);
        open_wallet_then(self.qt_thread(), |mut o, r| {
            o.as_mut().refresh();
            if r.is_ok() {
                o.as_mut().changed();
            }
            o.wallet_opened(q(r.err().unwrap_or_default()));
        });
    }

    /// target: "plain" | "encrypted" | "kwallet". `master` is the new master
    /// password when switching to "encrypted".
    fn switch_backend(mut self: Pin<&mut Self>, target: &QString, master: &QString) {
        let target = match target.to_string().as_str() {
            "kwallet" => Backend::KWallet,
            "encrypted" => Backend::Encrypted,
            _ => Backend::Plain,
        };
        let master = master.to_string();
        log::info!("switching password storage to {}", backend_name(target));
        let finish = move |mut o: Pin<&mut qobject::Passwords>, r: Result<(), String>| {
            let r = r.and_then(|_| with_store(|s, db| s.set_backend(db, target, (!master.is_empty()).then_some(master.as_str()))).map_err(|e| e.to_string()));
            match &r {
                Ok(_) => log::info!("password storage is now {}", backend_name(target)),
                Err(e) => log::warn!("switching password storage failed: {e}"),
            }
            o.as_mut().refresh();
            o.as_mut().changed();
            o.backend_switched(q(r.err().unwrap_or_default()));
        };
        if target == Backend::KWallet {
            self.as_mut().set_wallet_opening(true);
            open_wallet_then(self.qt_thread(), finish);
        } else {
            finish(self, Ok(()));
        }
    }

    fn unlock(mut self: Pin<&mut Self>, password: &QString) -> QString {
        if *self.backend() == q("kwallet") {
            self.open_wallet();
            return q("");
        }
        let r = with_store(|s, db| s.unlock(db, &password.to_string()));
        self.as_mut().refresh();
        match r {
            Ok(_) => {
                self.changed();
                q("")
            }
            Err(e) => q(e.to_string()),
        }
    }

    fn lock(mut self: Pin<&mut Self>) {
        with_store(|s, _| s.lock());
        self.as_mut().refresh();
        self.changed();
    }

    fn set_master_password(mut self: Pin<&mut Self>, current: &QString, new_password: &QString) -> QString {
        let cur = current.to_string();
        let r = with_store(|s, db| s.set_master_password(db, (!cur.is_empty()).then_some(cur.as_str()), &new_password.to_string()));
        self.as_mut().refresh();
        self.as_mut().changed();
        q(r.err().map(|e| e.to_string()).unwrap_or_default())
    }

    fn remove_master_password(mut self: Pin<&mut Self>, current: &QString) -> QString {
        let r = with_store(|s, db| s.remove_master_password(db, &current.to_string()));
        self.as_mut().refresh();
        self.as_mut().changed();
        q(r.err().map(|e| e.to_string()).unwrap_or_default())
    }

    /// A login form was submitted. Returns `null` or
    /// `{request, action: "save"|"update", host, username, locked}`.
    fn form_submitted(mut self: Pin<&mut Self>, page_url: &QString, private_mode: bool, json: &QString) -> QString {
        let url = page_url.to_string();
        let a = app();
        if !eligible(&url) || private_mode || a.private || !a.shared.settings().get_bool("passwords.save") {
            return q("null");
        }
        let v: Value = serde_json::from_str(&json.to_string()).unwrap_or(Value::Null);
        let (username, password, data) = (v["username"].as_str().unwrap_or(""), v["password"].as_str().unwrap_or(""), v["data"].as_str().unwrap_or(""));
        let (decision, locked) = match with_store(|s, db| s.decide(db, &url, username, password, data).map(|d| (d, s.is_locked(db)))) {
            Ok(x) => x,
            Err(e) => {
                log::warn!("password decision failed: {e}");
                return q("null");
            }
        };
        log::info!("login form submitted on {}: {:?}", create_host(&url), std::mem::discriminant(&decision));
        let (action, entry) = match &decision {
            Decision::Nothing => return q("null"),
            Decision::Save(e) => ("save", e.clone()),
            Decision::Update(e) => ("update", e.clone()),
        };
        let id = {
            let mut r = self.as_mut().rust_mut();
            r.next += 1;
            let id = r.next;
            // keep only a few pending prompts
            if r.pending.len() > 8 {
                let oldest = *r.pending.keys().min().unwrap();
                r.pending.remove(&oldest);
            }
            r.pending.insert(id, decision);
            id
        };
        js(&json!({"request": id, "action": action, "host": entry.host, "username": entry.username, "locked": locked}))
    }

    /// `choice`: "save" | "never" | "dismiss". Returns an error message or "".
    fn answer(mut self: Pin<&mut Self>, request: i32, choice: &QString) -> QString {
        let Some(d) = self.as_mut().rust_mut().pending.remove(&request) else { return q("") };
        let host = match &d {
            Decision::Save(e) | Decision::Update(e) => e.host.clone(),
            Decision::Nothing => String::new(),
        };
        let r = match choice.to_string().as_str() {
            "save" => with_store(|s, db| {
                // decide() could not compare entries while locked: redo it now.
                let d = match &d {
                    Decision::Save(e) => s.decide(db, &format!("https://{}/", e.host), &e.username, &e.password, &e.data)?,
                    other => other.clone(),
                };
                s.apply(db, &d)
            }),
            "never" => with_store(|_, db| PasswordStore::add_exception(db, &host)),
            _ => Ok(()),
        };
        self.as_mut().changed();
        q(r.err().map(|e| e.to_string()).unwrap_or_default())
    }

    /// Saved logins for a page (no passwords).
    fn logins(&self, page_url: &QString) -> QString {
        let url = page_url.to_string();
        if !eligible(&url) {
            return q("[]");
        }
        let list = with_store(|s, db| if s.is_locked(db) { Ok(vec![]) } else { s.entries_for(db, &url) }).unwrap_or_default();
        js(&Value::Array(list.iter().map(|e| entry_json(e, false)).collect()))
    }

    /// Credentials to fill into the page — only for an entry belonging to
    /// the page's own host.
    fn fill_data(&self, page_url: &QString, id: i32) -> QString {
        let url = page_url.to_string();
        if !eligible(&url) {
            return q("null");
        }
        let entry = with_store(|s, db| {
            let found = s.entries_for(db, &url).ok()?.into_iter().find(|e| e.id == id as i64)?;
            let _ = s.touch(db, found.id);
            Some(found)
        });
        match entry {
            Some(e) if e.host == create_host(&url) => js(&json!({"username": e.username, "password": e.password, "data": e.data})),
            _ => q("null"),
        }
    }

    fn autofill_enabled(&self) -> bool {
        app().shared.settings().get_bool("passwords.autofill")
    }

    fn http_auth(&self, url: &QString) -> QString {
        let url = url.to_string();
        let e = with_store(|s, db| if s.is_locked(db) { None } else { s.entries_for(db, &url).ok().and_then(|v| v.into_iter().next()) });
        match e {
            Some(e) => js(&json!({"username": e.username, "password": e.password})),
            None => q("null"),
        }
    }

    fn save_http_auth(mut self: Pin<&mut Self>, url: &QString, private_mode: bool, username: &QString, password: &QString) {
        let a = app();
        if private_mode || a.private {
            return;
        }
        let url = url.to_string();
        let r = with_store(|s, db| {
            let d = s.decide(db, &url, &username.to_string(), &password.to_string(), "")?;
            s.apply(db, &d)
        });
        if let Err(e) = r {
            log::warn!("saving HTTP credentials failed: {e}");
        }
        self.as_mut().changed();
    }

    // ---- manager ------------------------------------------------------------
    fn list(&self) -> QString {
        let list = with_store(|s, db| s.all(db)).unwrap_or_default();
        js(&Value::Array(list.iter().map(|e| entry_json(e, true)).collect()))
    }

    fn update(mut self: Pin<&mut Self>, json: &QString) -> QString {
        let v: Value = serde_json::from_str(&json.to_string()).unwrap_or(Value::Null);
        let r = with_store(|s, db| {
            let mut e = s.all(db)?.into_iter().find(|e| Some(e.id) == v["id"].as_i64()).ok_or_else(|| anyhow::anyhow!("entry not found"))?;
            if let Some(h) = v["host"].as_str() {
                e.host = h.trim().to_owned();
            }
            if let Some(u) = v["username"].as_str() {
                e.username = u.to_owned();
            }
            if let Some(p) = v["password"].as_str() {
                e.password = p.to_owned();
            }
            s.update(db, &e)
        });
        self.as_mut().changed();
        q(r.err().map(|e| e.to_string()).unwrap_or_default())
    }

    fn add(mut self: Pin<&mut Self>, json: &QString) -> QString {
        let v: Value = serde_json::from_str(&json.to_string()).unwrap_or(Value::Null);
        let site = v["host"].as_str().unwrap_or("").trim().to_owned();
        let host = if site.contains("://") { create_host(&site) } else { site };
        if host.is_empty() || v["password"].as_str().unwrap_or("").is_empty() {
            return q("Site and password are required");
        }
        let e = Entry { host, username: v["username"].as_str().unwrap_or("").into(), password: v["password"].as_str().unwrap_or("").into(), ..Default::default() };
        let r = with_store(|s, db| s.add(db, &e));
        self.as_mut().changed();
        q(r.err().map(|e| e.to_string()).unwrap_or_default())
    }

    fn remove(mut self: Pin<&mut Self>, id: i32) {
        let _ = with_store(|s, db| s.remove(db, id as i64));
        self.as_mut().changed();
    }

    fn remove_all(mut self: Pin<&mut Self>) {
        let _ = with_store(|s, db| s.remove_all(db));
        self.as_mut().changed();
    }

    fn exceptions(&self) -> QString {
        js(&json!(with_store(|_, db| PasswordStore::exceptions(db)).unwrap_or_default()))
    }

    fn remove_exception(mut self: Pin<&mut Self>, host: &QString) {
        let _ = with_store(|_, db| PasswordStore::remove_exception(db, &host.to_string()));
        self.as_mut().changed();
    }

    fn import_file(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let p = path.to_string();
        let p = p.strip_prefix("file://").unwrap_or(&p).to_owned();
        let r = std::fs::read_to_string(&p).map_err(anyhow::Error::from).and_then(|text| with_store(|s, db| s.import(db, &text)));
        self.as_mut().changed();
        match r {
            Ok((a, u)) => q(format!("Imported {a} new and {u} updated logins")),
            Err(e) => q(format!("Import failed: {e}")),
        }
    }

    /// Export to `.csv` (Firefox/Chrome compatible) or `.xml` (Falkon).
    fn export_file(&self, path: &QString) -> QString {
        let p = path.to_string();
        let p = p.strip_prefix("file://").unwrap_or(&p).to_owned();
        let r = with_store(|s, db| if p.ends_with(".xml") { s.export_falkon_xml(db) } else { s.export_csv(db) }).and_then(|text| {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            // The file contains passwords in clear text: owner-only permissions.
            let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&p)?;
            f.write_all(text.as_bytes())?;
            Ok(())
        });
        match r {
            Ok(_) => q(format!("Passwords exported to {p}")),
            Err(e) => q(format!("Export failed: {e}")),
        }
    }
}
