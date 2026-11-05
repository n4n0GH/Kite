//! Installed extensions: on-disk layout, install/update/uninstall.
//!
//! ```text
//! extensions/
//!     registry.json            [{id, uuid, enabled, version, source, ...}]
//!     storage.db               browser.storage data (see storage.rs)
//!     <sanitized id>/          unpacked XPI
//! ```

use super::i18n::Messages;
use super::manifest::Manifest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstallInfo {
    pub id: String,
    /// Host part of moz-extension:// URLs; random per install like Firefox.
    pub uuid: String,
    pub enabled: bool,
    pub version: String,
    /// "amo" | "file" | "temporary"
    pub source: String,
    #[serde(default)]
    pub amo_slug: String,
    #[serde(default)]
    pub installed_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    /// Unpacked directory. For temporary add-ons this is the user's folder.
    pub path: PathBuf,
    /// Optional permissions the user granted at runtime.
    #[serde(default)]
    pub granted_optional: Vec<String>,
    #[serde(default)]
    pub allow_private: bool,
}

#[derive(Debug, Clone)]
pub struct Extension {
    pub info: InstallInfo,
    pub manifest: Manifest,
    pub messages: Messages,
}

impl Extension {
    pub fn load(info: InstallInfo) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(info.path.join("manifest.json"))?;
        let pre = Manifest::parse(&text, None)?;
        let messages = Messages::load(&info.path, pre.default_locale.as_deref());
        let manifest = Manifest::parse(&text, Some(&messages))?;
        Ok(Extension { info, manifest, messages })
    }
    pub fn id(&self) -> &str {
        &self.info.id
    }
    /// The manifest rules out private windows (`"incognito": "not_allowed"`).
    pub fn private_not_allowed(&self) -> bool {
        self.manifest.raw.get("incognito").and_then(|v| v.as_str()) == Some("not_allowed")
    }
    /// Whether it runs in private windows: only if the user allowed it.
    pub fn runs_in_private(&self) -> bool {
        self.info.allow_private && !self.private_not_allowed()
    }
    pub fn base_url(&self) -> String {
        format!("{}://{}/", super::SCHEME, self.info.uuid)
    }
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), super::manifest::norm_path(path))
    }
    /// Resolve a request path inside the extension, refusing traversal.
    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        let rel = percent_encoding::percent_decode_str(rel.trim_start_matches('/')).decode_utf8().ok()?;
        let rel = rel.split(['?', '#']).next().unwrap_or("");
        let mut p = self.info.path.clone();
        for comp in rel.split('/') {
            match comp {
                "" | "." => {}
                ".." => return None,
                c => p.push(c),
            }
        }
        Some(p)
    }
    pub fn icon_url(&self, size: u32) -> Option<String> {
        Manifest::icon_for(&self.manifest.icons, size).map(|p| self.url(&p))
    }
    /// Effective API permissions (manifest + granted optional ones).
    pub fn has_permission(&self, p: &str) -> bool {
        self.manifest.has_permission(p) || self.info.granted_optional.iter().any(|g| g == p)
    }
    pub fn host_patterns(&self) -> Vec<String> {
        let mut v = self.manifest.host_permissions.clone();
        v.extend(self.info.granted_optional.iter().filter(|p| super::matchpattern::is_host_pattern(p)).cloned());
        v
    }
}

pub struct Registry {
    dir: PathBuf,
    pub extensions: Vec<Extension>,
    /// Extensions that failed to load: (info, error).
    pub broken: Vec<(InstallInfo, String)>,
}

pub fn sanitize_id(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || "@.-_".contains(c) { c } else { '_' }).collect()
}

/// Firefox requires an id; unsigned extensions without one get a stable,
/// name-derived id so updates of a locally installed file replace it.
pub fn fallback_id(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{}@kite.local", if slug.is_empty() { "extension".into() } else { slug })
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub id: String,
    pub name: String,
    pub version: String,
    pub updated: bool,
}

