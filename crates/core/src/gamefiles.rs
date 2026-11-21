use crate::download::{fetch, fetch_all, get_ok, Download, Hash, ProgressTx};
use crate::version::{library_files, LibraryFile, VersionJson};
use crate::paths::check_relative;
use crate::sync::safe_path;
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
    #[serde(default)]
    r#virtual: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub async fn manifest(client: &Client) -> Result<Manifest> {
    Ok(serde_json::from_slice(&get_ok(client, MANIFEST_URL).await?.bytes().await?)?)
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Choice {
    pub id: String,
    pub snapshot: bool,
}

// Releases and snapshots, newest first; alpha and beta versions are not offered.
// A few old snapshots have a space in their id ("1.14 Pre-Release 1"): an instance cannot hold those.
pub fn supported_versions(manifest: &Manifest) -> Vec<Choice> {
    let offered = manifest.versions.iter().filter(|v| matches!(v.kind.as_str(), "release" | "snapshot") && crate::paths::check_id(&v.id).is_ok());
    offered.map(|v| Choice { id: v.id.clone(), snapshot: v.kind == "snapshot" }).collect()
}

pub fn find<'a>(manifest: &'a Manifest, id: &str) -> Result<&'a ManifestVersion> {
    manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| Error::Other(format!("Minecraft version {id} does not exist")))
}

pub async fn version_json(client: &Client, paths: &Paths, id: &str) -> Result<VersionJson> {
    check_relative(id)?;
    let file = paths.versions().join(id).join(format!("{id}.json"));
    if !file.exists() {
        let manifest = manifest(client).await?;
        let entry = find(&manifest, id)?;
        let item = Download {
            url: entry.url.clone(),
            path: file.clone(),
            hash: Some(Hash::Sha1(entry.sha1.clone())),
            size: None, bearer: None,
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
            size: Some(object.size), bearer: None,
        });
    }
    Ok(downloads)
}

pub fn virtual_copies(index_json: &str, assets_dir: &Path, index_id: &str, game_dir: Option<&Path>) -> Result<Vec<(PathBuf, PathBuf)>> {
    let index: AssetIndex = serde_json::from_str(index_json)?;
    let target = match (index.map_to_resources, game_dir) {
        (true, Some(dir)) => dir.join("resources"),
        (false, _) if index.r#virtual => assets_dir.join("virtual").join(index_id),
        _ => return Ok(Vec::new()),
    };
    index
        .objects
        .iter()
        .map(|(name, o)| {
            let prefix = o.hash.get(..2).ok_or_else(|| Error::Other(format!("asset hash {:?} is too short", o.hash)))?;
            Ok((assets_dir.join("objects").join(prefix).join(&o.hash), target.join(safe_path(name)?)))
        })
        .collect()
}

// Under the game folder a file the player already has is never replaced.
pub fn copy_missing(copies: &[(PathBuf, PathBuf)], game_dir: Option<&Path>) -> Result<()> {
    for (from, to) in copies {
        if let Ok(have) = std::fs::metadata(to) {
            if game_dir.is_some_and(|d| to.starts_with(d)) || have.len() == std::fs::metadata(from)?.len() {
                continue;
            }
        }
        std::fs::create_dir_all(to.parent().unwrap_or(to))?;
        let part = crate::download::part_path(to);
        std::fs::copy(from, &part)?;
        std::fs::rename(&part, to)?;
    }
    Ok(())
}

fn once_per_path(items: Vec<Download>) -> Vec<Download> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|d| seen.insert(d.path.clone())).collect()
}

// A library without a URL comes from a loader's installer and must already be on disk.
fn library_download(lib: LibraryFile, libraries: &Path, id: &str) -> Result<Option<Download>> {
    let path = libraries.join(PathBuf::from(&lib.path));
    if lib.url.is_empty() {
        return match path.exists() {
            true => Ok(None),
            false => Err(Error::Other(format!("{} is missing; the {id} install did not complete", lib.path))),
        };
    }
    Ok(Some(Download { url: lib.url, path, hash: lib.sha1.map(Hash::Sha1), size: lib.size, bearer: None }))
}

