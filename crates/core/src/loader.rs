use serde::{Deserialize, Serialize};

use crate::download::ProgressTx;
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

pub async fn profile(
    client: &Client,
    paths: &Paths,
    minecraft: &str,
    loader: &Loader,
    java: &Path,
    progress: &ProgressTx,
) -> Result<VersionJson> {
    let version = loader.version.as_deref();
    match loader.kind {
        LoaderKind::Fabric => fabric::profile(client, paths, minecraft, version).await,
        kind => forge::install(client, paths, kind, minecraft, version, java, progress).await,
    }
}
