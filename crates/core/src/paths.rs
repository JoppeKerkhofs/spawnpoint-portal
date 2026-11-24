use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::platform::{self, Platform};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Paths> {
        let root = root_from(platform::current(), |name| std::env::var_os(name))
            .ok_or_else(|| Error::Other("there is no folder for Portal's files: set SPAWNPOINT_PORTAL_HOME".into()))?;
        Ok(Paths { root: std::path::absolute(root)? })
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("meta").join("versions")
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("meta").join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("meta").join("assets")
    }

    pub fn natives(&self, version_id: &str) -> PathBuf {
        self.root.join("meta").join("natives").join(version_id)
    }

    pub fn installers(&self) -> PathBuf {
        self.root.join("meta").join("installers")
    }

    pub fn packs(&self) -> PathBuf {
        self.root.join("meta").join("packs")
    }

    pub fn java(&self) -> PathBuf {
        self.root.join("java")
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn panels_file(&self) -> PathBuf {
        self.root.join("panels.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
}

// SPAWNPOINT_PORTAL_HOME wins everywhere; else the system's own place for a program's files.
fn root_from(platform: &Platform, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(home) = var("SPAWNPOINT_PORTAL_HOME") {
        return Some(home);
    }
    if platform.os == "windows" {
        return Some(var("APPDATA")?.join("SpawnpointPortal"));
    }
    // The XDG rules ignore a data folder that is not a full path.
    let data = var("XDG_DATA_HOME").filter(|d| d.has_root()).or_else(|| Some(var("HOME")?.join(".local").join("share")))?;
    Some(data.join("spawnpoint-portal"))
}

// For anything from the caller that ends up in a URL or one of our own file names.
pub(crate) fn check_id(id: &str) -> Result<()> {
    if id.is_empty() || id.bytes().all(|b| b == b'.') || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        return Err(Error::Other(format!("{id:?} is not a valid pack id")));
    }
    Ok(())
}

pub fn check_relative(name: &str) -> Result<()> {
    let plain = Path::new(name).components().all(|c| matches!(c, Component::Normal(_)));
    if name.is_empty() || !plain || name.contains('\\') {
        return Err(Error::Other(format!("{name:?} is not a plain relative path")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_the_override() {
        // SAFETY: the only test that touches this variable.
        let home = Path::new(if cfg!(windows) { r"C:\tmp\portal home" } else { "/tmp/portal home" });
        unsafe { std::env::set_var("SPAWNPOINT_PORTAL_HOME", home) };
        let p = Paths::new().unwrap();
        assert_eq!(p.versions(), home.join("meta").join("versions"));
        assert_eq!(p.natives("1.16.5"), home.join("meta").join("natives").join("1.16.5"));
        assert_eq!(p.panels_file(), home.join("panels.json"));

        unsafe { std::env::set_var("SPAWNPOINT_PORTAL_HOME", r"relative\home") };
        let root = Paths::new().unwrap().root;
        assert!(root.is_absolute() && root.ends_with(r"relative\home"), "{root:?}");
    }

    #[test]
    fn each_system_has_its_own_folder_and_the_override_wins() {
        use crate::platform::{LINUX, WINDOWS};
        let root = |platform, vars: &[(&str, &str)]| root_from(platform, |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(v)));
        assert_eq!(root(&WINDOWS, &[("APPDATA", "A"), ("SPAWNPOINT_PORTAL_HOME", "")]), Some(Path::new("A").join("SpawnpointPortal")));
        assert_eq!(root(&LINUX, &[("HOME", "/home/a"), ("APPDATA", "A")]), Some(PathBuf::from("/home/a/.local/share/spawnpoint-portal")));
        assert_eq!(root(&LINUX, &[("HOME", "/home/a"), ("XDG_DATA_HOME", "/data")]), Some(PathBuf::from("/data/spawnpoint-portal")));
        assert_eq!(root(&LINUX, &[("HOME", "/home/a"), ("XDG_DATA_HOME", "relative")]), Some(PathBuf::from("/home/a/.local/share/spawnpoint-portal")));
        for platform in [&WINDOWS, &LINUX] {
            assert_eq!(root(platform, &[("HOME", "/home/a"), ("APPDATA", "A"), ("SPAWNPOINT_PORTAL_HOME", "own")]), Some(PathBuf::from("own")));
            assert_eq!(root(platform, &[]), None);
        }
    }

    #[test]
    fn check_relative_accepts_plain_paths_only() {
        assert!(check_relative("bin/javaw.exe").is_ok());
        for bad in ["", "../x", "/x", r"C:\x", r"a\b", "."] {
            assert!(check_relative(bad).is_err(), "{bad}");
        }
        // A drive prefix on Windows; anywhere else an ordinary file name.
        assert_eq!(check_relative("C:x").is_err(), cfg!(windows));
    }
}