pub async fn install(client: &Client, paths: &Paths, version: &VersionJson, game_dir: Option<&Path>, progress: &ProgressTx) -> Result<()> {
    let missing = |what: &str| Error::Other(format!("version {} has no {what}", version.id));
    let client_jar = &version.downloads.as_ref().ok_or_else(|| missing("client download"))?.client;
    let mut files = vec![Download {
        url: client_jar.url.clone(),
        path: crate::launch::jar_path(paths, version),
        hash: client_jar.sha1.clone().map(Hash::Sha1),
        size: client_jar.size, bearer: None,
    }];
    for lib in library_files(version)? {
        files.extend(library_download(lib, &paths.libraries(), &version.id)?);
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
            size: None, bearer: None,
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
            size: Some(log.file.size), bearer: None,
        });
    }
    fetch_all(client, "assets", once_per_path(files), progress).await?;
    copy_missing(&virtual_copies(&tokio::fs::read_to_string(&index_file).await?, &assets, &index_ref.id, game_dir)?, game_dir)
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
    fn finds_a_version_in_the_manifest() {
        let m: Manifest = serde_json::from_str(include_str!("../tests/fixtures/version_manifest_v2.json")).unwrap();
        assert_eq!(find(&m, "1.21.1").unwrap().id, "1.21.1");
        let err = find(&m, "1.99").unwrap_err();
        assert_eq!(err.to_string(), "Minecraft version 1.99 does not exist");
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
    fn legacy_index_is_copied_out() {
        let c = virtual_copies(include_str!("../tests/fixtures/asset-index-legacy.json"), Path::new("A"), "legacy", None).unwrap();
        assert_eq!(c.len(), 3);
        assert!(c[0].1.starts_with(Path::new("A").join("virtual").join("legacy")));
        assert!(c[0].0.starts_with(Path::new("A").join("objects")));
    }

    #[test]
    fn a_modern_index_is_not() {
        assert!(virtual_copies(include_str!("../tests/fixtures/asset-index.json"), Path::new("A"), "17", None).unwrap().is_empty());
    }

    #[test]
    fn a_virtual_name_cannot_escape() {
        let h = "aa".repeat(20);
        let index = format!(r#"{{"virtual":true,"objects":{{"../x":{{"hash":"{h}","size":1}}}}}}"#);
        assert!(virtual_copies(&index, Path::new("A"), "legacy", None).is_err());
    }

    #[test]
    fn the_same_hash_is_downloaded_once() {
        let h = "aa".repeat(20);
        let index = format!(r#"{{"objects":{{"a":{{"hash":"{h}","size":1}},"b":{{"hash":"{h}","size":1}}}}}}"#);
        assert_eq!(asset_downloads(&index, Path::new("A")).unwrap().len(), 1);
    }

    #[test]
    fn libraries_without_a_url_are_never_downloaded() {
        let lib = |path: &str, url: &str| LibraryFile { path: path.into(), url: url.into(), sha1: None, size: None, native: false };
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(library_download(lib("Cargo.toml", ""), dir, "x").unwrap().is_none());
        let err = library_download(lib("nope.jar", ""), dir, "neoforge-1").unwrap_err();
        assert_eq!(err.to_string(), "nope.jar is missing; the neoforge-1 install did not complete");
        assert!(library_download(lib("nope.jar", "http://u"), dir, "x").unwrap().is_some());
    }

    #[test]
    fn bad_asset_hashes_are_refused() {
        for hash in ["a", "aabbcc", &"AA".repeat(20), &"zz".repeat(20), "\u{e9}\u{e9}", &"aa".repeat(21)] {
            let index = format!(r#"{{"objects":{{"a":{{"hash":"{hash}","size":1}}}}}}"#);
            assert!(asset_downloads(&index, Path::new("A")).is_err(), "{hash}");
        }
    }

    #[test]
    fn supported_versions_go_back_to_1_0_and_mark_snapshots() {
        let mut m: Manifest = serde_json::from_str(include_str!("../tests/fixtures/version_manifest_v2.json")).unwrap();
        let at = m.versions.iter().position(|v| v.id == "1.12.2").unwrap();
        for (id, kind) in [("1.0", "release"), ("b1.7.3", "old_beta"), ("1.14 Pre-Release 1", "snapshot"), ("1.5.2", "release")] {
            let mut v = m.versions[at].clone();
            (v.id, v.kind) = (id.into(), kind.into());
            m.versions.insert(at + 1, v);
        }
        let all = supported_versions(&m);
        assert!(all.iter().all(|v| v.id != "b1.7.3" && !v.id.contains(' ')), "no betas, no ids an instance cannot hold");
        assert!(all.iter().any(|v| v.snapshot), "the fixture holds a snapshot");
        let releases: Vec<_> = all.iter().filter(|v| !v.snapshot).map(|v| v.id.as_str()).collect();
        assert_eq!(releases, ["1.21.1", "1.20.1", "1.16.5", "1.12.2", "1.5.2", "1.0"]);
    }

    #[test]
    fn pre_1_6_index_is_copied_into_the_game_folder() {
        let index = include_str!("../tests/fixtures/asset-index-pre-1.6.json");
        let c = virtual_copies(index, Path::new("A"), "pre-1.6", Some(Path::new("G"))).unwrap();
        assert_eq!(c.len(), 749);
        assert!(c.iter().all(|(from, to)| from.starts_with("A") && to.starts_with(Path::new("G").join("resources"))));
        assert!(c.iter().any(|(_, to)| to == &Path::new("G").join("resources").join("icons").join("icon_16x16.png")));
        assert!(virtual_copies(index, Path::new("A"), "pre-1.6", None).unwrap().is_empty());
    }

    #[test]
    fn a_file_already_in_resources_is_never_overwritten() {
        let dir = crate::download::testing::tmp_dir();
        let (from, game) = (dir.join("objects").join("aa"), dir.join("game"));
        let (kept, fresh) = (game.join("resources").join("kept.png"), game.join("resources").join("sub").join("new.png"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(from.join("h"), b"object").unwrap();
        std::fs::write(&kept, b"mine").unwrap();
        let copies = [(from.join("h"), kept.clone()), (from.join("h"), fresh.clone())];
        copy_missing(&copies, Some(&game)).unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"mine");
        assert_eq!(std::fs::read(&fresh).unwrap(), b"object");
        copy_missing(&copies, None).unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"object");
    }

    #[test]
    fn a_copy_goes_through_a_part_file() {
        let dir = crate::download::testing::tmp_dir();
        let (from, to) = (dir.join("h"), dir.join("game").join("resources").join("a.png"));
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&from, b"object").unwrap();
        std::fs::write(crate::download::part_path(&to), b"cut off by a crash, longer than the object").unwrap();
        copy_missing(&[(from, to.clone())], Some(&dir.join("game"))).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"object");
        assert!(!crate::download::part_path(&to).exists());
    }
}
