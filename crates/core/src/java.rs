use crate::download::{fetch, fetch_all, get_ok, Download, Hash, ProgressTx};
use crate::paths::check_relative;
use crate::platform::{self, Platform};
use crate::{Error, Paths, Result};
use reqwest::Client;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const RUNTIMES_URL: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

pub fn runtime_manifest(platform: &Platform, all_json: &str, component: &str) -> Result<(String, String)> {
    let all: Value = serde_json::from_str(all_json)?;
    let manifest = &all[platform.java_runtime][component][0]["manifest"];
    match (manifest["url"].as_str(), manifest["sha1"].as_str()) {
        (Some(url), Some(sha1)) => Ok((url.into(), sha1.into())),
        // Mojang has Java 17 and newer for Apple chips, and game libraries for them from 1.19 on.
        _ if platform.os == "osx" => Err(Error::Unsupported("Minecraft before 1.19 does not run on Apple chips yet".into())),
        _ => Err(Error::Other(format!("no Java runtime {component} for {}", platform.os))),
    }
}

// The files that must be marked as programs where the system asks for that.
#[cfg(any(unix, test))]
fn executables(files_json: &str, dir: &Path) -> Result<Vec<PathBuf>> {
    let manifest: Value = serde_json::from_str(files_json)?;
    let files = manifest["files"].as_object().into_iter().flatten();
    Ok(files.filter(|(_, e)| e["type"] == "file" && e["executable"] == true).map(|(name, _)| dir.join(name)).collect())
}

pub fn runtime_downloads(files_json: &str, dir: &Path) -> Result<Vec<Download>> {
    let manifest: Value = serde_json::from_str(files_json)?;
    let files = manifest["files"]
        .as_object()
        .ok_or_else(|| Error::Other("Java runtime manifest has no files".into()))?;
    let mut downloads = Vec::new();
    // ponytail: entries of type `link` are skipped. They are copies of licence texts, and three
    // extras in the Linux Java 8 that nothing here starts. Create them if a runtime ever links a file it needs.
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
    let platform = platform::current();
    let (url, sha1) = runtime_manifest(platform, &get_ok(client, RUNTIMES_URL).await?.text().await?, component)?;
    let list = paths.java().join(format!("{component}.json"));
    fetch(client, &Download { url, path: list.clone(), hash: Some(Hash::Sha1(sha1)), size: None, bearer: None }).await?;
    let dir = paths.java().join(component);
    let files = tokio::fs::read_to_string(&list).await?;
    fetch_all(client, "java", runtime_downloads(&files, &dir)?, progress).await?;
    #[cfg(unix)]
    for program in executables(&files, &dir)? {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(binary(platform, &dir))
}

fn binary(platform: &Platform, runtime: &Path) -> PathBuf {
    runtime.join(platform.java_home).join("bin").join(platform.java)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::Hash;
    use crate::platform::{LINUX, MACOS, WINDOWS};
    use std::path::Path;

    #[test]
    fn finds_the_runtime() {
        let all = include_str!("../tests/fixtures/java-all.json");
        let (url, sha1) = runtime_manifest(&WINDOWS, all, "java-runtime-delta").unwrap();
        assert!(url.starts_with("https://piston-meta.mojang.com/v1/packages/"));
        assert!(sha1.len() == 40 && sha1.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(runtime_manifest(&WINDOWS, all, "nope").is_err());
        let (linux, _) = runtime_manifest(&LINUX, all, "java-runtime-delta").unwrap();
        assert!(linux.starts_with("https://piston-meta.mojang.com/v1/packages/") && linux != url);
        assert!(runtime_manifest(&LINUX, all, "jre-legacy").unwrap_err().to_string().ends_with("for linux"));
    }

    #[test]
    fn a_mac_gets_java_for_1_19_and_newer_and_a_sentence_for_older() {
        let all = include_str!("../tests/fixtures/java-all.json");
        assert!(runtime_manifest(&MACOS, all, "java-runtime-delta").is_ok());
        // What 1.18.2 and 1.16.5 ask for: listed for Apple chips, and empty.
        for older in ["java-runtime-beta", "jre-legacy"] {
            let err = runtime_manifest(&MACOS, all, older).unwrap_err();
            assert!(matches!(&err, Error::Unsupported(m) if m == "Minecraft before 1.19 does not run on Apple chips yet"), "{err}");
        }
        assert_eq!(binary(&MACOS, Path::new("J")), Path::new("J/jre.bundle/Contents/Home/bin/java"));
        assert_eq!(binary(&WINDOWS, Path::new("J")), Path::new("J").join("bin").join("javaw.exe"));
    }

    #[test]
    fn a_linux_runtime_is_files_and_programs_without_its_links() {
        let list = include_str!("../tests/fixtures/java-runtime-files-linux.json");
        let d = runtime_downloads(list, Path::new("J")).unwrap();
        let java = Path::new("J").join("bin/java");
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|x| x.path == java) && !d.iter().any(|x| x.path.ends_with("legal/java.compiler/LICENSE")));
        assert_eq!(executables(list, Path::new("J")).unwrap(), [java]);
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
