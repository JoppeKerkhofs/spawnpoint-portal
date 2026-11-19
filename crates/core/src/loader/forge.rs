use std::path::PathBuf;

use reqwest::Client;
use serde::Deserialize;

use crate::download::{fetch, get_ok, Download};
use crate::loader::LoaderKind;
use crate::paths::check_relative;
use crate::{Error, Paths, Result};

const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const FORGE_PROMOTIONS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

#[derive(Deserialize)]
struct Versions {
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct Promotions {
    promos: std::collections::BTreeMap<String, String>,
}

pub fn newest_neoforge(versions_json: &str, minecraft: &str) -> Result<String> {
    let mut parts: Vec<&str> = minecraft.split('.').collect();
    // 1.x versions drop the leading 1; year-based ones (26.1) keep it and gain a 0 patch.
    if parts[0] == "1" {
        parts.remove(0);
    }
    let want = if minecraft.starts_with("1.") { 2 } else { 3 };
    while parts.len() < want {
        parts.push("0");
    }
    let prefix = format!("{}.", parts.join("."));
    let all: Vec<String> = serde_json::from_str::<Versions>(versions_json)?
        .versions
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .collect();
    all.iter()
        .rev()
        .find(|v| !v.contains("-beta"))
        .or(all.last())
        .cloned()
        .ok_or_else(|| Error::Other(format!("NeoForge has no build for Minecraft {minecraft}")))
}

pub fn newest_forge(promotions_json: &str, minecraft: &str) -> Result<String> {
    let promos = serde_json::from_str::<Promotions>(promotions_json)?.promos;
    ["recommended", "latest"]
        .iter()
        .find_map(|k| promos.get(&format!("{minecraft}-{k}")))
        .cloned()
        .ok_or_else(|| Error::Other(format!("Forge has no build for Minecraft {minecraft}")))
}

pub fn installer_url(kind: LoaderKind, minecraft: &str, version: &str) -> String {
    match kind {
        LoaderKind::Forge => format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{minecraft}-{version}/forge-{minecraft}-{version}-installer.jar"
        ),
        _ => format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
        ),
    }
}

pub async fn installer(
    client: &Client,
    paths: &Paths,
    kind: LoaderKind,
    minecraft: &str,
    version: Option<&str>,
) -> Result<(String, PathBuf)> {
    let name = match kind {
        LoaderKind::NeoForge => "NeoForge",
        LoaderKind::Forge => "Forge",
        LoaderKind::Fabric => return Err(Error::Other("Fabric has no installer".into())),
    };
    check_relative(minecraft)?;
    let version = match version {
        Some(v) => v.to_string(),
        None if kind == LoaderKind::Forge => {
            newest_forge(&get_ok(client, FORGE_PROMOTIONS).await?.text().await?, minecraft)?
        }
        None => newest_neoforge(&get_ok(client, NEOFORGE_VERSIONS).await?.text().await?, minecraft)?,
    };
    check_relative(&version)?;
    let path = paths.installers().join(format!("{}-{minecraft}-{version}.jar", kind.as_str()));
    let item = Download { url: installer_url(kind, minecraft, &version), path: path.clone(), hash: None, size: None };
    fetch(client, &item).await.map_err(|e| match e {
        Error::Status { status: 404, .. } => {
            Error::Other(format!("{name} {version} does not exist for Minecraft {minecraft}"))
        }
        e => e,
    })?;
    Ok((version, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_versions() {
        let j = include_str!("../../tests/fixtures/neoforge-versions.json");
        assert_eq!(newest_neoforge(j, "1.21.1").unwrap(), "21.1.267");
        assert!(newest_neoforge(j, "1.16.5").is_err());
        assert!(newest_neoforge(j, "1.20.1").is_err());
        assert_eq!(newest_neoforge(j, "1.21").unwrap(), "21.0.167");
        assert_eq!(newest_neoforge(j, "1.21.2").unwrap(), "21.2.1-beta");
        assert_eq!(newest_neoforge(j, "26.3").unwrap(), "26.3.0.113");
        assert!(newest_neoforge(j, "26.4").is_err());
    }

    #[test]
    fn forge_versions() {
        let j = include_str!("../../tests/fixtures/forge-promotions.json");
        assert_eq!(newest_forge(j, "1.20.1").unwrap(), "47.4.28");
        assert_eq!(newest_forge(j, "1.16.5").unwrap(), "36.2.34");
        assert!(newest_forge(j, "0.0.1").is_err());
    }

    #[test]
    fn installer_urls() {
        assert_eq!(installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.90"), "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.90/neoforge-21.1.90-installer.jar");
        assert_eq!(installer_url(LoaderKind::Forge, "1.20.1", "47.3.0"), "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar");
    }
}
