//! `FormFill` — QML singleton for form autofill (saved addresses and
//! contact details) and the password generator.
//!
//! As with `Passwords`: page-facing calls take the URL from the view, only
//! http(s) pages qualify, pending "save this address?" decisions stay here
//! (QML only sees an id and a label), and private windows never save.

use crate::state::app;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use kite_core::formfill::{obtain_key, Address, Decision, FormFillStore};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
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
        /// The key is loaded: saved entries can be read and written.
        #[qproperty(bool, unlocked)]
        #[qproperty(bool, unlocking)]
        /// "kwallet" | "file" | "" (no key made yet).
        #[qproperty(QString, key_store, cxx_name = "keyStore")]
        /// Why the key could not be loaded ("" when fine).
        #[qproperty(QString, error)]
        #[qproperty(i32, count)]
        type FormFill = super::FormFillRust;

        #[qinvokable]
        fn enabled(self: &FormFill) -> bool;
        #[qinvokable]
        fn unlock(self: Pin<&mut FormFill>);

        // page integration (URLs come from the view, not the page)
        #[qinvokable]
        fn suggestions(self: &FormFill, page_url: &QString, field: &QString, prefix: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "fillData"]
        fn fill_data(self: &FormFill, page_url: &QString, id: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "formSubmitted"]
        fn form_submitted(self: Pin<&mut FormFill>, page_url: &QString, private_mode: bool, json: &QString) -> QString;
        #[qinvokable]
        fn answer(self: Pin<&mut FormFill>, request: i32, choice: &QString);

        // manager UI
        #[qinvokable]
        fn list(self: &FormFill) -> QString;
        #[qinvokable]
        fn save(self: Pin<&mut FormFill>, json: &QString) -> QString;
        #[qinvokable]
        fn remove(self: Pin<&mut FormFill>, id: i32);
        #[qinvokable]
        #[cxx_name = "removeAll"]
        fn remove_all(self: Pin<&mut FormFill>);

        // password generator
        #[qinvokable]
        #[cxx_name = "generatePassword"]
        fn generate_password(self: &FormFill) -> QString;

        #[qsignal]
        fn changed(self: Pin<&mut FormFill>);
    }

    impl cxx_qt::Initialize for FormFill {}
    impl cxx_qt::Threading for FormFill {}
}

#[derive(Default)]
pub struct FormFillRust {
    unlocked: bool,
    unlocking: bool,
    key_store: QString,
    error: QString,
    count: i32,
    pending: HashMap<i32, Decision>,
    next: i32,
    /// Run once the key is loaded (an answered prompt or a manager save).
    after_unlock: Vec<Box<dyn FnOnce(Pin<&mut qobject::FormFill>)>>,
}

fn q(s: impl AsRef<str>) -> QString {
    QString::from(s.as_ref())
}

fn eligible(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn with_store<T>(f: impl FnOnce(&mut FormFillStore, &kite_core::rusqlite::Connection) -> T) -> T {
    let a = app();
    let mut store = a.formfill.lock().unwrap_or_else(|e| e.into_inner());
    let db = a.shared.db();
    f(&mut store, &db)
}

fn address_json(a: &Address) -> Value {
    let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
    v["id"] = a.id.into();
    v["label"] = a.label().into();
    v
}

impl cxx_qt::Initialize for qobject::FormFill {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().refresh();
        // Saved entries: load the key now (off the UI thread), so suggestions
        // are there when the first form is focused.
        if *self.count() > 0 && self.enabled() {
            self.unlock();
        }
    }
}

impl qobject::FormFill {
    fn refresh(mut self: Pin<&mut Self>) {
        let (unlocked, count, store) = with_store(|s, db| (s.is_unlocked(), FormFillStore::count(db) as i32, FormFillStore::meta_keystore(db)));
        self.as_mut().set_unlocked(unlocked);
        self.as_mut().set_count(count);
        self.as_mut().set_key_store(q(store.map(|s| s.as_str()).unwrap_or("")));
    }

    fn enabled(&self) -> bool {
        app().shared.settings().get_bool("browsing.formFill")
    }

    /// Load (or, for the first entry, make) the key on a worker thread.
    fn unlock(mut self: Pin<&mut Self>) {
        if *self.unlocked() {
            self.run_after_unlock();
            return;
        }
        if *self.unlocking() {
            return;
        }
        self.as_mut().set_unlocking(true);
        let thread = self.qt_thread();
        let store = with_store(|_, db| FormFillStore::meta_keystore(db));
        let dir = app().shared.profile.root.clone();
        std::thread::spawn(move || {
            let r = obtain_key(&dir, store);
            let _ = thread.queue(move |mut o| {
                let r = r.and_then(|(key, store)| with_store(|s, db| s.set_key(db, key, store)));
                o.as_mut().set_unlocking(false);
                match r {
                    Ok(()) => {
                        o.as_mut().set_error(q(""));
                        o.as_mut().refresh();
                        o.as_mut().run_after_unlock();
                        o.as_mut().changed();
                    }
                    Err(e) => {
                        log::warn!("saved form data could not be unlocked: {e}");
                        o.as_mut().rust_mut().after_unlock.clear();
                        o.as_mut().set_error(q(e.to_string()));
                    }
                }
            });
        });
    }

    fn run_after_unlock(mut self: Pin<&mut Self>) {
        let jobs = std::mem::take(&mut self.as_mut().rust_mut().after_unlock);
        for job in jobs {
            job(self.as_mut());
        }
    }

