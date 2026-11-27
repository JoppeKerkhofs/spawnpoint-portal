use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use super::PackInstall;
use crate::download::{Download, blocking, fetch};
use crate::loader::{Loader, LoaderKind};
use crate::panel::{self, PackFile, Panel};
use crate::paths::check_id;
use crate::sync::{Origin, Wanted};
use crate::{Error, Paths, Result};

#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub overrides: String,
    pub files: Vec<u64>,
}

#[derive(Deserialize)]
struct Raw {
    minecraft: RawMinecraft,
    overrides: Option<String>,
    #[serde(default)]
    files: Vec<RawFile>,
}

#[derive(Deserialize)]
struct RawMinecraft {
    version: String,
    #[serde(default, rename = "modLoaders")]
    mod_loaders: Vec<RawLoader>,
}

#[derive(Deserialize)]
struct RawLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(rename = "fileID")]
    file_id: u64,
    required: Option<bool>,
}

pub(crate) fn parse_manifest(json: &str) -> Result<Manifest> {
    let raw: Raw = serde_json::from_str(json)?;
    check_id(&raw.minecraft.version)?;
    let chosen = raw.minecraft.mod_loaders.iter().find(|l| l.primary).or(raw.minecraft.mod_loaders.first());
    let loader = match chosen {
        None => None,
        Some(l) => {
            let (name, version) = match l.id.split_once('-') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (l.id.as_str(), None),
            };
            if let Some(v) = &version {
                check_id(v)?;
            }
            Some(Loader { kind: LoaderKind::parse(name)?, version })
        }
    };
    Ok(Manifest {
        minecraft: raw.minecraft.version,
        loader,
        overrides: raw.overrides.filter(|o| !o.is_empty()).unwrap_or_else(|| "overrides".into()),
        files: raw.files.iter().filter(|f| f.required != Some(false)).map(|f| f.file_id).collect(),
    })
}

pub(crate) fn wanted(manifest: &Manifest, files: &[PackFile], archive: &Path) -> Result<PackInstall> {
    let mut out = Vec::new();
    for f in files {
        if f.name.contains(['/', '\\']) {
            return Err(Error::Other(format!("{:?} is not a plain file name", f.name)));
        }
        let lower = f.name.to_ascii_lowercase();
        // ponytail: by extension; ask the panel for the class when a shader pack lands in the wrong folder
        let folder = if lower.ends_with(".jar") {
            "mods"
        } else if lower.ends_with(".zip") {
            "resourcepacks"
        } else {
            return Err(Error::Other(format!("do not know where to put {}", f.name)));
        };
        let from = match &f.url {
            Some(url) if url.starts_with("https://") => Origin::Url(url.clone()),
            Some(_) => return Err(Error::Other(format!("the panel lists a download that is not https: {}", f.name))),
            None => {
                let page = f.page.clone().filter(|p| p.starts_with("https://")).unwrap_or_default();
                Origin::Manual { name: f.name.clone(), page }
            }
        };
        out.push(Wanted { path: format!("{folder}/{}", f.name), from, hash: None, size: None, force: false });
    }
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.enclosed_name().ok_or_else(|| Error::Other("unsafe path in the pack archive".into()))?;
        let Ok(rel) = name.strip_prefix(&manifest.overrides) else { continue };
        let path = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let size = Some(bytes.len() as u64);
        out.push(Wanted { path, from: Origin::Bytes(bytes), hash: None, size, force: false });
    }
    Ok(PackInstall { minecraft: manifest.minecraft.clone(), loader: manifest.loader.clone(), files: out })
}

