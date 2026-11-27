use reqwest::Client;
use serde::Deserialize;

use super::{PackInstall, check_id, https_url};
use crate::download::{Hash, get_ok};
use crate::loader::{Loader, LoaderKind};
use crate::sync::{Origin, Wanted};
use crate::{Error, Result};

#[derive(Deserialize)]
struct Version {
    status: Option<String>,
    message: Option<String>,
    #[serde(default)]
    targets: Vec<Target>,
    #[serde(default)]
    files: Vec<File>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    version: String,
}

#[derive(Deserialize)]
struct File {
    path: String,
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    sha1: String,
    size: Option<u64>,
    #[serde(default)]
    serveronly: bool,
    curseforge: Option<Curseforge>,
}

#[derive(Deserialize)]
struct Curseforge {
    project: String,
}

fn file_path(path: &str, name: &str) -> String {
    let folder = path.strip_prefix("./").unwrap_or(path).trim_end_matches('/');
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

pub fn parse(json: &str) -> Result<PackInstall> {
    let v: Version = serde_json::from_str(json)?;
    if v.status.as_deref() == Some("error") {
        return Err(Error::Other(v.message.unwrap_or_else(|| "FTB answered with an error".into())));
    }
    let target = |kind: &str| v.targets.iter().find(|t| t.kind == kind);
    let minecraft = target("game")
        .ok_or_else(|| Error::Other("the FTB pack has no Minecraft version".into()))?
        .version
        .clone();
    let loader = match target("modloader") {
        Some(t) => Some(Loader { kind: LoaderKind::parse(&t.name)?, version: Some(t.version.clone()) }),
        None => None,
    };
    let mut files = Vec::new();
    for f in v.files.into_iter().filter(|f| !f.serveronly) {
        let from = if f.url.is_empty() {
            let page = f.curseforge.filter(|c| check_id(&c.project).is_ok()).map(|c| format!("https://www.curseforge.com/projects/{}", c.project));
            Origin::Manual { name: f.name.clone(), page: page.unwrap_or_default() }
        } else {
            Origin::Url(https_url(&f.url, &f.name)?)
        };
        let hash = (!f.sha1.is_empty()).then(|| Hash::Sha1(f.sha1));
        files.push(Wanted { path: file_path(&f.path, &f.name), from, hash, size: f.size, force: false });
    }
    Ok(PackInstall { minecraft, loader, files })
}

pub async fn resolve(client: &Client, pack: &str, version: &str) -> Result<PackInstall> {
    check_id(pack)?;
    check_id(version)?;
    let url = format!("https://api.feed-the-beast.com/v1/modpacks/public/modpack/{pack}/{version}");
    let body = match get_ok(client, &url).await {
        Err(Error::Status { status: 404, .. }) => {
            return Err(Error::Other(format!("FTB has no pack {pack} version {version}")));
        }
        other => other?.text().await?,
    };
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftb_pack() {
        let p = parse(include_str!("../../tests/fixtures/ftb-pack-version.json")).unwrap();
        assert_eq!(p.minecraft, "1.21.1");
        assert!(matches!(p.loader.as_ref().unwrap().kind, LoaderKind::NeoForge));
        assert_eq!(p.loader.as_ref().unwrap().version.as_deref(), Some("21.1.265"));
        assert_eq!(p.files.len(), 6); // 7 in the fixture, 1 server-only
        assert!(p.files.iter().any(|f| f.path == "mods/AE2-Things-1.4.2-beta.jar"));
        assert!(p.files.iter().any(|f| f.path == "config/EnderStorage.cfg"));
        assert!(p.files.iter().any(|f| f.path == "options.txt"));
        assert!(p.files.iter().all(|f| !f.path.starts_with("./") && !f.path.starts_with('/')));
        assert_eq!(p.files.iter().filter(|f| matches!(f.from, Origin::Manual { .. })).count(), 1);
        assert!(p.files.iter().all(|f| !f.path.contains("server-only-helper")));
        let manual = p.files.iter().find_map(|f| match &f.from {
            Origin::Manual { name, page } => Some((name.as_str(), page.as_str())),
            _ => None,
        });
        assert_eq!(manual, Some(("Manual-Mod-1.0.jar", "https://www.curseforge.com/projects/609977")));
        assert!(p.files.iter().all(|f| matches!(f.hash, Some(Hash::Sha1(_)))));
    }

    #[test]
    fn an_api_error_is_reported() {
        let err = parse(r#"{"status":"error","message":"Modpack not found"}"#).unwrap_err();
        assert!(err.to_string().contains("Modpack not found"));
    }

    #[test]
    fn a_plain_http_download_is_refused() {
        let json = r#"{"status":"success","targets":[{"name":"minecraft","type":"game","version":"1.21.1"}],
            "files":[{"path":"./mods","name":"a.jar","url":"http://x/a.jar","sha1":"","size":1}]}"#;
        assert!(parse(json).unwrap_err().to_string().contains("not https: a.jar"));
    }

    #[test]
    fn file_paths_are_normalised() {
        assert_eq!(file_path("./", "options.txt"), "options.txt");
        assert_eq!(file_path("./mods/", "a.jar"), "mods/a.jar");
        assert_eq!(file_path("./mods", "a.jar"), "mods/a.jar");
        assert_eq!(file_path("config/sub", "a.cfg"), "config/sub/a.cfg");
    }

    #[test]
    fn a_bad_curseforge_project_leaves_the_page_empty() {
        let json = r#"{"status":"success","targets":[{"name":"minecraft","type":"game","version":"1.21.1"}],
            "files":[{"path":"./mods","name":"a.jar","url":"","sha1":"","size":1,"curseforge":{"project":"1/../x?y"}}]}"#;
        let p = parse(json).unwrap();
        assert!(matches!(&p.files[0].from, Origin::Manual { page, .. } if page.is_empty()));
    }

    #[tokio::test]
    async fn bad_ids_fail_before_a_request() {
        let c = crate::http_client();
        assert!(resolve(&c, "126/../x", "1").await.is_err());
        assert!(resolve(&c, "126", "").await.is_err());
    }
}
