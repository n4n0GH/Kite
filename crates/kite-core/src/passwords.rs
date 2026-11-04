//! Password manager (Falkon's AutoFill + PasswordManager + backends).
//!
//! Two storage backends live in `browsedata.db`:
//!
//! * **plain** — Falkon's `autofill` table, unchanged, so passwords saved by
//!   Falkon keep working (and vice versa).
//! * **encrypted** — `kite_passwords_encrypted`: username, password and form
//!   data are sealed with XChaCha20-Poly1305 under a key derived from a master
//!   password with Argon2id. The host stays in clear text for lookups (as in
//!   Falkon's encrypted backend). The key only exists in memory while unlocked.
//!
//! Sites the user said "never" to are kept in Falkon's `autofill_exceptions`.

use argon2::Argon2;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Zeroize)]
pub struct Entry {
    pub id: i64,
    /// `host[:port]` (Falkon's PasswordManager::createHost).
    pub host: String,
    pub username: String,
    pub password: String,
    /// URL-encoded form fields (`name=value&...`) used to refill the form.
    pub data: String,
    pub last_used: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Plain,
    Encrypted,
    /// KDE Wallet (shared with Falkon's KWallet backend).
    KWallet,
}

/// Loaded KWallet folder.
struct Wallet {
    kw: crate::kwallet::KWallet,
    /// (wallet key, entry with a session-local numeric id)
    entries: Vec<(String, Entry)>,
    next_id: i64,
}

fn wallet_map(e: &Entry) -> std::collections::BTreeMap<String, String> {
    [
        ("host", e.host.clone()),
        ("username", e.username.clone()),
        ("password", e.password.clone()),
        ("updated", (e.last_used / 1000).to_string()),
        ("data", e.data.clone()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

/// What to do after a login form was submitted.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Already stored with the same password, storing disabled, or excepted.
    Nothing,
    Save(Entry),
    /// Same username, new password: update entry `id`.
    Update(Entry),
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS autofill (
    id INTEGER PRIMARY KEY, server TEXT NOT NULL, data TEXT, password TEXT,
    username TEXT, last_used INTEGER DEFAULT 0 NOT NULL);
CREATE TABLE IF NOT EXISTS autofill_exceptions (id INTEGER PRIMARY KEY, server TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS kite_passwords_encrypted (
    id INTEGER PRIMARY KEY, server TEXT NOT NULL, username_enc TEXT NOT NULL,
    password_enc TEXT NOT NULL, data_enc TEXT NOT NULL, last_used INTEGER DEFAULT 0 NOT NULL);
CREATE INDEX IF NOT EXISTS kite_passwords_encrypted_server ON kite_passwords_encrypted (server);
CREATE TABLE IF NOT EXISTS kite_password_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

const VERIFIER_PLAINTEXT: &[u8] = b"kite-password-store-v1";

/// `host[:port]` for a URL, or the whole URL if it has no host.
pub fn create_host(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.to_owned(),
            _ => url.to_owned(),
        },
        Err(_) => url.to_owned(),
    }
}

fn b64(v: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(v)
}
fn unb64(s: &str) -> anyhow::Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(s)?)
}

fn derive_key(password: &str, salt: &[u8]) -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0u8; 32]);
    // Argon2id, 64 MiB, 3 passes: deliberately slow to brute-force.
    let params = argon2::Params::new(64 * 1024, 3, 1, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?;
    Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| anyhow::anyhow!("key derivation failed: {e}"))?;
    Ok(key)
}

pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("system RNG");
    b
}

pub(crate) struct Sealer(XChaCha20Poly1305);

impl Sealer {
    pub(crate) fn new(key: &[u8; 32]) -> Self {
        Sealer(XChaCha20Poly1305::new(Key::from_slice(key)))
    }
    pub(crate) fn seal(&self, plain: &[u8]) -> String {
        let nonce = random::<24>();
        let ct = self.0.encrypt(XNonce::from_slice(&nonce), plain).expect("encryption cannot fail");
        let mut out = nonce.to_vec();
        out.extend(ct);
        b64(&out)
    }
    pub(crate) fn open(&self, sealed: &str) -> anyhow::Result<Vec<u8>> {
        let raw = unb64(sealed)?;
        if raw.len() < 24 {
            anyhow::bail!("corrupted entry");
        }
        self.0
            .decrypt(XNonce::from_slice(&raw[..24]), &raw[24..])
            .map_err(|_| anyhow::anyhow!("wrong master password or corrupted entry"))
    }
    pub(crate) fn open_str(&self, sealed: &str) -> anyhow::Result<String> {
        Ok(String::from_utf8(self.open(sealed)?)?)
    }
}