impl Registry {
    pub fn open(dir: PathBuf) -> Self {
        let infos: Vec<InstallInfo> = std::fs::read_to_string(dir.join("registry.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let mut r = Registry { dir, extensions: vec![], broken: vec![] };
        for info in infos {
            // Temporary add-ons only live for one session, as in Firefox.
            if info.source == "temporary" {
                continue;
            }
            match Extension::load(info.clone()) {
                Ok(e) => r.extensions.push(e),
                Err(err) => r.broken.push((info, err.to_string())),
            }
        }
        r
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let infos: Vec<&InstallInfo> =
            self.extensions.iter().map(|e| &e.info).chain(self.broken.iter().map(|(i, _)| i)).collect();
        crate::util::write_atomic(&self.dir.join("registry.json"), serde_json::to_string_pretty(&infos)?.as_bytes())?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Extension> {
        self.extensions.iter().find(|e| e.info.id == id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Extension> {
        self.extensions.iter_mut().find(|e| e.info.id == id)
    }
    pub fn by_uuid(&self, uuid: &str) -> Option<&Extension> {
        self.extensions.iter().find(|e| e.info.uuid.eq_ignore_ascii_case(uuid))
    }
    pub fn enabled(&self) -> impl Iterator<Item = &Extension> {
        self.extensions.iter().filter(|e| e.info.enabled)
    }

    /// Validate an XPI and return its manifest without installing.
    pub fn inspect(bytes: &[u8]) -> anyhow::Result<Manifest> {
        let text = super::xpi::read_manifest(bytes)?;
        Manifest::parse(&text, None)
    }

    /// Install or update from XPI bytes.
    pub fn install_xpi(&mut self, bytes: &[u8], source: &str, amo_slug: &str) -> anyhow::Result<InstallResult> {
        let pre = Self::inspect(bytes)?;
        let id = pre.gecko_id.clone().unwrap_or_else(|| fallback_id(&pre.name));
        let staging = self.dir.join(format!(".staging-{}", uuid::Uuid::new_v4()));
        super::xpi::extract(bytes, &staging)?;
        let dest = self.dir.join(sanitize_id(&id));
        let now = crate::util::now_ms();

        let existing = self.extensions.iter().position(|e| e.info.id == id);
        let mut info = match existing {
            Some(i) => {
                let old = self.extensions.remove(i);
                InstallInfo { updated_at: now, ..old.info }
            }
            None => {
                self.broken.retain(|(i, _)| i.id != id);
                InstallInfo {
                    id: id.clone(),
                    uuid: uuid::Uuid::new_v4().to_string(),
                    enabled: true,
                    version: String::new(),
                    source: source.into(),
                    amo_slug: amo_slug.into(),
                    installed_at: now,
                    updated_at: now,
                    path: dest.clone(),
                    granted_optional: vec![],
                    allow_private: false,
                }
            }
        };
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        std::fs::rename(&staging, &dest)?;
        info.path = dest;
        info.version = pre.version.clone();
        if !amo_slug.is_empty() {
            info.amo_slug = amo_slug.into();
            info.source = source.into();
        }
        let ext = Extension::load(info)?;
        let result = InstallResult { id: id.clone(), name: ext.manifest.name.clone(), version: ext.manifest.version.clone(), updated: existing.is_some() };
        self.extensions.push(ext);
        self.save()?;
        Ok(result)
    }

    /// Load an unpacked extension from a folder for this session only
    /// (Firefox's "Load Temporary Add-on").
    pub fn load_temporary(&mut self, dir: &Path) -> anyhow::Result<InstallResult> {
        let text = std::fs::read_to_string(dir.join("manifest.json"))
            .map_err(|_| anyhow::anyhow!("{} contains no manifest.json", dir.display()))?;
        let pre = Manifest::parse(&text, None)?;
        let id = pre.gecko_id.clone().unwrap_or_else(|| fallback_id(&pre.name));
        let uuid = self.get(&id).map(|e| e.info.uuid.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.extensions.retain(|e| e.info.id != id);
        let now = crate::util::now_ms();
        let ext = Extension::load(InstallInfo {
            id: id.clone(),
            uuid,
            enabled: true,
            version: pre.version.clone(),
            source: "temporary".into(),
            amo_slug: String::new(),
            installed_at: now,
            updated_at: now,
            path: dir.to_path_buf(),
            granted_optional: vec![],
            allow_private: false,
        })?;
        let r = InstallResult { id, name: ext.manifest.name.clone(), version: ext.manifest.version.clone(), updated: false };
        self.extensions.push(ext);
        Ok(r)
    }

    pub fn uninstall(&mut self, id: &str) -> anyhow::Result<()> {
        if let Some(i) = self.extensions.iter().position(|e| e.info.id == id) {
            let e = self.extensions.remove(i);
            if e.info.source != "temporary" && e.info.path.starts_with(&self.dir) {
                let _ = std::fs::remove_dir_all(&e.info.path);
            }
        } else if let Some(i) = self.broken.iter().position(|(e, _)| e.id == id) {
            let (info, _) = self.broken.remove(i);
            if info.path.starts_with(&self.dir) {
                let _ = std::fs::remove_dir_all(&info.path);
            }
        } else {
            anyhow::bail!("extension {id} is not installed");
        }
        self.save()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> anyhow::Result<()> {
        let e = self.get_mut(id).ok_or_else(|| anyhow::anyhow!("extension {id} is not installed"))?;
        e.info.enabled = enabled;
        self.save()
    }

    pub fn set_allow_private(&mut self, id: &str, allow: bool) -> anyhow::Result<()> {
        let e = self.get_mut(id).ok_or_else(|| anyhow::anyhow!("extension {id} is not installed"))?;
        e.info.allow_private = allow;
        self.save()
    }

    pub fn grant_optional(&mut self, id: &str, perms: &[String], grant: bool) -> anyhow::Result<()> {
        let e = self.get_mut(id).ok_or_else(|| anyhow::anyhow!("extension {id} is not installed"))?;
        for p in perms {
            e.info.granted_optional.retain(|g| g != p);
            if grant {
                e.info.granted_optional.push(p.clone());
            }
        }
        self.save()
    }
}

/// Simple dotted version comparison (Firefox's toolkit version format is
/// richer, but AMO versions are almost always plain dotted numbers).
pub fn version_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<(u64, String)> {
        v.split('.')
            .map(|p| {
                let n: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                (n.parse().unwrap_or(0), p[n.len()..].to_owned())
            })
            .collect()
    };
    let (a, b) = (parse(candidate), parse(current));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).cloned().unwrap_or((0, String::new()));
        let y = b.get(i).cloned().unwrap_or((0, String::new()));
        if x.0 != y.0 {
            return x.0 > y.0;
        }
        if x.1 != y.1 {
            // "1.0" > "1.0b1": empty suffix is a release, which sorts later.
            return match (x.1.is_empty(), y.1.is_empty()) {
                (true, false) => true,
                (false, true) => false,
                _ => x.1 > y.1,
            };
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webext::xpi::make_xpi;
    #[test]
    fn install_update_uninstall() {
        let d = tempfile::tempdir().unwrap();
        let mut r = Registry::open(d.path().into());
        let v1 = make_xpi(&[
            ("manifest.json", r#"{"manifest_version":2,"name":"__MSG_n__","version":"1.0","default_locale":"en","browser_specific_settings":{"gecko":{"id":"t@x"}}}"#),
            ("_locales/en/messages.json", r#"{"n":{"message":"Test Ext"}}"#),
        ]);
        let res = r.install_xpi(&v1, "file", "").unwrap();
        assert_eq!((res.name.as_str(), res.updated), ("Test Ext", false));
        let uuid = r.get("t@x").unwrap().info.uuid.clone();
        r.set_enabled("t@x", false).unwrap();
        let v2 = make_xpi(&[("manifest.json", r#"{"manifest_version":2,"name":"T2","version":"2.0","browser_specific_settings":{"gecko":{"id":"t@x"}}}"#)]);
        let res = r.install_xpi(&v2, "file", "").unwrap();
        assert!(res.updated);
        let e = r.get("t@x").unwrap();
        assert_eq!((e.info.uuid.as_str(), e.info.enabled, e.info.version.as_str()), (uuid.as_str(), false, "2.0"));
        assert!(e.resolve("../../etc/passwd").is_none());
        assert!(e.resolve("manifest.json").unwrap().exists());
        let r2 = Registry::open(d.path().into());
        assert_eq!(r2.extensions.len(), 1);
        r.uninstall("t@x").unwrap();
        assert!(!d.path().join("t@x").exists());
        assert_eq!(Registry::open(d.path().into()).extensions.len(), 0);
    }
    #[test]
    fn versions() {
        assert!(version_newer("1.10", "1.9"));
        assert!(!version_newer("1.9", "1.9.0"));
        assert!(version_newer("1.0", "1.0b1"));
        assert!(!version_newer("1.0b1", "1.0"));
        assert_eq!(fallback_id("My Great Ext!"), "my-great-ext@kite.local");
    }
}
