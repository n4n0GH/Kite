//! KWallet client over D-Bus (kwalletd6, falling back to kwalletd5).
//!
//! Uses the same layout as Falkon's KDE Frameworks Integration plugin so both
//! browsers share saved logins: network wallet, folder `FalkonPasswords`,
//! one map entry per login keyed by a UUID with the fields
//! `host`, `username`, `password`, `updated`, `data`.

use std::collections::{BTreeMap, HashMap};
use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

pub const FOLDER: &str = "FalkonPasswords";
const APP_ID: &str = "Kite";
const SERVICES: [(&str, &str); 2] = [("org.kde.kwalletd6", "/modules/kwalletd6"), ("org.kde.kwalletd5", "/modules/kwalletd5")];

// ---- QDataStream encoding of QMap<QString, QString> -------------------------

fn put_qstring(out: &mut Vec<u8>, s: &str) {
    let utf16: Vec<u16> = s.encode_utf16().collect();
    out.extend(((utf16.len() * 2) as u32).to_be_bytes());
    for u in utf16 {
        out.extend(u.to_be_bytes());
    }
}

fn get_qstring(b: &[u8], pos: &mut usize) -> anyhow::Result<String> {
    let len = u32::from_be_bytes(b.get(*pos..*pos + 4).ok_or_else(|| anyhow::anyhow!("truncated"))?.try_into()?);
    *pos += 4;
    if len == 0xFFFF_FFFF {
        return Ok(String::new()); // null QString
    }
    let bytes = b.get(*pos..*pos + len as usize).ok_or_else(|| anyhow::anyhow!("truncated"))?;
    *pos += len as usize;
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    Ok(String::from_utf16_lossy(&units))
}

pub fn encode_map(map: &BTreeMap<String, String>) -> Vec<u8> {
    let mut out = (map.len() as u32).to_be_bytes().to_vec();
    for (k, v) in map {
        put_qstring(&mut out, k);
        put_qstring(&mut out, v);
    }
    out
}

pub fn decode_map(b: &[u8]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut pos = 0;
    let n = u32::from_be_bytes(b.get(0..4).ok_or_else(|| anyhow::anyhow!("truncated"))?.try_into()?);
    pos += 4;
    let mut m = BTreeMap::new();
    for _ in 0..n {
        let k = get_qstring(b, &mut pos)?;
        let v = get_qstring(b, &mut pos)?;
        m.insert(k, v);
    }
    Ok(m)
}

// ---- client -----------------------------------------------------------------

pub struct KWallet {
    conn: Connection,
    service: &'static str,
    path: &'static str,
    handle: i32,
}

impl KWallet {
    /// Is a KWallet daemon available and enabled?
    pub fn available() -> bool {
        let Ok(conn) = Connection::session() else { return false };
        SERVICES.iter().any(|(s, p)| {
            conn.call_method(Some(*s), *p, Some("org.kde.KWallet"), "isEnabled", &())
                .ok()
                .and_then(|m| m.body().deserialize::<bool>().ok())
                .unwrap_or(false)
        })
    }

    /// Open the network wallet. May block while KWallet asks for the wallet
    /// password, so call it off the UI thread.
    pub fn open(window_id: i64) -> anyhow::Result<Self> {
        let conn = Connection::session()?;
        let mut last_err = anyhow::anyhow!("KWallet is not available");
        for (service, path) in SERVICES {
            let wallet: String = match conn.call_method(Some(service), path, Some("org.kde.KWallet"), "networkWallet", &()) {
                Ok(m) => m.body().deserialize()?,
                Err(e) => {
                    last_err = e.into();
                    continue;
                }
            };
            let handle: i32 = conn
                .call_method(Some(service), path, Some("org.kde.KWallet"), "open", &(wallet.as_str(), window_id, APP_ID))?
                .body()
                .deserialize()?;
            if handle < 0 {
                anyhow::bail!("the wallet could not be opened (access denied or cancelled)");
            }
            let w = KWallet { conn, service, path, handle };
            if !w.call::<bool>("hasFolder", &(handle, FOLDER, APP_ID))? {
                w.call::<bool>("createFolder", &(handle, FOLDER, APP_ID))?;
            }
            return Ok(w);
        }
        Err(last_err)
    }

    fn call<R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type>(&self, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> anyhow::Result<R> {
        let m = self.conn.call_method(Some(self.service), self.path, Some("org.kde.KWallet"), method, body)?;
        Ok(m.body().deserialize()?)
    }

    /// All entries of the folder: key -> field map.
    pub fn entries(&self) -> anyhow::Result<Vec<(String, BTreeMap<String, String>)>> {
        let list: HashMap<String, OwnedValue> = self.call("mapList", &(self.handle, FOLDER, APP_ID))?;
        let mut out = vec![];
        for (key, v) in list {
            let bytes: Vec<u8> = match Vec::<u8>::try_from(v) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if let Ok(map) = decode_map(&bytes) {
                out.push((key, map));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    pub fn write(&self, key: &str, map: &BTreeMap<String, String>) -> anyhow::Result<()> {
        let rc: i32 = self.call("writeMap", &(self.handle, FOLDER, key, encode_map(map), APP_ID))?;
        if rc != 0 {
            anyhow::bail!("KWallet refused to store the entry ({rc})");
        }
        Ok(())
    }

    pub fn remove(&self, key: &str) -> anyhow::Result<()> {
        let _: i32 = self.call("removeEntry", &(self.handle, FOLDER, key, APP_ID))?;
        Ok(())
    }
}

impl Drop for KWallet {
    fn drop(&mut self) {
        let _ = self.call::<i32>("close", &(self.handle, false, APP_ID));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qdatastream_map_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert("host".to_string(), "kde.org".to_string());
        m.insert("password".to_string(), "pä$$ 🔑".to_string());
        m.insert("data".to_string(), String::new());
        let b = encode_map(&m);
        // QDataStream: count, then (len-prefixed UTF-16BE) pairs
        assert_eq!(&b[..4], &[0, 0, 0, 3]);
        assert_eq!(decode_map(&b).unwrap(), m);
        // null QString values decode as empty
        let mut raw = vec![0, 0, 0, 1];
        put_qstring(&mut raw, "k");
        raw.extend([0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(decode_map(&raw).unwrap()["k"], "");
    }
}
