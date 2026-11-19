use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use reqwest::Client;
use serde::Deserialize;

use super::{PackInstall, check_id, https_url};
use crate::download::{Download, Hash, fetch, get_ok};
use crate::loader::{Loader, LoaderKind};
use crate::sync::{Origin, Wanted};
use crate::{Error, Paths, Result};

#[derive(Deserialize)]
struct Index {
    dependencies: BTreeMap<String, String>,
    files: Vec<IndexFile>,
}

#[derive(Deserialize)]
struct IndexFile {
    path: String,
    hashes: Hashes,
    env: Option<Env>,
    downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    file_size: Option<u64>,
}

#[derive(Deserialize)]
struct Hashes {
    sha512: String,
}

#[derive(Deserialize)]
struct Env {
    client: Option<String>,
}

pub fn parse_index(json: &str) -> Result<PackInstall> {
    let index: Index = serde_json::from_str(json)?;
    let dep = |name: &str| index.dependencies.get(name);
    if dep("quilt-loader").is_some() {
        return Err(Error::Unsupported("the Quilt loader is not supported".into()));
    }
    let minecraft = dep("minecraft")
        .ok_or_else(|| Error::Other("the Modrinth pack has no Minecraft version".into()))?
        .clone();
    let loader = [("neoforge", LoaderKind::NeoForge), ("forge", LoaderKind::Forge), ("fabric-loader", LoaderKind::Fabric)]
        .into_iter()
        .find_map(|(key, kind)| dep(key).map(|v| Loader { kind, version: Some(v.clone()) }));
    let mut files = Vec::new();
    for f in index.files {
        if f.env.and_then(|e| e.client).as_deref() == Some("unsupported") {
            continue;
        }
        let url = f.downloads.first().ok_or_else(|| Error::Other(format!("the pack lists no download: {}", f.path)))?;
        let from = Origin::Url(https_url(url, &f.path)?);
        files.push(Wanted { path: f.path, from, hash: Some(Hash::Sha512(f.hashes.sha512)), size: f.file_size });
    }
    Ok(PackInstall { minecraft, loader, files })
}

pub fn read_mrpack(archive: &Path) -> Result<PackInstall> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    let mut index = String::new();
    zip.by_name("modrinth.index.json")?.read_to_string(&mut index)?;
    let mut pack = parse_index(&index)?;
    for prefix in ["overrides", "client-overrides"] {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry
                .enclosed_name()
                .ok_or_else(|| Error::Other("unsafe path in the pack archive".to_string()))?;
            let Ok(rel) = name.strip_prefix(prefix) else { continue };
            let path = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            let size = Some(bytes.len() as u64);
            pack.files.push(Wanted { path, from: Origin::Bytes(bytes), hash: None, size });
        }
    }
    Ok(pack)
}

#[derive(Deserialize)]
struct Version {
    files: Vec<VersionFile>,
}

#[derive(Deserialize)]
struct VersionFile {
    url: String,
    #[serde(default)]
    primary: bool,
    hashes: Hashes,
    size: Option<u64>,
}

pub async fn resolve(client: &Client, paths: &Paths, version: &str) -> Result<PackInstall> {
    check_id(version)?;
    let url = format!("https://api.modrinth.com/v2/version/{version}");
    let body = match get_ok(client, &url).await {
        Err(Error::Status { status: 404, .. }) => {
            return Err(Error::Other(format!("Modrinth has no version {version}")));
        }
        other => other?.text().await?,
    };
    let files = serde_json::from_str::<Version>(&body)?.files;
    let file = files
        .iter()
        .find(|f| f.primary)
        .or(files.first())
        .ok_or_else(|| Error::Other(format!("Modrinth version {version} has no files")))?;
    let path = paths.packs().join(format!("modrinth-{version}.mrpack"));
    let item = Download {
        url: https_url(&file.url, version)?,
        path: path.clone(),
        hash: Some(Hash::Sha512(file.hashes.sha512.clone())),
        size: file.size, bearer: None,
    };
    fetch(client, &item).await?;
    read_mrpack(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::tmp_dir;
    use std::io::Write;

    const INDEX: &str = include_str!("../../tests/fixtures/modrinth.index.json");

    #[test]
    fn index() {
        let p = parse_index(INDEX).unwrap();
        assert_eq!(p.files.len(), 2);
        assert!(p.files.iter().all(|f| matches!(f.hash, Some(Hash::Sha512(_))) && matches!(f.from, Origin::Url(_))));
        assert!(matches!(p.loader.unwrap().kind, LoaderKind::Fabric));
    }

    #[test]
    fn quilt_is_refused() {
        let json = r#"{"dependencies":{"minecraft":"1.21.1","quilt-loader":"0.26.0"},"files":[]}"#;
        assert!(matches!(parse_index(json), Err(Error::Unsupported(_))));
    }

    #[test]
    fn a_plain_http_download_is_refused() {
        let json = r#"{"dependencies":{"minecraft":"1.21.1"},"files":[{"path":"mods/a.jar","hashes":{"sha512":"x"},
            "downloads":["http://x/a.jar"],"fileSize":1}]}"#;
        assert!(parse_index(json).unwrap_err().to_string().contains("not https: mods/a.jar"));
    }

    #[test]
    fn overrides() {
        let dir = tmp_dir();
        let path = dir.join("pack.mrpack");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        w.start_file("modrinth.index.json", opts).unwrap();
        w.write_all(INDEX.as_bytes()).unwrap();
        w.add_directory("overrides/config/", opts).unwrap();
        for (name, body) in [
            ("overrides/config/a.toml", "plain"),
            ("client-overrides/config/a.toml", "client"),
            ("overrides/options.txt", "x"),
            ("server-overrides/server.properties", "y"),
        ] {
            w.start_file(name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();

        let p = read_mrpack(&path).unwrap();
        let a: Vec<_> = p.files.iter().filter(|f| f.path == "config/a.toml").collect();
        assert!(matches!(&a.last().unwrap().from, Origin::Bytes(b) if b == b"client"));
        assert!(p.files.iter().any(|f| f.path == "options.txt"));
        assert!(!p.files.iter().any(|f| f.path.contains("server.properties")));
        assert!(p.files.iter().all(|f| f.path != "config" && f.path != "config/"));
    }

    #[tokio::test]
    async fn bad_ids_fail_before_a_request() {
        let paths = Paths { root: std::env::temp_dir() };
        assert!(resolve(&crate::http_client(), &paths, "../x").await.is_err());
    }
}