pub struct PasswordStore {
    key: Option<Zeroizing<[u8; 32]>>,
    wallet: Option<Wallet>,
}

impl PasswordStore {
    pub fn open(db: &Connection) -> anyhow::Result<Self> {
        db.execute_batch(SCHEMA)?;
        Ok(PasswordStore { key: None, wallet: None })
    }

    fn meta(db: &Connection, key: &str) -> Option<String> {
        db.query_row("SELECT value FROM kite_password_meta WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }
    fn set_meta(db: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
        db.execute("INSERT OR REPLACE INTO kite_password_meta (key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
    }

    pub fn backend(&self, db: &Connection) -> Backend {
        match Self::meta(db, "backend").as_deref() {
            Some("encrypted") => Backend::Encrypted,
            Some("kwallet") => Backend::KWallet,
            _ => Backend::Plain,
        }
    }

    /// Encrypted backend without the key in memory.
    pub fn is_locked(&self, db: &Connection) -> bool {
        match self.backend(db) {
            Backend::Plain => false,
            Backend::Encrypted => self.key.is_none(),
            Backend::KWallet => self.wallet.is_none(),
        }
    }

    pub fn lock(&mut self) {
        self.key = None;
        self.wallet = None;
    }

    /// Use an opened wallet (KWallet backend). Opening is done by the caller,
    /// off the UI thread, because KWallet may ask for the wallet password.
    pub fn attach_wallet(&mut self, kw: crate::kwallet::KWallet) -> anyhow::Result<()> {
        let mut w = Wallet { kw, entries: vec![], next_id: 1 };
        for (key, m) in w.kw.entries()? {
            let g = |k: &str| m.get(k).cloned().unwrap_or_default();
            // Falkon also stores "exists" marker maps keyed by host; skip them.
            if g("host").is_empty() || g("password").is_empty() {
                continue;
            }
            let id = w.next_id;
            w.next_id += 1;
            let last_used = g("updated").parse::<i64>().unwrap_or(0) * 1000;
            w.entries.push((key, Entry { id, host: g("host"), username: g("username"), password: g("password"), data: g("data"), last_used }));
        }
        self.wallet = Some(w);
        Ok(())
    }

    fn wallet(&self) -> anyhow::Result<&Wallet> {
        self.wallet.as_ref().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))
    }

    /// Switch storage. Entries move along: they must be readable now (the
    /// current store unlocked) and the target must be usable (wallet
    /// attached / `master` given for Encrypted).
    pub fn set_backend(&mut self, db: &mut Connection, target: Backend, master: Option<&str>) -> anyhow::Result<()> {
        let current = self.backend(db);
        if current == target {
            return Ok(());
        }
        let entries = self.all(db)?;
        match target {
            Backend::KWallet => {
                let w = self.wallet.as_mut().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))?;
                for e in &entries {
                    if w.entries.iter().any(|(_, x)| x.host == e.host && x.username == e.username && x.password == e.password) {
                        continue;
                    }
                    let key = format!("{{{}}}", uuid::Uuid::new_v4());
                    w.kw.write(&key, &wallet_map(e))?;
                    let id = w.next_id;
                    w.next_id += 1;
                    w.entries.push((key, Entry { id, ..e.clone() }));
                }
                let tx = db.transaction()?;
                tx.execute("DELETE FROM autofill", [])?;
                tx.execute("DELETE FROM kite_passwords_encrypted", [])?;
                tx.execute("DELETE FROM kite_password_meta", [])?;
                Self::set_meta(&tx, "backend", "kwallet")?;
                tx.commit()?;
                let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
                self.key = None;
            }
            Backend::Plain | Backend::Encrypted => {
                // Leaving an encrypted store: decrypt to plain first.
                let tx = db.transaction()?;
                tx.execute("DELETE FROM autofill", [])?;
                tx.execute("DELETE FROM kite_passwords_encrypted", [])?;
                tx.execute("DELETE FROM kite_password_meta", [])?;
                for e in &entries {
                    tx.execute(
                        "INSERT INTO autofill (server, data, password, username, last_used) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![e.host, e.data, e.password, e.username, e.last_used],
                    )?;
                }
                tx.commit()?;
                // Wallet entries are left in place (Falkon may still use them).
                self.wallet = None;
                self.key = None;
                if target == Backend::Encrypted {
                    self.set_master_password(db, None, master.ok_or_else(|| anyhow::anyhow!("a master password is required"))?)?;
                }
            }
        }
        Ok(())
    }

    fn check_key(db: &Connection, key: &[u8; 32]) -> bool {
        Self::meta(db, "verifier").map(|v| Sealer::new(key).open(&v).map(|p| p == VERIFIER_PLAINTEXT).unwrap_or(false)).unwrap_or(false)
    }

    fn key_for(db: &Connection, password: &str) -> anyhow::Result<Zeroizing<[u8; 32]>> {
        let salt = unb64(&Self::meta(db, "salt").ok_or_else(|| anyhow::anyhow!("no master password set"))?)?;
        derive_key(password, &salt)
    }

    pub fn unlock(&mut self, db: &Connection, password: &str) -> anyhow::Result<()> {
        let key = Self::key_for(db, password)?;
        if !Self::check_key(db, &key) {
            anyhow::bail!("Wrong master password");
        }
        self.key = Some(key);
        Ok(())
    }

    fn sealer(&self, db: &Connection) -> anyhow::Result<Option<Sealer>> {
        match self.backend(db) {
            Backend::Plain | Backend::KWallet => Ok(None),
            Backend::Encrypted => match &self.key {
                Some(k) => Ok(Some(Sealer::new(k))),
                None => anyhow::bail!("The password store is locked"),
            },
        }
    }

    /// Turn on encryption (or change the master password). All entries are
    /// moved/re-encrypted. `current` is required when already encrypted.
    pub fn set_master_password(&mut self, db: &mut Connection, current: Option<&str>, new: &str) -> anyhow::Result<()> {
        if new.is_empty() {
            anyhow::bail!("The master password must not be empty");
        }
        match self.backend(db) {
            Backend::Encrypted => self.unlock(db, current.unwrap_or(""))?,
            Backend::KWallet => return self.set_backend(db, Backend::Encrypted, Some(new)),
            Backend::Plain => {}
        }
        let entries = self.all(db)?;
        let salt = random::<16>();
        let key = derive_key(new, &salt)?;
        let s = Sealer::new(&key);
        let tx = db.transaction()?;
        tx.execute("DELETE FROM kite_passwords_encrypted", [])?;
        for e in &entries {
            tx.execute(
                "INSERT INTO kite_passwords_encrypted (server, username_enc, password_enc, data_enc, last_used) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.host, s.seal(e.username.as_bytes()), s.seal(e.password.as_bytes()), s.seal(e.data.as_bytes()), e.last_used],
            )?;
        }
        tx.execute("DELETE FROM autofill", [])?;
        Self::set_meta(&tx, "salt", &b64(&salt))?;
        Self::set_meta(&tx, "verifier", &s.seal(VERIFIER_PLAINTEXT))?;
        Self::set_meta(&tx, "backend", "encrypted")?;
        tx.commit()?;
        // Plain-text copies were in the WAL/free pages; compact them away.
        let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
        self.key = Some(key);
        Ok(())
    }

    /// Turn encryption off (back to Falkon's plain table).
    pub fn remove_master_password(&mut self, db: &mut Connection, current: &str) -> anyhow::Result<()> {
        if self.backend(db) != Backend::Encrypted {
            return Ok(());
        }
        self.unlock(db, current)?;
        let entries = self.all(db)?;
        let tx = db.transaction()?;
        for e in &entries {
            tx.execute(
                "INSERT INTO autofill (server, data, password, username, last_used) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.host, e.data, e.password, e.username, e.last_used],
            )?;
        }
        tx.execute("DELETE FROM kite_passwords_encrypted", [])?;
        tx.execute("DELETE FROM kite_password_meta", [])?;
        tx.commit()?;
        self.key = None;
        Ok(())
    }

    fn query(&self, db: &Connection, host: Option<&str>) -> anyhow::Result<Vec<Entry>> {
        let mut out = vec![];
        if self.backend(db) == Backend::KWallet {
            let mut v: Vec<Entry> = self.wallet()?.entries.iter().map(|(_, e)| e).filter(|e| host.map(|h| e.host == h).unwrap_or(true)).cloned().collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.last_used));
            return Ok(v);
        }
        match self.sealer(db)? {
            None => {
                let sql = format!(
                    "SELECT id, server, username, password, data, last_used FROM autofill {} ORDER BY last_used DESC",
                    if host.is_some() { "WHERE server = ?1" } else { "" }
                );
                let mut stmt = db.prepare(&sql)?;
                let map = |r: &rusqlite::Row| {
                    Ok(Entry {
                        id: r.get(0)?,
                        host: r.get(1)?,
                        username: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        password: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        data: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        last_used: r.get(5)?,
                    })
                };
                let rows = match host {
                    Some(h) => stmt.query_map([h], map)?.collect::<Result<Vec<_>, _>>()?,
                    None => stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?,
                };
                out.extend(rows);
            }
            Some(s) => {
                let sql = format!(
                    "SELECT id, server, username_enc, password_enc, data_enc, last_used FROM kite_passwords_encrypted {} ORDER BY last_used DESC",
                    if host.is_some() { "WHERE server = ?1" } else { "" }
                );
                let mut stmt = db.prepare(&sql)?;
                let map = |r: &rusqlite::Row| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)?));
                let rows = match host {
                    Some(h) => stmt.query_map([h], map)?.collect::<Result<Vec<_>, _>>()?,
                    None => stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?,
                };
                for (id, host, u, p, d, last_used) in rows {
                    out.push(Entry { id, host, username: s.open_str(&u)?, password: s.open_str(&p)?, data: s.open_str(&d)?, last_used });
                }
            }
        }
        Ok(out)
    }

    /// Entries for a page URL, most recently used first.
    pub fn entries_for(&self, db: &Connection, url: &str) -> anyhow::Result<Vec<Entry>> {
        self.query(db, Some(&create_host(url)))
    }

    pub fn all(&self, db: &Connection) -> anyhow::Result<Vec<Entry>> {
        let mut v = self.query(db, None)?;
        v.sort_by(|a, b| a.host.cmp(&b.host).then(a.username.cmp(&b.username)));
        Ok(v)
    }

    pub fn add(&mut self, db: &Connection, e: &Entry) -> anyhow::Result<i64> {
        let now = crate::util::now_ms();
        if self.backend(db) == Backend::KWallet {
            let w = self.wallet.as_mut().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))?;
            let key = format!("{{{}}}", uuid::Uuid::new_v4());
            let entry = Entry { id: w.next_id, last_used: now, ..e.clone() };
            w.kw.write(&key, &wallet_map(&entry))?;
            w.next_id += 1;
            w.entries.push((key, entry.clone()));
            return Ok(entry.id);
        }
        match self.sealer(db)? {
            None => db.execute(
                "INSERT INTO autofill (server, data, password, username, last_used) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.host, e.data, e.password, e.username, now],
            )?,
            Some(s) => db.execute(
                "INSERT INTO kite_passwords_encrypted (server, username_enc, password_enc, data_enc, last_used) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.host, s.seal(e.username.as_bytes()), s.seal(e.password.as_bytes()), s.seal(e.data.as_bytes()), now],
            )?,
        };
        Ok(db.last_insert_rowid())
    }

    pub fn update(&mut self, db: &Connection, e: &Entry) -> anyhow::Result<()> {
        let now = crate::util::now_ms();
        if self.backend(db) == Backend::KWallet {
            let w = self.wallet.as_mut().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))?;
            let (key, slot) = w.entries.iter_mut().find(|(_, x)| x.id == e.id).ok_or_else(|| anyhow::anyhow!("no such password entry"))?;
            let entry = Entry { last_used: now, ..e.clone() };
            w.kw.write(key, &wallet_map(&entry))?;
            *slot = entry;
            return Ok(());
        }
        let n = match self.sealer(db)? {
            None => db.execute(
                "UPDATE autofill SET server = ?2, data = ?3, password = ?4, username = ?5, last_used = ?6 WHERE id = ?1",
                params![e.id, e.host, e.data, e.password, e.username, now],
            )?,
            Some(s) => db.execute(
                "UPDATE kite_passwords_encrypted SET server = ?2, username_enc = ?3, password_enc = ?4, data_enc = ?5, last_used = ?6 WHERE id = ?1",
                params![e.id, e.host, s.seal(e.username.as_bytes()), s.seal(e.password.as_bytes()), s.seal(e.data.as_bytes()), now],
            )?,
        };
        if n == 0 {
            anyhow::bail!("no such password entry");
        }
        Ok(())
    }

    pub fn touch(&mut self, db: &Connection, id: i64) -> anyhow::Result<()> {
        if self.backend(db) == Backend::KWallet {
            let w = self.wallet.as_mut().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))?;
            if let Some((key, e)) = w.entries.iter_mut().find(|(_, x)| x.id == id) {
                e.last_used = crate::util::now_ms();
                w.kw.write(key, &wallet_map(e))?;
            }
            return Ok(());
        }
        let table = if self.backend(db) == Backend::Encrypted { "kite_passwords_encrypted" } else { "autofill" };
        db.execute(&format!("UPDATE {table} SET last_used = ?2 WHERE id = ?1"), params![id, crate::util::now_ms()])?;
        Ok(())
    }

    pub fn remove(&mut self, db: &Connection, id: i64) -> anyhow::Result<()> {
        if self.backend(db) == Backend::KWallet {
            let w = self.wallet.as_mut().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))?;
            if let Some(i) = w.entries.iter().position(|(_, x)| x.id == id) {
                let (key, _) = w.entries.remove(i);
                w.kw.remove(&key)?;
            }
            return Ok(());
        }
        let table = if self.backend(db) == Backend::Encrypted { "kite_passwords_encrypted" } else { "autofill" };
        db.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])?;
        Ok(())
    }

    pub fn remove_all(&mut self, db: &Connection) -> anyhow::Result<()> {
        if self.backend(db) == Backend::KWallet {
            let w = self.wallet.as_mut().ok_or_else(|| anyhow::anyhow!("KWallet is not open"))?;
            for (key, _) in w.entries.drain(..) {
                w.kw.remove(&key)?;
            }
            return Ok(());
        }
        db.execute("DELETE FROM autofill", [])?;
        db.execute("DELETE FROM kite_passwords_encrypted", [])?;
        Ok(())
    }

    // ---- exceptions ("never save for this site") --------------------------

    pub fn is_excepted(db: &Connection, url: &str) -> bool {
        let host = create_host(url);
        db.query_row("SELECT count(id) FROM autofill_exceptions WHERE server = ?1", [&host], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
    }
    pub fn add_exception(db: &Connection, url_or_host: &str) -> anyhow::Result<()> {
        let host = if url_or_host.contains("://") { create_host(url_or_host) } else { url_or_host.to_owned() };
        if !Self::exceptions(db)?.contains(&host) {
            db.execute("INSERT INTO autofill_exceptions (server) VALUES (?1)", [host])?;
        }
        Ok(())
    }
    pub fn remove_exception(db: &Connection, host: &str) -> anyhow::Result<()> {
        db.execute("DELETE FROM autofill_exceptions WHERE server = ?1", [host])?;
        Ok(())
    }
    pub fn exceptions(db: &Connection) -> anyhow::Result<Vec<String>> {
        let mut stmt = db.prepare("SELECT server FROM autofill_exceptions ORDER BY server")?;
        let v = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok(v)
    }

    /// Falkon's AutoFill::saveForm logic.
    pub fn decide(&mut self, db: &Connection, url: &str, username: &str, password: &str, data: &str) -> anyhow::Result<Decision> {
        if password.is_empty() || Self::is_excepted(db, url) {
            return Ok(Decision::Nothing);
        }
        let new = Entry { id: 0, host: create_host(url), username: username.into(), password: password.into(), data: data.into(), last_used: 0 };
        if self.is_locked(db) {
            // We can't compare against stored entries; offer to save, the UI
            // will ask for the master password first.
            return Ok(Decision::Save(new));
        }
        for e in self.entries_for(db, url)? {
            if e.username == username {
                if e.password == password {
                    self.touch(db, e.id)?;
                    return Ok(Decision::Nothing);
                }
                return Ok(Decision::Update(Entry { id: e.id, ..new }));
            }
        }
        Ok(Decision::Save(new))
    }

    /// Commit a decision taken by `decide` once the user agreed.
    pub fn apply(&mut self, db: &Connection, d: &Decision) -> anyhow::Result<()> {
        match d {
            Decision::Nothing => Ok(()),
            Decision::Save(e) => self.add(db, e).map(|_| ()),
            Decision::Update(e) => self.update(db, e),
        }
    }

    // ---- import / export ------------------------------------------------------

    /// Falkon's password export format (XML).
    pub fn export_falkon_xml(&self, db: &Connection) -> anyhow::Result<String> {
        fn esc(s: &str) -> String {
            s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
        }
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<passwords version=\"1.0\">\n");
        for e in self.all(db)? {
            out += &format!(
                "    <entry>\n        <server>{}</server>\n        <username>{}</username>\n        <password>{}</password>\n        <data>{}</data>\n    </entry>\n",
                esc(&e.host), esc(&e.username), esc(&e.password), esc(&e.data)
            );
        }
        for x in Self::exceptions(db)? {
            out += &format!("    <exception>\n        <server>{}</server>\n    </exception>\n", esc(&x));
        }
        out += "</passwords>\n";
        Ok(out)
    }

    /// Firefox / Chrome compatible CSV (`url,username,password`).
    pub fn export_csv(&self, db: &Connection) -> anyhow::Result<String> {
        let mut w = csv::Writer::from_writer(vec![]);
        w.write_record(["url", "username", "password", "httpRealm", "formActionOrigin", "guid", "timeCreated", "timeLastUsed", "timePasswordChanged"])?;
        for e in self.all(db)? {
            let url = if e.host.contains("://") { e.host.clone() } else { format!("https://{}", e.host) };
            w.write_record([url.as_str(), &e.username, &e.password, "", &url, "", "", &e.last_used.to_string(), ""])?;
        }
        Ok(String::from_utf8(w.into_inner()?)?)
    }

    /// Import Falkon XML or a Firefox/Chrome/Bitwarden-style CSV. Entries
    /// with an existing host+username are updated. Returns (added, updated).
    pub fn import(&mut self, db: &Connection, text: &str) -> anyhow::Result<(usize, usize)> {
        let mut entries: Vec<Entry> = vec![];
        let trimmed = text.trim_start_matches('\u{feff}').trim_start();
        if trimmed.starts_with('<') {
            let unesc = |s: &str| s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
            let field = |block: &str, tag: &str| -> String {
                let re = regex::Regex::new(&format!(r"(?s)<{tag}>(.*?)</{tag}>")).unwrap();
                re.captures(block).map(|c| unesc(&c[1])).unwrap_or_default()
            };
            let entry_re = regex::Regex::new(r"(?s)<entry>(.*?)</entry>").unwrap();
            for c in entry_re.captures_iter(trimmed) {
                let b = &c[1];
                entries.push(Entry { host: field(b, "server"), username: field(b, "username"), password: field(b, "password"), data: field(b, "data"), ..Default::default() });
            }
            let exc_re = regex::Regex::new(r"(?s)<exception>(.*?)</exception>").unwrap();
            for c in exc_re.captures_iter(trimmed) {
                Self::add_exception(db, &field(&c[1], "server"))?;
            }
        } else {
            let mut r = csv::ReaderBuilder::new().flexible(true).from_reader(trimmed.as_bytes());
            let headers: Vec<String> = r.headers()?.iter().map(|h| h.trim().to_lowercase()).collect();
            let col = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
            let (Some(u), Some(n), Some(p)) = (col(&["url", "origin", "login_uri", "website", "hostname"]), col(&["username", "login_username", "login"]), col(&["password", "login_password"])) else {
                anyhow::bail!("unrecognised CSV: need url, username and password columns");
            };
            for rec in r.records() {
                let rec = rec?;
                let url = rec.get(u).unwrap_or("").trim();
                let password = rec.get(p).unwrap_or("");
                if url.is_empty() || password.is_empty() {
                    continue;
                }
                entries.push(Entry { host: create_host(url), username: rec.get(n).unwrap_or("").into(), password: password.into(), ..Default::default() });
            }
        }
        let (mut added, mut updated) = (0, 0);
        for e in entries.into_iter().filter(|e| !e.host.is_empty() && !e.password.is_empty()) {
            let existing = self.query(db, Some(&e.host))?.into_iter().find(|x| x.username == e.username);
            match existing {
                Some(x) if x.password == e.password => {}
                Some(x) => {
                    self.update(db, &Entry { id: x.id, data: if e.data.is_empty() { x.data.clone() } else { e.data.clone() }, ..e })?;
                    updated += 1;
                }
                None => {
                    self.add(db, &e)?;
                    added += 1;
                }
            }
        }
        Ok((added, updated))
    }
}

