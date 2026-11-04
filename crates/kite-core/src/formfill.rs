//! Form autofill: saved addresses and contact details that pages' forms can
//! be filled with ("browsing.formFill").
//!
//! Entries live in `browsedata.db` (`kite_formfill`), each sealed whole with
//! XChaCha20-Poly1305 — nothing about them is stored in clear text. The key
//! is random and kept outside the database: in KDE Wallet (folder "Kite")
//! when it is available, otherwise in an owner-only file in the profile
//! (`formfill.key`). Which one is used is recorded in `kite_formfill_meta`,
//! with a sealed check value to recognise a lost or wrong key.

use crate::passwords::{random, Sealer};
use base64::Engine as _;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use zeroize::Zeroizing;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kite_formfill (id INTEGER PRIMARY KEY, sealed TEXT NOT NULL, last_used INTEGER NOT NULL DEFAULT 0, uses INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS kite_formfill_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

const WALLET_FOLDER: &str = "Kite";
const WALLET_ENTRY: &str = "formfill-key";
const KEY_FILE: &str = "formfill.key";
const CHECK: &str = "kite-formfill";

/// One saved address / set of contact details.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Address {
    #[serde(skip_deserializing)]
    pub id: i64,
    pub given_name: String,
    pub family_name: String,
    pub organization: String,
    /// Street address; lines separated by "\n".
    pub street: String,
    pub city: String,
    pub region: String,
    pub postal_code: String,
    pub country: String,
    pub email: String,
    pub tel: String,
}

/// The kinds of form fields Kite fills (named after the HTML autocomplete
/// tokens the page script recognises).
pub const FIELDS: [&str; 15] = [
    "name", "given-name", "family-name", "organization", "street-address", "address-line1", "address-line2", "address-line3",
    "address-level2", "address-level1", "postal-code", "country", "email", "tel", "tel-national",
];

impl Address {
    pub fn name(&self) -> String {
        [self.given_name.trim(), self.family_name.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ")
    }

    fn line(&self, n: usize) -> String {
        let lines: Vec<&str> = self.street.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if n == 2 && lines.len() > 2 {
            // Two-line forms: everything after the first line goes in line 2
            // unless the form also has line 3.
            return lines[1].to_owned();
        }
        lines.get(n - 1).map(|s| s.to_string()).unwrap_or_default()
    }

    /// The value for a field kind ("" when none).
    pub fn value(&self, field: &str) -> String {
        match field {
            "name" => self.name(),
            "given-name" => self.given_name.trim().into(),
            "family-name" => self.family_name.trim().into(),
            "organization" => self.organization.trim().into(),
            "street-address" => self.street.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n"),
            "address-line1" => self.line(1),
            "address-line2" => self.line(2),
            "address-line3" => self.line(3),
            "address-level2" => self.city.trim().into(),
            "address-level1" => self.region.trim().into(),
            "postal-code" => self.postal_code.trim().into(),
            "country" => self.country.trim().into(),
            "email" => self.email.trim().into(),
            "tel" => self.tel.trim().into(),
            "tel-national" => {
                let t = self.tel.trim();
                // Drop a leading "+<country code>" when the number has one.
                match t.strip_prefix('+') {
                    Some(rest) => rest.split_once([' ', '-']).map(|(_, n)| n.trim().to_owned()).unwrap_or_else(|| t.to_owned()),
                    None => t.to_owned(),
                }
            }
            _ => String::new(),
        }
    }

    /// All field values (for filling a whole form).
    pub fn values(&self) -> BTreeMap<&'static str, String> {
        FIELDS.iter().map(|f| (*f, self.value(f))).filter(|(_, v)| !v.is_empty()).collect()
    }

    /// Build an address from submitted form fields (kind → value).
    pub fn from_fields(fields: &BTreeMap<String, String>) -> Address {
        let get = |k: &str| fields.get(k).map(|v| v.trim().to_owned()).unwrap_or_default();
        let mut a = Address {
            given_name: get("given-name"),
            family_name: get("family-name"),
            organization: get("organization"),
            city: get("address-level2"),
            region: get("address-level1"),
            postal_code: get("postal-code"),
            country: get("country"),
            email: get("email"),
            tel: if get("tel").is_empty() { get("tel-national") } else { get("tel") },
            ..Default::default()
        };
        if a.given_name.is_empty() && a.family_name.is_empty() {
            let full = get("name");
            match full.rsplit_once(' ') {
                Some((g, f)) => {
                    a.given_name = g.trim().into();
                    a.family_name = f.trim().into();
                }
                None => a.given_name = full,
            }
        }
        a.street = get("street-address");
        if a.street.is_empty() {
            a.street = ["address-line1", "address-line2", "address-line3"].iter().map(|k| get(k)).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        }
        a
    }

