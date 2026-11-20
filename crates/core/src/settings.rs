use serde::{Deserialize, Serialize};

use crate::{Error, Paths, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub player_name: String,
    #[serde(default = "on")]
    pub discord: bool,
}

fn on() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings { player_name: "Player".into(), discord: true }
    }
}

pub fn load(paths: &Paths) -> Settings {
    std::fs::read(paths.settings_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(paths: &Paths, settings: &Settings) -> Result<()> {
    if !valid_player_name(&settings.player_name) {
        return Err(Error::Other(format!("{:?} is not a valid player name", settings.player_name)));
    }
    std::fs::create_dir_all(&paths.root)?;
    let tmp = paths.root.join("settings.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(tmp, paths.settings_file())?;
    Ok(())
}

pub fn valid_player_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_names() {
        assert!(valid_player_name("Steve_01"));
        for bad in ["ab", "has space", "seventeen_chars_x", "naïve", ""] {
            assert!(!valid_player_name(bad), "{bad}");
        }
    }

    #[test]
    fn an_old_settings_file_has_discord_on() {
        let dir = crate::download::testing::tmp_dir();
        let p = Paths { root: dir.join("home") };
        std::fs::create_dir_all(&p.root).unwrap();
        std::fs::write(p.settings_file(), br#"{"player_name":"JoppeK"}"#).unwrap();
        let s = load(&p);
        assert_eq!((s.player_name.as_str(), s.discord), ("JoppeK", true));
    }

    #[test]
    fn settings_round_trip_and_default() {
        let dir = crate::download::testing::tmp_dir();
        let p = Paths { root: dir.join("home") };
        assert_eq!(load(&p).player_name, "Player");
        save(&p, &Settings { player_name: "JoppeK".into(), discord: false }).unwrap();
        assert_eq!((load(&p).player_name.as_str(), load(&p).discord), ("JoppeK", false));
        assert!(save(&p, &Settings { player_name: "x".into(), discord: true }).is_err());
        std::fs::write(p.settings_file(), b"garbage").unwrap();
        assert_eq!(load(&p).player_name, "Player");
    }
}