pub async fn resolve(client: &reqwest::Client, paths: &Paths, panel: &Panel, slug: &str) -> Result<PackInstall> {
    let pack = panel::client_pack(panel, slug).await?;
    let url = pack.archive.ok_or_else(|| {
        Error::Other(format!("this pack's author forbids outside downloads; fetch it from {}", pack.page))
    })?;
    if !url.starts_with("https://") {
        return Err(Error::Other("the panel gave a pack download that is not https".into()));
    }
    let path = paths.packs().join(format!("curseforge-{}-{}.zip", pack.project, pack.file));
    fetch(client, &Download { url, path: path.clone(), hash: None, size: None, bearer: None }).await?;
    let archive = path.clone();
    let text = blocking(move || {
        let mut text = String::new();
        zip::ZipArchive::new(std::fs::File::open(&archive)?)?.by_name("manifest.json")?.read_to_string(&mut text)?;
        Ok(text)
    })
    .await?;
    let manifest = parse_manifest(&text)?;
    let files = panel::client_pack_files(panel, slug, &manifest.files).await?;
    blocking(move || wanted(&manifest, &files, &path)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::tmp_dir;
    use std::io::Write;

    #[test]
    fn manifest() {
        let m = parse_manifest(include_str!("../../tests/fixtures/curseforge-manifest.json")).unwrap();
        assert_eq!(m.files.len(), 2);
        assert_eq!((m.minecraft.as_str(), m.overrides.as_str()), ("1.20.1", "overrides"));
        let l = m.loader.unwrap();
        assert_eq!((l.kind, l.version.as_deref()), (LoaderKind::Forge, Some("47.2.0")));
    }

    #[test]
    fn the_primary_loader_wins_and_quilt_is_refused() {
        let json = |loaders: &str| format!(r#"{{"minecraft":{{"version":"1.21.1","modLoaders":{loaders}}},"files":[]}}"#);
        let m = parse_manifest(&json(r#"[{"id":"fabric-0.16.5"},{"id":"neoforge-21.1.90","primary":true}]"#)).unwrap();
        assert_eq!(m.loader.unwrap().kind, LoaderKind::NeoForge);
        assert!(parse_manifest(&json("[]")).unwrap().loader.is_none());
        assert!(matches!(parse_manifest(&json(r#"[{"id":"quilt-0.26.0"}]"#)), Err(Error::Unsupported(_))));
        assert!(parse_manifest(&json(r#"[{"id":"forge-1?x"}]"#)).is_err());
    }

    fn pf(name: &str, url: Option<&str>, page: Option<&str>) -> PackFile {
        PackFile { project: 1, file: 2, name: name.into(), url: url.map(Into::into), page: page.map(Into::into) }
    }

    fn archive(entries: &[(&str, &str)]) -> (crate::download::testing::Tmp, std::path::PathBuf) {
        let dir = tmp_dir();
        let path = dir.join("pack.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn files_and_overrides() {
        let m = parse_manifest(include_str!("../../tests/fixtures/curseforge-manifest.json")).unwrap();
        let (_dir, path) = archive(&[("manifest.json", "{}"), ("overrides/config/x.toml", "x"), ("other/y.txt", "y")]);
        let page = "https://www.curseforge.com/minecraft/mc-mods/b";
        let files = [pf("a.jar", Some("https://cdn/a.jar"), None), pf("b.jar", None, Some(page)), pf("pack.zip", Some("https://cdn/pack.zip"), None)];
        let p = wanted(&m, &files, &path).unwrap();
        assert_eq!((p.minecraft.as_str(), p.loader.clone().unwrap().kind), ("1.20.1", LoaderKind::Forge));
        let get = |path: &str| p.files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path}"));
        assert!(matches!(&get("mods/a.jar").from, Origin::Url(u) if u == "https://cdn/a.jar"));
        assert!(matches!(&get("mods/b.jar").from, Origin::Manual { name, page: pg } if name == "b.jar" && pg == page));
        assert!(matches!(&get("resourcepacks/pack.zip").from, Origin::Url(_)));
        assert!(matches!(&get("config/x.toml").from, Origin::Bytes(b) if b == b"x"));
        assert_eq!(p.files.len(), 4);
    }

    #[test]
    fn what_the_panel_sends_is_checked() {
        let m = parse_manifest(include_str!("../../tests/fixtures/curseforge-manifest.json")).unwrap();
        let (_dir, path) = archive(&[]);
        for bad in [pf("a/b.jar", None, None), pf(r"..\a.jar", None, None), pf("a.jar", Some("http://cdn/a.jar"), None), pf("a.txt", None, None)] {
            assert!(wanted(&m, &[bad], &path).is_err());
        }
        let p = wanted(&m, &[pf("a.jar", None, Some("javascript:x"))], &path).unwrap();
        assert!(matches!(&p.files[0].from, Origin::Manual { page, .. } if page.is_empty()));
    }
}