    fn parts(&self) -> [&str; 10] {
        [&self.given_name, &self.family_name, &self.organization, &self.street, &self.city, &self.region, &self.postal_code, &self.country, &self.email, &self.tel]
    }

    pub fn is_empty(&self) -> bool {
        self.parts().iter().all(|p| p.trim().is_empty())
    }

    /// A short description: name and street (or email).
    pub fn label(&self) -> String {
        let name = self.name();
        let second = [self.line(1), self.city.trim().into(), self.email.trim().into(), self.tel.trim().into(), self.organization.trim().into()]
            .into_iter()
            .find(|s: &String| !s.is_empty())
            .unwrap_or_default();
        match (name.is_empty(), second.is_empty()) {
            (false, false) => format!("{name}, {second}"),
            (false, true) => name,
            _ => second,
        }
    }

    /// Does `self` already hold everything in `other`?
    fn covers(&self, other: &Address) -> bool {
        self.parts().iter().zip(other.parts()).all(|(a, b)| b.trim().is_empty() || norm(a) == norm(b))
    }

    /// Is `other` probably the same person/address, with changes?
    fn same_identity(&self, other: &Address) -> bool {
        let same = |a: &str, b: &str| !a.trim().is_empty() && norm(a) == norm(b);
        (same(&self.name(), &other.name()) && (same(&self.line(1), &other.line(1)) || same(&self.email, &other.email) || same(&self.tel, &other.tel)))
            || (same(&self.email, &other.email) && same(&self.line(1), &other.line(1)))
    }

    /// Fill the empty parts of `self` from `old`, keeping what was entered.
    fn merged_over(mut self, old: &Address) -> Address {
        let fields: [(&mut String, &String); 10] = [
            (&mut self.given_name, &old.given_name),
            (&mut self.family_name, &old.family_name),
            (&mut self.organization, &old.organization),
            (&mut self.street, &old.street),
            (&mut self.city, &old.city),
            (&mut self.region, &old.region),
            (&mut self.postal_code, &old.postal_code),
            (&mut self.country, &old.country),
            (&mut self.email, &old.email),
            (&mut self.tel, &old.tel),
        ];
        for (new, old) in fields {
            if new.trim().is_empty() {
                *new = old.clone();
            }
        }
        self.id = old.id;
        self
    }
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// What to do after a form with address fields was submitted.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Nothing,
    Save(Address),
    Update(Address),
}

/// A suggestion for a focused field.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    pub id: i64,
    /// The value for the focused field.
    pub label: String,
    /// What else the entry holds, to tell entries apart.
    pub detail: String,
}

/// Where the key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStore {
    KWallet,
    File,
}

impl KeyStore {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyStore::KWallet => "kwallet",
            KeyStore::File => "file",
        }
    }
    fn parse(s: &str) -> Option<KeyStore> {
        match s {
            "kwallet" => Some(KeyStore::KWallet),
            "file" => Some(KeyStore::File),
            _ => None,
        }
    }
}

fn b64(v: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(v)
}

fn key_from_b64(s: &str) -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let raw = Zeroizing::new(base64::engine::general_purpose::STANDARD.decode(s.trim())?);
    let mut key = Zeroizing::new([0u8; 32]);
    if raw.len() != 32 {
        anyhow::bail!("the key is damaged");
    }
    key.copy_from_slice(&raw);
    Ok(key)
}

fn read_key_file(dir: &Path) -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let text = Zeroizing::new(std::fs::read_to_string(dir.join(KEY_FILE)).map_err(|e| anyhow::anyhow!("the key file could not be read: {e}"))?);
    key_from_b64(&text)
}

