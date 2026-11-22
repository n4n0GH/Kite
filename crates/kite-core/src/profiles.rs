//! The profiles: listing, creating and deleting them, and which are open.
//!
//! `profiles.json` beside the profile directories holds the choices made in
//! the profile picker. A profile in use has its `profile.lock` file locked,
//! which the system releases when that Kite ends, however it ends.
//!
//! One profile is the default one: it opens when none is asked for, is always
//! there and can't be deleted. It's named "default" until it's renamed; its
//! name is then kept in `profiles.json`, and "default" stays a way to say it
//! (`resolve`), which no other profile can be named.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};

/// The default profile's name until it's given another, and always a way to say it.
pub const DEFAULT: &str = "default";
/// Directories of `--private` runs (paths: `private-<pid>`), not profiles.
const PRIVATE_PREFIX: &str = "private-";
const LOCK_FILE: &str = "profile.lock";
/// A profile's picture, in its directory: it's renamed and deleted with it.
const AVATAR_FILE: &str = "avatar.png";
/// The picker crops pictures to 256 pixels a side, which is a quarter of this at most.
pub const AVATAR_LIMIT: usize = 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProfileInfo {
    pub name: String,
    #[serde(rename = "inUse")]
    pub in_use: bool,
    /// A child's profile (`parental`).
    pub child: bool,
    /// The default profile.
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    /// The file of its picture, "" without one.
    pub avatar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Choices {
    /// Ask at startup, when there are several profiles.
    pub ask: bool,
    /// The one picked last, offered first.
    pub last: String,
    /// The default profile's name.
    pub default: String,
}

impl Default for Choices {
    fn default() -> Self {
        Self { ask: true, last: DEFAULT.into(), default: DEFAULT.into() }
    }
}

pub struct Profiles {
    base: PathBuf,
}

impl Profiles {
    /// `base`: what holds the `profiles` directory (`paths::base_dir`).
    pub fn at(base: &Path) -> Self {
        Self { base: base.to_owned() }
    }

    pub fn open() -> anyhow::Result<Self> {
        Ok(Self::at(&crate::paths::base_dir()?))
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.base.join("profiles").join(name)
    }

    /// The profile's logs and crash report (`crashlog::dir`), kept beside it.
    fn logs_dir(&self, name: &str) -> PathBuf {
        self.base.join("logs").join(name)
    }

    /// The default profile's name: "default", or what it was renamed to.
    pub fn default_name(&self) -> String {
        Some(self.choices().default).filter(|n| validate(n).is_ok()).unwrap_or_else(|| DEFAULT.into())
    }

    /// The profile meant by `name`: "default" is the default one, whatever it's named.
    pub fn resolve(&self, name: &str) -> String {
        if name == DEFAULT {
            self.default_name()
        } else {
            name.to_owned()
        }
    }

    /// Whether a profile can't be given this name (`own`: the name it has now, if it
    /// has one): another has it, or it's "default" and this isn't the default one.
    /// (Directory names may differ in case only on some file systems.)
    fn taken(&self, name: &str, own: Option<&str>) -> Option<String> {
        let default = self.default_name();
        if name.eq_ignore_ascii_case(DEFAULT) && own != Some(default.as_str()) && !default.eq_ignore_ascii_case(DEFAULT) {
            return Some(format!("“{name}” always means the default profile, which is “{default}”. Choose another name."));
        }
        let same = own.is_some_and(|own| own.eq_ignore_ascii_case(name));
        (!same && (self.dir(name).exists() || self.list().iter().any(|p| p.name.eq_ignore_ascii_case(name)))).then(|| format!("A profile named “{name}” already exists."))
    }

    /// The default one always, then the others by name.
    pub fn list(&self) -> Vec<ProfileInfo> {
        let default = self.default_name();
        let mut names: Vec<String> = std::fs::read_dir(self.base.join("profiles"))
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| !n.starts_with(PRIVATE_PREFIX) && !n.starts_with('.') && *n != default)
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|n| n.to_lowercase());
        names.insert(0, default.clone());
        let children = crate::parental::Parental::at(&self.base).children();
        names
            .into_iter()
            .map(|name| {
                let avatar = self.avatar_path(&name);
                let avatar = if avatar.is_file() { avatar.to_string_lossy().into_owned() } else { String::new() };
                ProfileInfo { in_use: self.in_use(&name), child: children.contains_key(&name), is_default: name == default, avatar, name }
            })
            .collect()
    }

    fn avatar_path(&self, name: &str) -> PathBuf {
        self.dir(name).join(AVATAR_FILE)
    }

    /// The profile's picture: a PNG file's contents (the picker's, cropped and scaled), or
    /// None for no picture. An open profile's can be changed too: Kite itself doesn't hold it.
    pub fn set_avatar(&self, name: &str, png: Option<&[u8]>) -> anyhow::Result<()> {
        validate(name)?;
        let dir = self.dir(name);
        // (The default profile is there before it was ever opened.)
        if name != self.default_name() && !dir.is_dir() {
            anyhow::bail!("There is no profile named “{name}”.");
        }
        let path = self.avatar_path(name);
        let Some(png) = png else {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            };
        };
        if !png.starts_with(PNG_SIGNATURE) || png.len() > AVATAR_LIMIT {
            anyhow::bail!("That picture can't be used.");
        }
        crate::util::write_atomic(&path, png)?;
        Ok(())
    }

    pub fn in_use(&self, name: &str) -> bool {
        let Ok(file) = File::open(self.dir(name).join(LOCK_FILE)) else { return false };
        file.try_lock().is_err()
    }

    /// The profile open in Kite, if it's just the one: where a link should go,
    /// without asking.
    pub fn only_open(&self) -> Option<String> {
        let mut open = self.list().into_iter().filter(|p| p.in_use);
        let only = open.next()?;
        open.next().is_none().then_some(only.name)
    }

    pub fn create(&self, name: &str) -> anyhow::Result<()> {
        validate(name)?;
        if let Some(why) = self.taken(name, None) {
            anyhow::bail!(why);
        }
        std::fs::create_dir_all(self.dir(name))?;
        Ok(())
    }

    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        validate(name)?;
        if name == self.default_name() {
            anyhow::bail!("The default profile can't be deleted.");
        }
        if self.in_use(name) {
            anyhow::bail!("“{name}” is open in Kite. Close it first.");
        }
        let dir = self.dir(name);
        if !dir.is_dir() {
            anyhow::bail!("There is no profile named “{name}”.");
        }
        std::fs::remove_dir_all(dir)?;
        // Its logs and crash report go with it: they hold addresses it visited.
        if let Err(e) = std::fs::remove_dir_all(self.logs_dir(name)) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }
        // (A new profile of that name isn't a child's for having been one.)
        let parental = crate::parental::Parental::at(&self.base);
        if parental.is_child(name) {
            parental.set_child(name, None)?;
        }
        Ok(())
    }

    pub fn rename(&self, from: &str, to: &str) -> anyhow::Result<()> {
        validate(from)?;
        validate(to)?;
        if self.in_use(from) {
            anyhow::bail!("“{from}” is open in Kite. Close it first.");
        }
        // The default profile stays the default one under its new name. (It's there
        // before it was ever opened: then there's nothing to move yet.)
        let is_default = from == self.default_name();
        let from_dir = self.dir(from);
        if !from_dir.is_dir() && !is_default {
            anyhow::bail!("There is no profile named “{from}”.");
        }
        // A change of letter case only (“work” → “Work”) keeps the one profile;
        // any other name that's taken is refused, as when creating one.
        if let Some(why) = self.taken(to, Some(from)) {
            anyhow::bail!(why);
        }
        if from_dir.is_dir() {
            std::fs::rename(&from_dir, self.dir(to))?;
        }
        // A child's profile stays one under its new name.
        crate::parental::Parental::at(&self.base).renamed(from, to);
        // Its logs and crash report keep its name. (Any left under the new name
        // are of a profile deleted by an older Kite, which kept them.)
        if self.logs_dir(from).is_dir() {
            if !to.eq_ignore_ascii_case(from) {
                let _ = std::fs::remove_dir_all(self.logs_dir(to));
            }
            let _ = std::fs::rename(self.logs_dir(from), self.logs_dir(to));
        }
        // The one offered first follows its new name; so does which one is the default.
        let mut choices = self.choices();
        if choices.last == from || is_default {
            if choices.last == from {
                choices.last = to.to_owned();
            }
            if is_default {
                choices.default = to.to_owned();
                // (Without this the default profile would be an empty one named as before.)
                self.set_choices(&choices)?;
            } else {
                let _ = self.set_choices(&choices);
            }
        }
        Ok(())
    }

    pub fn choices(&self) -> Choices {
        std::fs::read_to_string(self.base.join("profiles.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn set_choices(&self, c: &Choices) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.base)?;
        // (The user's alone, as the profiles in it are: it has their names, and the plugins.)
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.base, std::fs::Permissions::from_mode(0o700));
        }
        crate::util::write_atomic(&self.base.join("profiles.json"), serde_json::to_string_pretty(c)?.as_bytes())?;
        Ok(())
    }

    /// Whether a start without `--profile` should show the picker.
    pub fn should_ask(&self) -> bool {
        self.choices().ask && self.list().len() > 1
    }
}