    /// Do `f` with the key loaded (now, or once it is).
    fn when_unlocked(mut self: Pin<&mut Self>, f: impl FnOnce(Pin<&mut qobject::FormFill>) + 'static) {
        self.as_mut().rust_mut().after_unlock.push(Box::new(f));
        self.unlock();
    }

    /// `[{id, label, detail}]` for a focused field of kind `field`.
    fn suggestions(&self, page_url: &QString, field: &QString, prefix: &QString) -> QString {
        if !eligible(&page_url.to_string()) || !self.enabled() || !*self.unlocked() {
            return q("[]");
        }
        let list = with_store(|s, db| s.suggestions(db, &field.to_string(), &prefix.to_string())).unwrap_or_default();
        q(serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()))
    }

    /// `{kind: value}` for every kind the entry has a value for.
    fn fill_data(&self, page_url: &QString, id: i32) -> QString {
        if !eligible(&page_url.to_string()) || !self.enabled() {
            return q("null");
        }
        let a = with_store(|s, db| {
            let a = s.get(db, id as i64).ok().flatten()?;
            let _ = s.touch(db, a.id);
            Some(a)
        });
        match a {
            Some(a) => q(json!(a.values()).to_string()),
            None => q("null"),
        }
    }

    /// A form with address fields was submitted (`json`: kind → value).
    /// Returns `null` or `{request, action: "save"|"update", label}`.
    fn form_submitted(mut self: Pin<&mut Self>, page_url: &QString, private_mode: bool, json: &QString) -> QString {
        let a = app();
        if !eligible(&page_url.to_string()) || private_mode || a.private || !self.enabled() || !a.shared.settings().get_bool("browsing.formFillSave") {
            return q("null");
        }
        let fields: BTreeMap<String, String> = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let new = Address::from_fields(&fields);
        // Locked with saved entries: they can't be compared, so no prompt.
        let decision = if *self.unlocked() || *self.count() == 0 { with_store(|s, db| s.decide(db, &new)).unwrap_or(Decision::Nothing) } else { Decision::Nothing };
        let (action, entry) = match &decision {
            Decision::Nothing => return q("null"),
            Decision::Save(e) => ("save", e.clone()),
            Decision::Update(e) => ("update", e.clone()),
        };
        let id = {
            let mut r = self.as_mut().rust_mut();
            r.next += 1;
            let id = r.next;
            if r.pending.len() > 8 {
                let oldest = *r.pending.keys().min().unwrap();
                r.pending.remove(&oldest);
            }
            r.pending.insert(id, decision);
            id
        };
        q(json!({"request": id, "action": action, "label": entry.label()}).to_string())
    }

    /// `choice`: "save" | "dismiss".
    fn answer(mut self: Pin<&mut Self>, request: i32, choice: &QString) {
        let Some(d) = self.as_mut().rust_mut().pending.remove(&request) else { return };
        if choice.to_string() != "save" {
            return;
        }
        self.when_unlocked(move |mut o| {
            let r = with_store(|s, db| {
                // Decided before the key was there: compare again now.
                let d = match &d {
                    Decision::Save(a) => s.decide(db, a)?,
                    other => other.clone(),
                };
                s.apply(db, &d)
            });
            if let Err(e) = r {
                log::warn!("saving form data failed: {e}");
            }
            o.as_mut().refresh();
            o.changed();
        });
    }

    fn list(&self) -> QString {
        let list = with_store(|s, db| s.all(db)).unwrap_or_default();
        q(Value::Array(list.iter().map(address_json).collect()).to_string())
    }

    /// Add (no id) or change an entry. Returns an error message or "".
    fn save(mut self: Pin<&mut Self>, json: &QString) -> QString {
        let v: Value = serde_json::from_str(&json.to_string()).unwrap_or(Value::Null);
        let Ok(mut a) = serde_json::from_value::<Address>(v.clone()) else { return q("Invalid entry") };
        a.id = v["id"].as_i64().unwrap_or(0);
        if a.is_empty() {
            return q("Fill in at least one field");
        }
        self.as_mut().when_unlocked(move |mut o| {
            let r = with_store(|s, db| if a.id > 0 { s.update(db, &a) } else { s.add(db, &a).map(|_| ()) });
            if let Err(e) = r {
                log::warn!("saving form data failed: {e}");
            }
            o.as_mut().refresh();
            o.changed();
        });
        q("")
    }

    fn remove(mut self: Pin<&mut Self>, id: i32) {
        let _ = with_store(|s, db| s.remove(db, id as i64));
        self.as_mut().refresh();
        self.changed();
    }

    /// Remove every entry. With the key lost (`error` set) this also forgets
    /// the key record, so new entries get a new key.
    fn remove_all(mut self: Pin<&mut Self>) {
        let lost = !self.error().is_empty();
        let _ = with_store(|s, db| if lost || !s.is_unlocked() { s.reset(db) } else { s.remove_all(db) });
        self.as_mut().set_error(q(""));
        self.as_mut().refresh();
        self.changed();
    }

    fn generate_password(&self) -> QString {
        let (len, symbols) = {
            let s = app().shared.settings();
            (s.get("passwords.generatorLength").as_u64().unwrap_or(20) as usize, s.get_bool("passwords.generatorSymbols"))
        };
        q(kite_core::passwords::generate_password(len, symbols))
    }
}
