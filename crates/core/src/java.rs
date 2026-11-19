use crate::download::{fetch_all, Download, Hash, ProgressTx};
use crate::{Error, Paths, Result};
use reqwest::Client;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const RUNTIMES_URL: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

pub fn runtime_manifest_url(all_json: &str, component: &str) -> Result<String> {
    let all: Value = serde_json::from_str(all_json)?;
    all["windows-x64"][component][0]["manifest"]["url"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| Error::Other(format!("no Java runtime {component} for Windows")))
}

pub fn runtime_downloads(files_json: &str, dir: &Path) -> Result<Vec<Download>> {
    let manifest: Value = serde_json::from_str(files_json)?;
    let files = manifest["files"]
        .as_object()
        .ok_or_else(|| Error::Other("Java runtime manifest has no files".into()))?;
    let mut downloads = Vec::new();
    for (name, entry) in files.iter().filter(|(_, e)| e["type"] == "file") {
        let raw = &entry["downloads"]["raw"];
        let (Some(url), Some(sha1)) = (raw["url"].as_str(), raw["sha1"].as_str()) else {
            return Err(Error::Other(format!("Java runtime file {name} has no raw download")));
        };
        downloads.push(Download {
            url: url.into(),
            path: dir.join(name),
            hash: Some(Hash::Sha1(sha1.into())),
            size: raw["size"].as_u64(),
        });
    }
    Ok(downloads)
}

async fn get_text(client: &Client, url: &str) -> Result<String> {
    let response = client.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status { url: url.into(), status: status.as_u16() });
    }
    Ok(response.text().await?)
}

pub async fn ensure(client: &Client, paths: &Paths, component: &str, progress: &ProgressTx) -> Result<PathBuf> {
    let url = runtime_manifest_url(&get_text(client, RUNTIMES_URL).await?, component)?;
    let dir = paths.java().join(component);
    let items = runtime_downloads(&get_text(client, &url).await?, &dir)?;
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
        let url = runtime_manifest_url(include_str!("../tests/fixtures/java-all.json"), "java-runtime-delta").unwrap();
        assert!(url.starts_with("https://piston-meta.mojang.com/v1/packages/"));
        assert!(runtime_manifest_url(include_str!("../tests/fixtures/java-all.json"), "nope").is_err());
    }

    #[test]
    fn files_only() {
        let d = runtime_downloads(include_str!("../tests/fixtures/java-runtime-files.json"), Path::new("J")).unwrap();
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|x| x.path == Path::new("J").join("bin").join("javaw.exe")));
        assert!(d.iter().all(|x| matches!(x.hash, Some(Hash::Sha1(_)))));
    }
}