/// Marks the profile at `root` as in use for as long as the file is kept.
/// None if another process has it, or there is no telling.
pub fn lock(root: &Path) -> Option<File> {
    let file = File::options().create(true).truncate(false).write(true).open(root.join(LOCK_FILE)).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Names become directory names, and are passed around in menus and arguments.
pub fn validate(name: &str) -> anyhow::Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 40
        && !name.starts_with(['.', '-', ' '])
        && !name.ends_with(' ')
        && !name.starts_with(PRIVATE_PREFIX)
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
    if !ok {
        anyhow::bail!("Profile names can have letters, digits, spaces, “-”, “_” and “.”, and can't start with a dot or “private-”.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(p: &Profiles) -> Vec<String> {
        p.list().into_iter().map(|i| i.name).collect()
    }

    #[test]
    fn lists_default_first_without_private_runs() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        assert_eq!(names(&p), ["default"]);
        p.create("Work").unwrap();
        p.create("art").unwrap();
        std::fs::create_dir_all(dir.path().join("profiles/private-123")).unwrap();
        std::fs::create_dir_all(dir.path().join("profiles/default")).unwrap();
        assert_eq!(names(&p), ["default", "art", "Work"]);
    }

    #[test]
    fn asks_only_with_several_profiles_and_remembers() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        assert!(!p.should_ask());
        p.create("Work").unwrap();
        assert!(p.should_ask());
        p.set_choices(&Choices { ask: false, last: "Work".into(), ..p.choices() }).unwrap();
        assert!(!p.should_ask());
        assert_eq!(Profiles::at(dir.path()).choices().last, "Work");
    }

    #[test]
    fn knows_the_only_open_profile() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        p.create("Work").unwrap();
        p.create("Home").unwrap();
        assert_eq!(p.only_open(), None);
        let work = lock(&dir.path().join("profiles/Work")).unwrap();
        assert_eq!(p.only_open().as_deref(), Some("Work"));
        let home = lock(&dir.path().join("profiles/Home")).unwrap();
        assert_eq!(p.only_open(), None, "which one isn't clear");
        drop((work, home));
    }

    #[test]
    fn refuses_bad_names() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        for bad in ["", "../evil", "a/b", "..", ".hidden", "-p", "private-1", " x", "x ", "a\\b", "a\nb", &"x".repeat(41)] {
            assert!(p.create(bad).is_err(), "{bad:?}");
            assert!(p.delete(bad).is_err(), "{bad:?}");
        }
        assert!(p.create("My Profile_2.0").is_ok());
        assert!(p.create("My Profile_2.0").is_err());
        assert!(p.create("my profile_2.0").is_err());
        assert!(!dir.path().join("evil").exists());
    }

    #[test]
    fn renames_a_profile_with_its_data_and_the_last_choice() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        p.create("Work").unwrap();
        p.create("Home").unwrap();
        std::fs::write(dir.path().join("profiles/Work/bookmarks.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.path().join("logs/Work")).unwrap();
        std::fs::write(dir.path().join("logs/Work/last-crash.txt"), "boom").unwrap();
        // (Logs left behind under the new name by an older Kite's deletion.)
        std::fs::create_dir_all(dir.path().join("logs/Job")).unwrap();
        std::fs::write(dir.path().join("logs/Job/kite.log"), "old").unwrap();
        p.set_choices(&Choices { ask: true, last: "Work".into(), ..p.choices() }).unwrap();

        p.rename("Work", "Job").unwrap();
        assert_eq!(names(&p), ["default", "Home", "Job"]);
        assert!(dir.path().join("profiles/Job/bookmarks.json").exists(), "data moved with it");
        assert!(!dir.path().join("profiles/Work").exists());
        assert!(dir.path().join("logs/Job/last-crash.txt").exists(), "logs moved with it");
        assert!(!dir.path().join("logs/Job/kite.log").exists(), "stale logs under the new name are gone");
        assert!(!dir.path().join("logs/Work").exists());
        assert_eq!(p.choices().last, "Job", "the last choice follows");

        // Refusals: a name that's taken (the default profile's too), one that doesn't exist, a bad name.
        assert!(p.rename("Job", "default").is_err());
        assert!(p.rename("Job", "Home").is_err());
        assert!(p.rename("Job", "../evil").is_err());
        assert!(p.rename("missing", "x").is_err());
        // Open profiles can't be renamed.
        let held = lock(&dir.path().join("profiles/Job")).unwrap();
        assert!(p.rename("Job", "Gig").is_err());
        drop(held);
        // A change of letter case is allowed, logs included.
        p.rename("Job", "job").unwrap();
        assert!(names(&p).contains(&"job".to_owned()));
        assert!(dir.path().join("logs/job/last-crash.txt").exists());
    }

    #[test]
    fn the_default_profile_stays_the_default_one_under_another_name() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        p.create("Work").unwrap();
        std::fs::create_dir_all(dir.path().join("profiles/default")).unwrap();
        std::fs::write(dir.path().join("profiles/default/bookmarks.json"), "{}").unwrap();
        assert_eq!((p.default_name().as_str(), p.resolve("default").as_str(), p.resolve("Work").as_str()), ("default", "default", "Work"));
        assert!(p.list()[0].is_default && !p.list()[1].is_default);

        // Not to a name another has, and not while it's open.
        assert!(p.rename("default", "work").is_err());
        let held = lock(&dir.path().join("profiles/default")).unwrap();
        assert!(p.rename("default", "Anna").is_err());
        drop(held);

        p.rename("default", "Anna").unwrap();
        assert_eq!(names(&p), ["Anna", "Work"], "still first");
        assert!(p.list()[0].is_default);
        assert!(dir.path().join("profiles/Anna/bookmarks.json").exists() && !dir.path().join("profiles/default").exists());
        assert_eq!((p.default_name().as_str(), p.resolve("default").as_str()), ("Anna", "Anna"), "“default” still says it");
        assert_eq!(p.choices().last, "Anna", "it was the one offered first");
        assert!(p.delete("Anna").is_err(), "it can't be deleted under any name");
        assert!(Profiles::at(dir.path()).should_ask());

        // “default” is no other profile's name now.
        for name in ["default", "Default"] {
            assert!(p.create(name).is_err(), "{name}");
            assert!(p.rename("Work", name).is_err(), "{name}");
        }
        assert_eq!(names(&p), ["Anna", "Work"]);
        // Another name again, its letter case alone, and back to what it was.
        p.rename("Anna", "anna").unwrap();
        p.rename("anna", "default").unwrap();
        assert_eq!(names(&p), ["default", "Work"]);
        assert!(dir.path().join("profiles/default/bookmarks.json").exists());

        // One that was never opened has no directory yet: it takes a name all the same.
        let fresh = tempfile::tempdir().unwrap();
        let p = Profiles::at(fresh.path());
        p.rename("default", "Main").unwrap();
        assert_eq!(names(&p), ["Main"]);
        p.set_avatar("Main", Some(&[PNG_SIGNATURE, b"pixels"].concat())).unwrap();
        // A name in profiles.json that can't be a profile's isn't taken for it.
        p.set_choices(&Choices { default: "../evil".into(), ..p.choices() }).unwrap();
        assert_eq!(p.default_name(), "default");
    }

    #[test]
    fn a_picture_is_kept_with_its_profile() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        p.create("Work").unwrap();
        let avatar = |name: &str| p.list().into_iter().find(|i| i.name == name).unwrap().avatar;
        assert_eq!(avatar("Work"), "");
        let png = [PNG_SIGNATURE, b"pixels"].concat();
        p.set_avatar("Work", Some(&png)).unwrap();
        assert_eq!(std::fs::read(avatar("Work")).unwrap(), png);
        // The default profile takes one before it was ever opened.
        p.set_avatar("default", Some(&png)).unwrap();
        assert!(avatar("default").ends_with("profiles/default/avatar.png"));
        // An open one's can be changed.
        let held = lock(&dir.path().join("profiles/Work")).unwrap();
        p.set_avatar("Work", Some(&[PNG_SIGNATURE, b"others"].concat())).unwrap();
        drop(held);
        // It follows a new name, and goes with the profile.
        p.rename("Work", "Job").unwrap();
        assert!(avatar("Job").ends_with("profiles/Job/avatar.png"));
        p.set_avatar("Job", None).unwrap();
        assert_eq!(avatar("Job"), "");
        p.set_avatar("Job", None).unwrap();

        // Refusals: no such profile, a bad name, not a PNG file, too large a one.
        assert!(p.set_avatar("missing", Some(&png)).is_err());
        assert!(p.set_avatar("../evil", Some(&png)).is_err());
        assert!(p.set_avatar("Job", Some(b"<svg/>")).is_err());
        assert!(p.set_avatar("Job", Some(&[PNG_SIGNATURE, &vec![0; AVATAR_LIMIT][..]].concat())).is_err());
        assert_eq!(avatar("Job"), "");
        assert!(!dir.path().join("profiles/missing").exists() && !dir.path().join("evil").exists());
    }

    #[test]
    fn deleting_keeps_default_and_open_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::at(dir.path());
        assert!(p.delete("default").is_err());
        assert!(p.delete("missing").is_err());
        p.create("Work").unwrap();
        let root = dir.path().join("profiles/Work");
        std::fs::write(root.join("bookmarks.json"), "{}").unwrap();
        let logs = dir.path().join("logs/Work");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("last-crash.txt"), "boom").unwrap();

        let held = lock(&root).expect("lock");
        assert!(p.in_use("Work"));
        assert!(lock(&root).is_none(), "only one holder");
        assert!(p.delete("Work").is_err());
        assert!(root.join("bookmarks.json").exists());

        drop(held);
        assert!(!p.in_use("Work"));
        p.delete("Work").unwrap();
        assert!(!root.exists());
        assert!(!logs.exists(), "its logs and crash report go with it");
        // A child's profile that's deleted leaves nothing for a new one of its name.
        let parental = crate::parental::Parental::at(dir.path());
        parental.set_password("", "sesame").unwrap();
        p.create("Kid").unwrap();
        parental.set_child("Kid", Some(Default::default())).unwrap();
        assert!(p.list().iter().any(|i| i.name == "Kid" && i.child) && p.list().iter().any(|i| i.name == "default" && !i.child));
        p.rename("Kid", "Kiddo").unwrap();
        assert!(parental.is_child("Kiddo"), "held to the same under its new name");
        p.delete("Kiddo").unwrap();
        assert!(!parental.is_child("Kiddo"));
        // One that never ran has no logs: deleting it is fine all the same.
        p.create("Fresh").unwrap();
        p.delete("Fresh").unwrap();
    }
}