fn write_key_file(dir: &Path, key: &[u8; 32]) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = dir.join(KEY_FILE);
    let tmp = dir.join(format!("{KEY_FILE}.tmp"));
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.write_all(Zeroizing::new(b64(key)).as_bytes())?;
    f.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn wallet_key(create: bool) -> anyhow::Result<Option<Zeroizing<[u8; 32]>>> {
    let kw = crate::kwallet::KWallet::open_folder(0, WALLET_FOLDER)?;
    if let Some((_, map)) = kw.entries()?.into_iter().find(|(k, _)| k == WALLET_ENTRY) {
        return map.get("key").map(|k| key_from_b64(k)).transpose();
    }
    if !create {
        return Ok(None);
    }
    let key = Zeroizing::new(random::<32>());
    let mut map = BTreeMap::new();
    map.insert("key".to_string(), b64(key.as_ref()));
    kw.write(WALLET_ENTRY, &map)?;
    Ok(Some(key))
}

/// Get the key (blocking: KDE Wallet may ask for its password, so call it
/// off the UI thread). `store` is the recorded key store (`meta_keystore`);
/// with none, a new key is made — in KDE Wallet when available.
pub fn obtain_key(profile_dir: &Path, store: Option<KeyStore>) -> anyhow::Result<(Zeroizing<[u8; 32]>, KeyStore)> {
    match store {
        Some(KeyStore::File) => Ok((read_key_file(profile_dir)?, KeyStore::File)),
        Some(KeyStore::KWallet) => match wallet_key(false)? {
            Some(k) => Ok((k, KeyStore::KWallet)),
            None => anyhow::bail!("the key for saved form data is missing from KDE Wallet"),
        },
        None => {
            if crate::kwallet::KWallet::available() {
                match wallet_key(true) {
                    Ok(Some(k)) => return Ok((k, KeyStore::KWallet)),
                    Ok(None) => {}
                    Err(e) => log::warn!("form data key not kept in KDE Wallet: {e}"),
                }
            }
            let key = Zeroizing::new(random::<32>());
            write_key_file(profile_dir, &key)?;
            Ok((key, KeyStore::File))
        }
    }
}

pub struct FormFillStore {
    key: Option<Zeroizing<[u8; 32]>>,
}

impl FormFillStore {
    pub fn open(db: &Connection) -> anyhow::Result<Self> {
        db.execute_batch(SCHEMA)?;
        Ok(FormFillStore { key: None })
    }

