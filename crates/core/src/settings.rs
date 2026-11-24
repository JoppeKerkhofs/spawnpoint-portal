use serde::{Deserialize, Serialize};

use crate::{Paths, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "on")]
    pub discord: bool,
    #[serde(default)]
    pub account_name: Option<String>,
}

fn on() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings { discord: true, account_name: None }
    }
}

pub fn load(paths: &Paths) -> Settings {
    std::fs::read(paths.settings_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(paths: &Paths, settings: &Settings) -> Result<()> {
    std::fs::create_dir_all(&paths.root)?;
    let tmp = paths.root.join("settings.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(tmp, paths.settings_file())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_settings_file_has_discord_on() {
        let dir = crate::download::testing::tmp_dir();
        let p = Paths { root: dir.join("home") };
        std::fs::create_dir_all(&p.root).unwrap();
        std::fs::write(p.settings_file(), br#"{"player_name":"JoppeK"}"#).unwrap();
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn settings_round_trip_and_default() {
        let dir = crate::download::testing::tmp_dir();
        let p = Paths { root: dir.join("home") };
        assert_eq!(load(&p), Settings::default());
        let new = Settings { discord: false, account_name: Some("JoppeK".into()) };
        save(&p, &new).unwrap();
        assert_eq!(load(&p), new);
        std::fs::write(p.settings_file(), b"garbage").unwrap();
        assert_eq!(load(&p), Settings::default());
    }
}
