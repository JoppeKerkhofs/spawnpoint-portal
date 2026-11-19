use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Paths> {
        let home = std::env::var_os("SPAWNPOINT_PORTAL_HOME").filter(|h| !h.is_empty());
        let root = if let Some(home) = home {
            PathBuf::from(home)
        } else if let Some(appdata) = std::env::var_os("APPDATA") {
            PathBuf::from(appdata).join("SpawnpointPortal")
        } else {
            return Err(Error::Other(
                "neither SPAWNPOINT_PORTAL_HOME nor APPDATA is set".into(),
            ));
        };
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
        unsafe { std::env::set_var("SPAWNPOINT_PORTAL_HOME", r"C:\tmp\portal home") };
        let p = Paths::new().unwrap();
        assert_eq!(p.versions(), PathBuf::from(r"C:\tmp\portal home\meta\versions"));
        assert_eq!(p.natives("1.16.5"), PathBuf::from(r"C:\tmp\portal home\meta\natives\1.16.5"));
        assert_eq!(p.panels_file(), PathBuf::from(r"C:\tmp\portal home\panels.json"));

        unsafe { std::env::set_var("SPAWNPOINT_PORTAL_HOME", r"relative\home") };
        let root = Paths::new().unwrap().root;
        assert!(root.is_absolute() && root.ends_with(r"relative\home"), "{root:?}");
        unsafe { std::env::set_var("SPAWNPOINT_PORTAL_HOME", "") };
        let root = Paths::new().unwrap().root;
        assert!(root.is_absolute() && root.ends_with("SpawnpointPortal"), "{root:?}");
    }

    #[test]
    fn check_relative_accepts_plain_paths_only() {
        assert!(check_relative("bin/javaw.exe").is_ok());
        for bad in ["", "../x", "/x", r"C:\x", "C:x", r"a\b", "."] {
            assert!(check_relative(bad).is_err(), "{bad}");
        }
    }
}
