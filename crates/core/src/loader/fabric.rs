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

pub fn newest(loaders_json: &str) -> Result<String> {
    let entries: Vec<Entry> = serde_json::from_str(loaders_json)?;
    entries
        .into_iter()
        .next()
        .map(|e| e.loader.version)
        .ok_or_else(|| Error::Other("Fabric has no loader for this Minecraft version".into()))
}

pub async fn profile(client: &Client, paths: &Paths, minecraft: &str, version: Option<&str>) -> Result<VersionJson> {
    check_relative(minecraft)?;
    let missing = || Error::Other(format!("Fabric {} does not exist for Minecraft {minecraft}", version.unwrap_or("loader")));
    let version = match version {
        Some(v) => v.to_string(),
        None => {
            let list = get_ok(client, &format!("{META}/{minecraft}")).await.map_err(|e| match e {
                Error::Status { status: 400 | 404, .. } => missing(),
                e => e,
            })?;
            newest(&list.text().await?)?
        }
    };
    check_relative(&version)?;
    let id = format!("fabric-loader-{version}-{minecraft}");
    let file = paths.versions().join(&id).join(format!("{id}.json"));
    let item = Download {
        url: format!("{META}/{minecraft}/{version}/profile/json"),
        path: file.clone(),
        hash: None,
        size: None,
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
    use crate::loader::{Loader, LoaderKind};

    #[test]
    fn loader_names() {
        assert_eq!(Loader::parse("Fabric:0.16.5").unwrap().version.as_deref(), Some("0.16.5"));
        assert_eq!(Loader::parse("fabric").unwrap().version, None);
        assert!(Loader::parse("fabric:").is_err());
        assert!(matches!(LoaderKind::parse("NeoForge").unwrap(), LoaderKind::NeoForge));
        assert!(matches!(LoaderKind::parse("quilt"), Err(Error::Unsupported(_))));
        assert!(LoaderKind::parse("rift").is_err());
    }

    #[test]
    fn newest_fabric() {
        assert_eq!(newest(include_str!("../../tests/fixtures/fabric-loaders-1.21.1.json")).unwrap(), "0.20.1");
        assert!(newest("[]").is_err());
    }
}