    fn meta(db: &Connection, key: &str) -> Option<String> {
        db.query_row("SELECT value FROM kite_formfill_meta WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }
    fn set_meta(db: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
        db.execute("INSERT OR REPLACE INTO kite_formfill_meta (key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
    }

    /// The key store recorded for this profile (none before the first key).
    pub fn meta_keystore(db: &Connection) -> Option<KeyStore> {
        Self::meta(db, "keystore").and_then(|s| KeyStore::parse(&s))
    }

    pub fn count(db: &Connection) -> usize {
        db.query_row("SELECT COUNT(*) FROM kite_formfill", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as usize
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    /// Take a key from `obtain_key`. A first key is recorded; a key that does
    /// not open the existing entries is refused.
    pub fn set_key(&mut self, db: &Connection, key: Zeroizing<[u8; 32]>, store: KeyStore) -> anyhow::Result<()> {
        let sealer = Sealer::new(&key);
        match Self::meta(db, "check") {
            Some(check) => {
                if sealer.open_str(&check).ok().as_deref() != Some(CHECK) {
                    anyhow::bail!("the key does not match the saved form data");
                }
            }
            None => {
                Self::set_meta(db, "check", &sealer.seal(CHECK.as_bytes()))?;
                Self::set_meta(db, "keystore", store.as_str())?;
            }
        }
        self.key = Some(key);
        Ok(())
    }

    /// Forget all entries and the key record (after a lost key; the next
    /// save makes a new key).
    pub fn reset(&mut self, db: &Connection) -> anyhow::Result<()> {
        db.execute_batch("DELETE FROM kite_formfill; DELETE FROM kite_formfill_meta;")?;
        self.key = None;
        Ok(())
    }

    fn sealer(&self) -> anyhow::Result<Sealer> {
        self.key.as_ref().map(|k| Sealer::new(k)).ok_or_else(|| anyhow::anyhow!("saved form data is locked"))
    }

    pub fn all(&self, db: &Connection) -> anyhow::Result<Vec<Address>> {
        let sealer = self.sealer()?;
        let mut stmt = db.prepare("SELECT id, sealed FROM kite_formfill ORDER BY uses DESC, last_used DESC, id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = vec![];
        for row in rows {
            let (id, sealed) = row?;
            match sealer.open(&sealed).ok().and_then(|plain| serde_json::from_slice::<Address>(&Zeroizing::new(plain)).ok()) {
                Some(mut a) => {
                    a.id = id;
                    out.push(a);
                }
                None => log::warn!("saved form entry {id} could not be read"),
            }
        }
        Ok(out)
    }

    pub fn get(&self, db: &Connection, id: i64) -> anyhow::Result<Option<Address>> {
        Ok(self.all(db)?.into_iter().find(|a| a.id == id))
    }

    fn sealed(&self, a: &Address) -> anyhow::Result<String> {
        let plain = Zeroizing::new(serde_json::to_vec(a)?);
        Ok(self.sealer()?.seal(&plain))
    }

    pub fn add(&self, db: &Connection, a: &Address) -> anyhow::Result<i64> {
        if a.is_empty() {
            anyhow::bail!("the address is empty");
        }
        db.execute("INSERT INTO kite_formfill (sealed, last_used) VALUES (?1, ?2)", params![self.sealed(a)?, crate::util::now_ms() / 1000])?;
        Ok(db.last_insert_rowid())
    }

    pub fn update(&self, db: &Connection, a: &Address) -> anyhow::Result<()> {
        if a.is_empty() {
            anyhow::bail!("the address is empty");
        }
        let n = db.execute("UPDATE kite_formfill SET sealed = ?1 WHERE id = ?2", params![self.sealed(a)?, a.id])?;
        if n == 0 {
            anyhow::bail!("entry not found");
        }
        Ok(())
    }

    pub fn touch(&self, db: &Connection, id: i64) -> anyhow::Result<()> {
        db.execute("UPDATE kite_formfill SET uses = uses + 1, last_used = ?1 WHERE id = ?2", params![crate::util::now_ms() / 1000, id])?;
        Ok(())
    }

    pub fn remove(&self, db: &Connection, id: i64) -> anyhow::Result<()> {
        db.execute("DELETE FROM kite_formfill WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn remove_all(&self, db: &Connection) -> anyhow::Result<()> {
        db.execute("DELETE FROM kite_formfill", [])?;
        Ok(())
    }

    /// Entries for a focused field of kind `field` whose value starts with
    /// what was typed (`prefix`), one per distinct value.
    pub fn suggestions(&self, db: &Connection, field: &str, prefix: &str) -> anyhow::Result<Vec<Suggestion>> {
        let prefix = norm(prefix);
        let mut seen = std::collections::HashSet::new();
        let mut out = vec![];
        for a in self.all(db)? {
            let value = a.value(field);
            let v = norm(&value);
            if v.is_empty() || !v.starts_with(&prefix) || !seen.insert(v) {
                continue;
            }
            let label = value.lines().next().unwrap_or_default().to_owned();
            let detail = a.label();
            out.push(Suggestion { id: a.id, detail: if norm(&detail) == norm(&label) { String::new() } else { detail }, label });
        }
        Ok(out)
    }

    /// Compare a submitted address with the saved ones. Too little to be
    /// worth keeping (fewer than three parts, none of them a street, email
    /// or phone number) is ignored.
    pub fn decide(&self, db: &Connection, new: &Address) -> anyhow::Result<Decision> {
        let filled = new.parts().iter().filter(|p| !p.trim().is_empty()).count();
        if filled < 3 || (new.street.trim().is_empty() && new.email.trim().is_empty() && new.tel.trim().is_empty()) {
            return Ok(Decision::Nothing);
        }
        // Nothing saved yet: no need for the key to know that.
        let saved = if Self::count(db) == 0 { vec![] } else { self.all(db)? };
        if let Some(a) = saved.iter().find(|a| a.covers(new)) {
            self.touch(db, a.id)?;
            return Ok(Decision::Nothing);
        }
        if let Some(a) = saved.iter().find(|a| a.same_identity(new)) {
            return Ok(Decision::Update(new.clone().merged_over(a)));
        }
        Ok(Decision::Save(new.clone()))
    }

    pub fn apply(&self, db: &Connection, d: &Decision) -> anyhow::Result<()> {
        match d {
            Decision::Nothing => Ok(()),
            Decision::Save(a) => self.add(db, a).map(|_| ()),
            Decision::Update(a) => self.update(db, a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn values_and_parsing() {
        let a = Address::from_fields(&fields(&[("name", "Ada  Mary Lovelace"), ("address-line1", "12 St James's Sq"), ("address-line2", "Flat 3"), ("address-level2", "London"), ("email", "ada@example.org"), ("tel", "+44 20 7946 0000")]));
        assert_eq!((a.given_name.as_str(), a.family_name.as_str()), ("Ada  Mary", "Lovelace"));
        assert_eq!(a.street, "12 St James's Sq\nFlat 3");
        assert_eq!(a.value("name"), "Ada  Mary Lovelace");
        assert_eq!(a.value("address-line2"), "Flat 3");
        assert_eq!(a.value("address-line3"), "");
        assert_eq!(a.value("street-address"), "12 St James's Sq\nFlat 3");
        assert_eq!(a.value("tel-national"), "20 7946 0000");
        assert_eq!(a.label(), "Ada  Mary Lovelace, 12 St James's Sq");
        assert!(!a.values().contains_key("country"));
    }

    #[test]
    fn sealed_store() {
        let db = Connection::open_in_memory().unwrap();
        let mut s = FormFillStore::open(&db).unwrap();
        assert!(s.all(&db).is_err());
        let key = Zeroizing::new(random::<32>());
        s.set_key(&db, key.clone(), KeyStore::File).unwrap();
        assert_eq!(FormFillStore::meta_keystore(&db), Some(KeyStore::File));
        let a = Address { given_name: "Grace".into(), family_name: "Hopper".into(), street: "1 Navy Way".into(), email: "grace@example.org".into(), ..Default::default() };
        let d = s.decide(&db, &a).unwrap();
        assert_eq!(d, Decision::Save(a.clone()));
        s.apply(&db, &d).unwrap();
        // Nothing readable in the database.
        let raw: String = db.query_row("SELECT sealed FROM kite_formfill", [], |r| r.get(0)).unwrap();
        assert!(!raw.contains("Grace") && !raw.contains("Navy"));
        // The same (or less) again: nothing to do.
        let less = Address { given_name: "grace".into(), email: "GRACE@example.org".into(), tel: String::new(), street: "1 Navy Way".into(), ..Default::default() };
        assert_eq!(s.decide(&db, &less).unwrap(), Decision::Nothing);
        // A changed phone number for the same person: an update, merged.
        let changed = Address { given_name: "Grace".into(), family_name: "Hopper".into(), street: "1 Navy Way".into(), tel: "555-0100".into(), ..Default::default() };
        match s.decide(&db, &changed).unwrap() {
            Decision::Update(u) => {
                assert_eq!(u.email, "grace@example.org");
                assert_eq!(u.tel, "555-0100");
                s.apply(&db, &Decision::Update(u)).unwrap();
            }
            d => panic!("{d:?}"),
        }
        assert_eq!(s.all(&db).unwrap().len(), 1);
        // Too little to save.
        assert_eq!(s.decide(&db, &Address { email: "x@y.z".into(), ..Default::default() }).unwrap(), Decision::Nothing);
        // Suggestions by prefix.
        let sug = s.suggestions(&db, "email", "gr").unwrap();
        assert_eq!(sug.len(), 1);
        assert_eq!(sug[0].label, "grace@example.org");
        assert!(s.suggestions(&db, "email", "zz").unwrap().is_empty());
        // Another key is refused; a fresh store with the right key reads it.
        let mut other = FormFillStore::open(&db).unwrap();
        assert!(other.set_key(&db, Zeroizing::new(random::<32>()), KeyStore::File).is_err());
        other.set_key(&db, key, KeyStore::File).unwrap();
        assert_eq!(other.all(&db).unwrap()[0].tel, "555-0100");
        other.reset(&db).unwrap();
        assert_eq!(FormFillStore::count(&db), 0);
        assert_eq!(FormFillStore::meta_keystore(&db), None);
    }

    #[test]
    fn key_file() {
        let dir = tempfile::tempdir().unwrap();
        let (k, store) = (Zeroizing::new(random::<32>()), KeyStore::File);
        write_key_file(dir.path(), &k).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dir.path().join(KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        let (k2, s2) = obtain_key(dir.path(), Some(store)).unwrap();
        assert_eq!((*k2, s2), (*k, KeyStore::File));
    }
}
