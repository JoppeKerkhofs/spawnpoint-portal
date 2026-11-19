use crate::download::{fetch, fetch_all, Download, Hash, ProgressTx};
use crate::version::{library_files, VersionJson};
use crate::paths::check_relative;
use crate::{Error, Paths, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<ManifestVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: BTreeMap<String, AssetObject>,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub async fn manifest(client: &Client) -> Result<Manifest> {
    let response = client.get(MANIFEST_URL).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status { url: MANIFEST_URL.into(), status: status.as_u16() });
    }
    Ok(serde_json::from_slice(&response.bytes().await?)?)
}

pub async fn version_json(client: &Client, paths: &Paths, id: &str) -> Result<VersionJson> {
    check_relative(id)?;
    let file = paths.versions().join(id).join(format!("{id}.json"));
    if !file.exists() {
        let manifest = manifest(client).await?;
        let entry = manifest
            .versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| Error::Other(format!("Minecraft version {id} does not exist")))?;
        let item = Download {
            url: entry.url.clone(),
            path: file.clone(),
            hash: Some(Hash::Sha1(entry.sha1.clone())),
            size: None,
        };
        fetch(client, &item).await?;
    }
    Ok(serde_json::from_slice(&tokio::fs::read(&file).await?)?)
}

pub fn asset_downloads(index_json: &str, assets_dir: &Path) -> Result<Vec<Download>> {
    let index: AssetIndex = serde_json::from_str(index_json)?;
    let mut seen = HashSet::new();
    let mut downloads = Vec::new();
    for object in index.objects.values().filter(|o| seen.insert(o.hash.clone())) {
        let hash = &object.hash;
        if hash.len() != 40 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(Error::Other(format!("asset hash {hash:?} is not 40 lower-case hex characters")));
        }
        let prefix = &hash[..2];
        downloads.push(Download {
            url: format!("https://resources.download.minecraft.net/{prefix}/{}", object.hash),
            path: assets_dir.join("objects").join(prefix).join(&object.hash),
            hash: Some(Hash::Sha1(object.hash.clone())),
            size: Some(object.size),
        });
    }
    Ok(downloads)
}

fn once_per_path(items: Vec<Download>) -> Vec<Download> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|d| seen.insert(d.path.clone())).collect()
}

pub async fn install(client: &Client, paths: &Paths, version: &VersionJson, progress: &ProgressTx) -> Result<()> {
    let missing = |what: &str| Error::Other(format!("version {} has no {what}", version.id));
    let client_jar = &version.downloads.as_ref().ok_or_else(|| missing("client download"))?.client;
    let mut files = vec![Download {
        url: client_jar.url.clone(),
        path: crate::launch::jar_path(paths, version),
        hash: client_jar.sha1.clone().map(Hash::Sha1),
        size: client_jar.size,
    }];
    for lib in library_files(version)? {
        files.push(Download {
            url: lib.url,
            path: paths.libraries().join(PathBuf::from(lib.path)),
            hash: lib.sha1.map(Hash::Sha1),
            size: lib.size,
        });
    }
    fetch_all(client, "libraries", once_per_path(files), progress).await?;

    let assets = paths.assets();
    let index_ref = version.asset_index.as_ref().ok_or_else(|| missing("asset index"))?;
    check_relative(&index_ref.id)?;
    let index_file = assets.join("indexes").join(format!("{}.json", index_ref.id));
    fetch(
        client,
        &Download {
            url: index_ref.url.clone(),
            path: index_file.clone(),
            hash: Some(Hash::Sha1(index_ref.sha1.clone())),
            size: None,
        },
    )
    .await?;
    let mut files = asset_downloads(&tokio::fs::read_to_string(&index_file).await?, &assets)?;
    if let Some(log) = version.logging.as_ref().and_then(|l| l.client.as_ref()) {
        check_relative(&log.file.id)?;
        files.push(Download {
            url: log.file.url.clone(),
            path: assets.join("log_configs").join(&log.file.id),
            hash: Some(Hash::Sha1(log.file.sha1.clone())),
            size: Some(log.file.size),
        });
    }
    fetch_all(client, "assets", once_per_path(files), progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_the_manifest() {
        let m: Manifest = serde_json::from_str(include_str!("../tests/fixtures/version_manifest_v2.json")).unwrap();
        assert!(m.versions.iter().any(|v| v.id == "1.21.1" && v.kind == "release"));
    }

    #[test]
    fn assets_become_downloads() {
        let d = asset_downloads(include_str!("../tests/fixtures/asset-index.json"), Path::new("A")).unwrap();
        assert_eq!(d.len(), 3);
        let h = "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356";
        assert_eq!(d[0].url, format!("https://resources.download.minecraft.net/{}/{}", &h[..2], h));
        assert_eq!(d[0].path, Path::new("A").join("objects").join(&h[..2]).join(h));
    }

    #[test]
    fn the_same_hash_is_downloaded_once() {
        let h = "aa".repeat(20);
        let index = format!(r#"{{"objects":{{"a":{{"hash":"{h}","size":1}},"b":{{"hash":"{h}","size":1}}}}}}"#);
        assert_eq!(asset_downloads(&index, Path::new("A")).unwrap().len(), 1);
    }

    #[test]
    fn bad_asset_hashes_are_refused() {
        for hash in ["a", "aabbcc", &"AA".repeat(20), &"zz".repeat(20), "\u{e9}\u{e9}", &"aa".repeat(21)] {
            let index = format!(r#"{{"objects":{{"a":{{"hash":"{hash}","size":1}}}}}}"#);
            assert!(asset_downloads(&index, Path::new("A")).is_err(), "{hash}");
        }
    }
}