// ---- password generator -------------------------------------------------------

const GEN_LOWER: &str = "abcdefghijkmnopqrstuvwxyz";
const GEN_UPPER: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const GEN_DIGITS: &str = "23456789";
const GEN_SYMBOLS: &str = "-_.!@#$%&*+=?";

/// A uniformly random index below `n` (rejection sampling, no modulo bias).
fn random_below(n: usize) -> usize {
    debug_assert!(n > 0 && n <= 256);
    let limit = 256 - 256 % n;
    loop {
        let b = random::<1>()[0] as usize;
        if b < limit {
            return b % n;
        }
    }
}

/// A random password of `len` characters (8..=128) from letters and digits,
/// plus symbols when `symbols`; look-alike characters (l, I, O, 0, 1) are
/// left out. Every class used appears at least once.
pub fn generate_password(len: usize, symbols: bool) -> String {
    let len = len.clamp(8, 128);
    let mut classes: Vec<&[u8]> = vec![GEN_LOWER.as_bytes(), GEN_UPPER.as_bytes(), GEN_DIGITS.as_bytes()];
    if symbols {
        classes.push(GEN_SYMBOLS.as_bytes());
    }
    let all: Vec<u8> = classes.concat();
    let mut out: Vec<u8> = classes.iter().map(|c| c[random_below(c.len())]).collect();
    while out.len() < len {
        out.push(all[random_below(all.len())]);
    }
    // Shuffle so the guaranteed characters are not always first (Fisher-Yates).
    for i in (1..out.len()).rev() {
        out.swap(i, random_below(i + 1));
    }
    String::from_utf8(out).expect("ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        crate::db::open_in_memory().unwrap()
    }

    #[test]
    fn hosts() {
        assert_eq!(create_host("https://example.com/login?x"), "example.com");
        assert_eq!(create_host("http://localhost:8080/"), "localhost:8080");
    }

    #[test]
    fn plain_flow_and_falkon_compat() {
        let db = db();
        let mut s = PasswordStore::open(&db).unwrap();
        // An entry written by Falkon:
        db.execute("INSERT INTO autofill (server, data, password, username, last_used) VALUES ('kde.org', 'u=ann&p=pw', 'pw', 'ann', 1)", []).unwrap();
        assert_eq!(s.entries_for(&db, "https://kde.org/login").unwrap()[0].username, "ann");
        assert_eq!(s.decide(&db, "https://kde.org/", "ann", "pw", "").unwrap(), Decision::Nothing);
        let d = s.decide(&db, "https://kde.org/", "ann", "new", "").unwrap();
        assert!(matches!(d, Decision::Update(ref e) if e.password == "new"));
        s.apply(&db, &d).unwrap();
        assert_eq!(s.entries_for(&db, "https://kde.org/").unwrap()[0].password, "new");
        let d = s.decide(&db, "https://kde.org/", "bob", "x", "").unwrap();
        assert!(matches!(d, Decision::Save(_)));
        s.apply(&db, &d).unwrap();
        assert_eq!(s.all(&db).unwrap().len(), 2);
        PasswordStore::add_exception(&db, "https://evil.test/a").unwrap();
        assert_eq!(s.decide(&db, "https://evil.test/b", "a", "b", "").unwrap(), Decision::Nothing);
    }

    #[test]
    fn generated_passwords() {
        for (len, sym) in [(8, false), (20, true), (64, true)] {
            let p = generate_password(len, sym);
            assert_eq!(p.len(), len);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()) && p.chars().any(|c| c.is_ascii_uppercase()) && p.chars().any(|c| c.is_ascii_digit()));
            assert_eq!(p.chars().any(|c| GEN_SYMBOLS.contains(c)), sym);
            assert!(!p.contains(['l', 'I', 'O', '0', '1']));
        }
        assert_eq!(generate_password(3, true).len(), 8);
        assert_ne!(generate_password(20, true), generate_password(20, true));
    }

    #[test]
    fn encryption_roundtrip() {
        let mut db = db();
        let mut s = PasswordStore::open(&db).unwrap();
        s.add(&db, &Entry { host: "kde.org".into(), username: "ann".into(), password: "secret-pw".into(), ..Default::default() }).unwrap();
        s.set_master_password(&mut db, None, "master").unwrap();
        assert_eq!(s.backend(&db), Backend::Encrypted);
        // nothing readable left in plain form
        let plain: i64 = db.query_row("SELECT count(*) FROM autofill", [], |r| r.get(0)).unwrap();
        assert_eq!(plain, 0);
        let raw: String = db.query_row("SELECT password_enc FROM kite_passwords_encrypted", [], |r| r.get(0)).unwrap();
        assert!(!raw.contains("secret"));
        assert_eq!(s.all(&db).unwrap()[0].password, "secret-pw");
        s.lock();
        assert!(s.is_locked(&db));
        assert!(s.all(&db).is_err());
        assert!(s.unlock(&db, "wrong").is_err());
        s.unlock(&db, "master").unwrap();
        assert_eq!(s.entries_for(&db, "https://kde.org/").unwrap()[0].username, "ann");
        s.set_master_password(&mut db, Some("master"), "master2").unwrap();
        s.lock();
        assert!(s.unlock(&db, "master").is_err());
        s.unlock(&db, "master2").unwrap();
        s.remove_master_password(&mut db, "master2").unwrap();
        assert_eq!(s.backend(&db), Backend::Plain);
        assert_eq!(s.all(&db).unwrap()[0].password, "secret-pw");
    }

    #[test]
    fn switch_backends() {
        let mut db = db();
        let mut s = PasswordStore::open(&db).unwrap();
        s.add(&db, &Entry { host: "a.test".into(), username: "u".into(), password: "p".into(), ..Default::default() }).unwrap();
        assert!(s.set_backend(&mut db, Backend::KWallet, None).is_err()); // wallet not open
        assert_eq!(s.backend(&db), Backend::Plain);
        s.set_backend(&mut db, Backend::Encrypted, Some("master")).unwrap();
        assert_eq!(s.backend(&db), Backend::Encrypted);
        assert_eq!(s.all(&db).unwrap()[0].password, "p");
        s.set_backend(&mut db, Backend::Plain, None).unwrap();
        assert_eq!(s.backend(&db), Backend::Plain);
        assert_eq!(s.all(&db).unwrap().len(), 1);
    }

    #[test]
    fn import_export() {
        let db = db();
        let mut s = PasswordStore::open(&db).unwrap();
        let csv = "\u{feff}\"url\",\"username\",\"password\",\"httpRealm\"\nhttps://a.test/login,ann,p1,\nhttp://b.test:8080,bob,\"p,2\",\n";
        assert_eq!(s.import(&db, csv).unwrap(), (2, 0));
        assert_eq!(s.import(&db, csv).unwrap(), (0, 0));
        assert_eq!(s.entries_for(&db, "http://b.test:8080/").unwrap()[0].password, "p,2");
        PasswordStore::add_exception(&db, "never.test").unwrap();
        let xml = s.export_falkon_xml(&db).unwrap();
        let db2 = crate::db::open_in_memory().unwrap();
        let mut s2 = PasswordStore::open(&db2).unwrap();
        assert_eq!(s2.import(&db2, &xml).unwrap(), (2, 0));
        assert_eq!(PasswordStore::exceptions(&db2).unwrap(), vec!["never.test"]);
        let out = s2.export_csv(&db2).unwrap();
        assert!(out.contains("https://a.test,ann,p1"));
    }
}
