use reqwest::Client;
use serde::Deserialize;

use crate::download::{fetch, get_ok, Download};
use crate::paths::check_relative;
use crate::version::VersionJson;
use crate::{Error, Paths, Result};

const META: &str = "https://meta.fabricmc.net/v2/versions/loader";

#[derive(Deserialize)]
struct Entry {
    loader: LoaderVersion,
}

#[derive(Deserialize)]
struct LoaderVersion {
    version: String,
}

// Newest first, as Fabric lists them; never empty.
pub fn listed(loaders_json: &str, minecraft: &str) -> Result<Vec<String>> {
    let entries: Vec<Entry> = serde_json::from_str(loaders_json)?;
    match entries.is_empty() {
        true => Err(Error::Other(format!("Fabric has no loader for Minecraft {minecraft}"))),
        false => Ok(entries.into_iter().map(|e| e.loader.version).collect()),
    }
}

pub fn newest(loaders_json: &str, minecraft: &str) -> Result<String> {
    Ok(listed(loaders_json, minecraft)?.swap_remove(0))
}

pub async fn versions(client: &Client, minecraft: &str) -> Result<Vec<String>> {
    let list = get_ok(client, &format!("{META}/{minecraft}")).await.map_err(|e| match e {
        Error::Status { status: 400 | 404, .. } => Error::Other(format!("Fabric has no loader for Minecraft {minecraft}")),
        e => e,
    })?;
    listed(&list.text().await?, minecraft)
}

pub async fn newest_version(client: &Client, minecraft: &str) -> Result<String> {
    Ok(versions(client, minecraft).await?.swap_remove(0))
}

pub async fn profile(client: &Client, paths: &Paths, minecraft: &str, version: &str) -> Result<VersionJson> {
    check_relative(minecraft)?;
    let missing = || Error::Other(format!("Fabric {version} does not exist for Minecraft {minecraft}"));
    check_relative(version)?;
    let id = format!("fabric-loader-{version}-{minecraft}");
    let file = paths.versions().join(&id).join(format!("{id}.json"));
    let item = Download {
        url: format!("{META}/{minecraft}/{version}/profile/json"),
        path: file.clone(),
        hash: None,
        size: None, bearer: None,
    };
    fetch(client, &item).await.map_err(|e| match e {
        Error::Status { status: 400 | 404, .. } => missing(),
        e => e,
    })?;
    let profile: VersionJson = serde_json::from_slice(&tokio::fs::read(&file).await?)?;
    profile.check_ids()?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn newest_fabric() {
        assert_eq!(newest(include_str!("../../tests/fixtures/fabric-loaders-1.21.1.json"), "1.21.1").unwrap(), "0.20.1");
        assert_eq!(newest("[]", "1.2").unwrap_err().to_string(), "Fabric has no loader for Minecraft 1.2");
    }
}
