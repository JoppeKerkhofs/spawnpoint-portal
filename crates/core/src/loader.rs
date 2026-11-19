use serde::{Deserialize, Serialize};

use crate::download::ProgressTx;
use crate::paths::check_id;
use crate::version::VersionJson;
use crate::{Error, Paths, Result};
use reqwest::Client;
use std::path::Path;

pub mod fabric;
pub mod forge;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
    NeoForge,
    Forge,
}

impl LoaderKind {
    pub fn parse(name: &str) -> Result<LoaderKind> {
        match name.to_ascii_lowercase().as_str() {
            "fabric" => Ok(LoaderKind::Fabric),
            "neoforge" => Ok(LoaderKind::NeoForge),
            "forge" => Ok(LoaderKind::Forge),
            "quilt" => Err(Error::Unsupported("the Quilt loader is not supported".into())),
            _ => Err(Error::Other(format!("unknown loader {name}"))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LoaderKind::Fabric => "fabric",
            LoaderKind::NeoForge => "neoforge",
            LoaderKind::Forge => "forge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loader {
    pub kind: LoaderKind,
    pub version: Option<String>,
}

impl Loader {
    pub fn parse(text: &str) -> Result<Loader> {
        match text.split_once(':') {
            None => Ok(Loader { kind: LoaderKind::parse(text)?, version: None }),
            Some((_, "")) => Err(Error::Other(format!("loader {text:?} has an empty version"))),
            Some((name, version)) => {
                Ok(Loader { kind: LoaderKind::parse(name)?, version: Some(version.into()) })
            }
        }
    }
}

pub async fn resolve(client: &Client, minecraft: &str, loader: &Loader) -> Result<Loader> {
    check_id(minecraft)?;
    let version = match &loader.version {
        Some(v) => v.clone(),
        None if loader.kind == LoaderKind::Fabric => fabric::newest_version(client, minecraft).await?,
        None => forge::newest_version(client, loader.kind, minecraft).await?,
    };
    check_id(&version)?;
    Ok(Loader { kind: loader.kind, version: Some(version) })
}

pub async fn profile(
    client: &Client,
    paths: &Paths,
    minecraft: &str,
    kind: LoaderKind,
    version: &str,
    java: &Path,
    progress: &ProgressTx,
) -> Result<VersionJson> {
    match kind {
        LoaderKind::Fabric => fabric::profile(client, paths, minecraft, version).await,
        kind => forge::install(client, paths, kind, minecraft, version, java, progress).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_refuses_odd_versions_before_any_request() {
        let c = crate::http_client();
        let given = |v: &str| Loader { kind: LoaderKind::NeoForge, version: Some(v.into()) };
        assert!(resolve(&c, "1.21.1", &given("1?x")).await.is_err());
        assert!(resolve(&c, "1.21.1", &given("21.1.1#a")).await.is_err());
        assert!(resolve(&c, "1%2e", &given("21.1.1")).await.is_err());
        assert!(resolve(&c, "1.21.1", &given("..")).await.is_err());
    }

    #[test]
    fn loader_names() {
        assert_eq!(Loader::parse("Fabric:0.16.5").unwrap().version.as_deref(), Some("0.16.5"));
        assert_eq!(Loader::parse("fabric").unwrap().version, None);
        assert!(Loader::parse("fabric:").is_err());
        assert!(matches!(LoaderKind::parse("NeoForge").unwrap(), LoaderKind::NeoForge));
        assert!(matches!(LoaderKind::parse("quilt"), Err(Error::Unsupported(_))));
        assert!(LoaderKind::parse("rift").is_err());
    }
}
