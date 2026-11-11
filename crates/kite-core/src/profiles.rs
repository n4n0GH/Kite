//! The profiles: listing, creating and deleting them, and which are open.
//!
//! `profiles.json` beside the profile directories holds the choices made in
//! the profile picker. A profile in use has its `profile.lock` file locked,
//! which the system releases when that Kite ends, however it ends.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};

pub const DEFAULT: &str = "default";
/// Directories of `--private` runs (paths: `private-<pid>`), not profiles.
const PRIVATE_PREFIX: &str = "private-";
const LOCK_FILE: &str = "profile.lock";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProfileInfo {
    pub name: String,
    #[serde(rename = "inUse")]
    pub in_use: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Choices {
    /// Ask at startup, when there are several profiles.
    pub ask: bool,
    /// The one picked last, offered first.
    pub last: String,
}

impl Default for Choices {
    fn default() -> Self {
        Self { ask: true, last: DEFAULT.into() }
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

    /// The default one always, then the others by name.
    pub fn list(&self) -> Vec<ProfileInfo> {
        let mut names: Vec<String> = std::fs::read_dir(self.base.join("profiles"))
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| !n.starts_with(PRIVATE_PREFIX) && !n.starts_with('.') && n != DEFAULT)
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|n| n.to_lowercase());
        names.insert(0, DEFAULT.into());
        names.into_iter().map(|name| ProfileInfo { in_use: self.in_use(&name), name }).collect()
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
        let dir = self.dir(name);
        // (Directory names may differ in case only on some file systems.)
        if dir.exists() || self.list().iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            anyhow::bail!("A profile named “{name}” already exists.");
        }
        std::fs::create_dir_all(dir)?;
        Ok(())
    }

    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        validate(name)?;
        if name == DEFAULT {
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
        Ok(())
    }

    pub fn rename(&self, from: &str, to: &str) -> anyhow::Result<()> {
        validate(to)?;
        if from == DEFAULT {
            anyhow::bail!("The default profile can't be renamed.");
        }
        if self.in_use(from) {
            anyhow::bail!("“{from}” is open in Kite. Close it first.");
        }
        let from_dir = self.dir(from);
        if !from_dir.is_dir() {
            anyhow::bail!("There is no profile named “{from}”.");
        }
        // A change of letter case only (“work” → “Work”) keeps the one profile;
        // any other name that's taken is refused, as when creating one.
        if !to.eq_ignore_ascii_case(from) && (self.dir(to).exists() || self.list().iter().any(|p| p.name.eq_ignore_ascii_case(to))) {
            anyhow::bail!("A profile named “{to}” already exists.");
        }
        std::fs::rename(&from_dir, self.dir(to))?;
        // Its logs and crash report keep its name. (Any left under the new name
        // are of a profile deleted by an older Kite, which kept them.)
        if self.logs_dir(from).is_dir() {
            if !to.eq_ignore_ascii_case(from) {
                let _ = std::fs::remove_dir_all(self.logs_dir(to));
            }
            let _ = std::fs::rename(self.logs_dir(from), self.logs_dir(to));
        }
        // The one offered first follows its new name.
        if self.choices().last == from {
            let _ = self.set_choices(&Choices { last: to.to_owned(), ..self.choices() });
        }
        Ok(())
    }

    pub fn choices(&self) -> Choices {
        std::fs::read_to_string(self.base.join("profiles.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn set_choices(&self, c: &Choices) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.base)?;
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
        p.set_choices(&Choices { ask: false, last: "Work".into() }).unwrap();
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
        p.set_choices(&Choices { ask: true, last: "Work".into() }).unwrap();

        p.rename("Work", "Job").unwrap();
        assert_eq!(names(&p), ["default", "Home", "Job"]);
        assert!(dir.path().join("profiles/Job/bookmarks.json").exists(), "data moved with it");
        assert!(!dir.path().join("profiles/Work").exists());
        assert!(dir.path().join("logs/Job/last-crash.txt").exists(), "logs moved with it");
        assert!(!dir.path().join("logs/Job/kite.log").exists(), "stale logs under the new name are gone");
        assert!(!dir.path().join("logs/Work").exists());
        assert_eq!(p.choices().last, "Job", "the last choice follows");

        // Refusals: the default, a name that's taken, one that doesn't exist, a bad name.
        assert!(p.rename("default", "Main").is_err());
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
        // One that never ran has no logs: deleting it is fine all the same.
        p.create("Fresh").unwrap();
        p.delete("Fresh").unwrap();
    }
}
