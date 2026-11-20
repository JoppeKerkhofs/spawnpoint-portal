use crate::download::{fetch, fetch_all, get_ok, Download, Hash, ProgressTx};
use crate::paths::check_relative;
use crate::{Error, Paths, Result};
use reqwest::Client;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const RUNTIMES_URL: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

pub fn runtime_manifest(all_json: &str, component: &str) -> Result<(String, String)> {
    let all: Value = serde_json::from_str(all_json)?;
    let manifest = &all["windows-x64"][component][0]["manifest"];
    match (manifest["url"].as_str(), manifest["sha1"].as_str()) {
        (Some(url), Some(sha1)) => Ok((url.into(), sha1.into())),
        _ => Err(Error::Other(format!("no Java runtime {component} for Windows"))),
    }
}

pub fn runtime_downloads(files_json: &str, dir: &Path) -> Result<Vec<Download>> {
    let manifest: Value = serde_json::from_str(files_json)?;
    let files = manifest["files"]
        .as_object()
        .ok_or_else(|| Error::Other("Java runtime manifest has no files".into()))?;
    let mut downloads = Vec::new();
    for (name, entry) in files.iter().filter(|(_, e)| e["type"] == "file") {
        check_relative(name)?;
        let raw = &entry["downloads"]["raw"];
        let (Some(url), Some(sha1)) = (raw["url"].as_str(), raw["sha1"].as_str()) else {
            return Err(Error::Other(format!("Java runtime file {name} has no raw download")));
        };
        downloads.push(Download {
            url: url.into(),
            path: dir.join(name),
            hash: Some(Hash::Sha1(sha1.into())),
            size: raw["size"].as_u64(), bearer: None,
        });
    }
    Ok(downloads)
}

pub async fn ensure(client: &Client, paths: &Paths, component: &str, progress: &ProgressTx) -> Result<PathBuf> {
    check_relative(component)?;
    let (url, sha1) = runtime_manifest(&get_ok(client, RUNTIMES_URL).await?.text().await?, component)?;
    let list = paths.java().join(format!("{component}.json"));
    fetch(client, &Download { url, path: list.clone(), hash: Some(Hash::Sha1(sha1)), size: None, bearer: None }).await?;
    let dir = paths.java().join(component);
    let items = runtime_downloads(&tokio::fs::read_to_string(&list).await?, &dir)?;
    fetch_all(client, "java", items, progress).await?;
    Ok(dir.join("bin").join("javaw.exe"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::Hash;
    use std::path::Path;

    #[test]
    fn finds_the_runtime() {
        let (url, sha1) = runtime_manifest(include_str!("../tests/fixtures/java-all.json"), "java-runtime-delta").unwrap();
        assert!(url.starts_with("https://piston-meta.mojang.com/v1/packages/"));
        assert!(sha1.len() == 40 && sha1.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(runtime_manifest(include_str!("../tests/fixtures/java-all.json"), "nope").is_err());
    }

    #[test]
    fn files_only() {
        let d = runtime_downloads(include_str!("../tests/fixtures/java-runtime-files.json"), Path::new("J")).unwrap();
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|x| x.path == Path::new("J").join("bin").join("javaw.exe")));
        assert!(d.iter().all(|x| matches!(x.hash, Some(Hash::Sha1(_)))));
    }

    #[test]
    fn refuses_paths_outside_the_folder() {
        for key in ["../x", "C:\\x", "a\\b", "/x"] {
            let raw = serde_json::json!({"url": "u", "sha1": "s", "size": 1});
            let json = serde_json::json!({"files": {key: {"type": "file", "downloads": {"raw": raw}}}}).to_string();
            assert!(runtime_downloads(&json, Path::new("J")).is_err(), "{key}");
        }
    }
}
