use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::download::{fetch, get_ok, Download};
use crate::loader::LoaderKind;
use crate::paths::check_relative;
use crate::version::{maven_path, Library};
use crate::{Error, Paths, Result};

const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const FORGE_PROMOTIONS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

#[derive(Deserialize)]
struct Versions {
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct Promotions {
    promos: BTreeMap<String, String>,
}

pub fn newest_neoforge(versions_json: &str, minecraft: &str) -> Result<String> {
    let mut parts: Vec<&str> = minecraft.split('.').collect();
    // 1.x versions drop the leading 1; year-based ones (26.1) keep it and gain a 0 patch.
    if parts[0] == "1" {
        parts.remove(0);
    }
    let want = if minecraft.starts_with("1.") { 2 } else { 3 };
    while parts.len() < want {
        parts.push("0");
    }
    let prefix = format!("{}.", parts.join("."));
    let all: Vec<String> = serde_json::from_str::<Versions>(versions_json)?
        .versions
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .collect();
    all.iter()
        .rev()
        .find(|v| !v.contains("-beta"))
        .or(all.last())
        .cloned()
        .ok_or_else(|| Error::Other(format!("NeoForge has no build for Minecraft {minecraft}")))
}

pub fn newest_forge(promotions_json: &str, minecraft: &str) -> Result<String> {
    let promos = serde_json::from_str::<Promotions>(promotions_json)?.promos;
    ["recommended", "latest"]
        .iter()
        .find_map(|k| promos.get(&format!("{minecraft}-{k}")))
        .cloned()
        .ok_or_else(|| Error::Other(format!("Forge has no build for Minecraft {minecraft}")))
}

pub fn installer_url(kind: LoaderKind, minecraft: &str, version: &str) -> String {
    match kind {
        LoaderKind::Forge => format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{minecraft}-{version}/forge-{minecraft}-{version}-installer.jar"
        ),
        _ => format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
        ),
    }
}

pub async fn installer(
    client: &Client,
    paths: &Paths,
    kind: LoaderKind,
    minecraft: &str,
    version: Option<&str>,
) -> Result<(String, PathBuf)> {
    let name = match kind {
        LoaderKind::NeoForge => "NeoForge",
        LoaderKind::Forge => "Forge",
        LoaderKind::Fabric => return Err(Error::Other("Fabric has no installer".into())),
    };
    check_relative(minecraft)?;
    let version = match version {
        Some(v) => v.to_string(),
        None if kind == LoaderKind::Forge => {
            newest_forge(&get_ok(client, FORGE_PROMOTIONS).await?.text().await?, minecraft)?
        }
        None => newest_neoforge(&get_ok(client, NEOFORGE_VERSIONS).await?.text().await?, minecraft)?,
    };
    check_relative(&version)?;
    let path = paths.installers().join(format!("{}-{minecraft}-{version}.jar", kind.as_str()));
    let item = Download { url: installer_url(kind, minecraft, &version), path: path.clone(), hash: None, size: None };
    fetch(client, &item).await.map_err(|e| match e {
        Error::Status { status: 404, .. } => {
            Error::Other(format!("{name} {version} does not exist for Minecraft {minecraft}"))
        }
        e => e,
    })?;
    Ok((version, path))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProfile {
    pub minecraft: String,
    pub libraries: Vec<Library>,
    pub data: BTreeMap<String, DataEntry>,
    pub processors: Vec<Processor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    pub client: String,
    pub server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Processor {
    #[serde(default)]
    pub sides: Vec<String>,
    pub jar: String,
    pub classpath: Vec<String>,
    pub args: Vec<String>,
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    pub outputs: Vec<(PathBuf, String)>,
}

pub struct Context<'a> {
    pub libraries: &'a Path,
    pub minecraft_jar: &'a Path,
    pub installer: &'a Path,
    pub extracted: &'a Path,
}

fn under(base: &Path, relative: &str) -> Result<PathBuf> {
    check_relative(relative)?;
    Ok(relative.split('/').fold(base.to_path_buf(), |p, part| p.join(part)))
}

fn library(ctx: &Context, coordinate: &str) -> Result<PathBuf> {
    under(ctx.libraries, &maven_path(coordinate)?)
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// `in_data` values may point into the installer and may not reference other data.
fn resolve(value: &str, profile: &InstallProfile, ctx: &Context, in_data: bool) -> Result<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        let end = rest[start..].find('}').ok_or_else(|| Error::Other(format!("unclosed placeholder in {value:?}")))? + start;
        let name = &rest[start + 1..end];
        out.push_str(&rest[..start]);
        let entry = profile.data.get(name).filter(|_| !in_data);
        out.push_str(&match (entry, name) {
            (Some(e), _) => resolve(&e.client, profile, ctx, true)?,
            (None, "SIDE") => "client".into(),
            (None, "MINECRAFT_JAR") => text(ctx.minecraft_jar),
            (None, "MINECRAFT_VERSION") => profile.minecraft.clone(),
            (None, "ROOT") => text(ctx.libraries.parent().unwrap_or(ctx.libraries)),
            (None, "INSTALLER") => text(ctx.installer),
            (None, "LIBRARY_DIR") => text(ctx.libraries),
            _ => return Err(Error::Other(format!("unknown placeholder {{{name}}}"))),
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    if let Some(coordinate) = out.strip_prefix('[').and_then(|o| o.strip_suffix(']')) {
        return Ok(text(&library(ctx, coordinate)?));
    }
    if let Some(quoted) = out.strip_prefix('\'').and_then(|o| o.strip_suffix('\'')) {
        return Ok(quoted.into());
    }
    match out.strip_prefix('/').filter(|_| in_data) {
        Some(inner) => Ok(text(&under(ctx.extracted, inner)?)),
        None => Ok(out),
    }
}

pub fn steps(profile: &InstallProfile, ctx: &Context) -> Result<Vec<Step>> {
    profile
        .processors
        .iter()
        .filter(|p| p.sides.is_empty() || p.sides.iter().any(|s| s == "client"))
        .map(|p| {
            Ok(Step {
                jar: library(ctx, &p.jar)?,
                classpath: p.classpath.iter().map(|c| library(ctx, c)).collect::<Result<_>>()?,
                args: p.args.iter().map(|a| resolve(a, profile, ctx, false)).collect::<Result<_>>()?,
                outputs: p
                    .outputs
                    .iter()
                    .map(|(k, v)| Ok((PathBuf::from(resolve(k, profile, ctx, false)?), resolve(v, profile, ctx, false)?)))
                    .collect::<Result<_>>()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_versions() {
        let j = include_str!("../../tests/fixtures/neoforge-versions.json");
        assert_eq!(newest_neoforge(j, "1.21.1").unwrap(), "21.1.267");
        assert!(newest_neoforge(j, "1.16.5").is_err());
        assert!(newest_neoforge(j, "1.20.1").is_err());
        assert_eq!(newest_neoforge(j, "1.21").unwrap(), "21.0.167");
        assert_eq!(newest_neoforge(j, "1.21.2").unwrap(), "21.2.1-beta");
        assert_eq!(newest_neoforge(j, "26.3").unwrap(), "26.3.0.113");
        assert!(newest_neoforge(j, "26.4").is_err());
    }

    #[test]
    fn forge_versions() {
        let j = include_str!("../../tests/fixtures/forge-promotions.json");
        assert_eq!(newest_forge(j, "1.20.1").unwrap(), "47.4.28");
        assert_eq!(newest_forge(j, "1.16.5").unwrap(), "36.2.34");
        assert!(newest_forge(j, "0.0.1").is_err());
    }

    #[test]
    fn installer_urls() {
        assert_eq!(installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.90"), "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.90/neoforge-21.1.90-installer.jar");
        assert_eq!(installer_url(LoaderKind::Forge, "1.20.1", "47.3.0"), "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar");
    }

    fn fixture(name: &str) -> InstallProfile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    fn ctx() -> Context<'static> {
        Context {
            libraries: Path::new(r"C:\data root\meta\libraries"),
            minecraft_jar: Path::new(r"C:\data root\meta\versions\1.21.1\1.21.1.jar"),
            installer: Path::new(r"C:\data root\meta\installers\neoforge-1.21.1-21.1.267.jar"),
            extracted: Path::new(r"C:\data root\meta\extracted"),
        }
    }

    #[test]
    fn client_processors_only() {
        let p = fixture("neoforge-install_profile.json");
        let s = steps(&p, &ctx()).unwrap();
        assert!(!s.is_empty());
        assert!(s.len() <= p.processors.len());
        assert!(!s.iter().any(|x| x.args.iter().any(|a| a == "server" || a.contains('{') || a.starts_with('['))));
        assert!(s.iter().all(|x| x.jar.starts_with(r"C:\data root\meta\libraries")));
        assert!(s.iter().any(|x| x.args.iter().any(|a| a == r"C:\data root\meta\versions\1.21.1\1.21.1.jar")));
    }

    #[test]
    fn binpatch_comes_from_the_installer() {
        let s = steps(&fixture("neoforge-install_profile.json"), &ctx()).unwrap();
        assert!(s.iter().any(|x| x.args.iter().any(|a| a.ends_with(r"data\client.lzma"))));
    }

    #[test]
    fn the_older_profile_parses_too() {
        let p = fixture("forge-1.16.5-install_profile.json");
        let s = steps(&p, &ctx()).unwrap();
        let (_, hash) = &s.iter().find(|x| !x.outputs.is_empty()).unwrap().outputs[0];
        assert_eq!(hash.len(), 40);
    }

    fn one_processor(jar: &str, arg: &str) -> InstallProfile {
        serde_json::from_value(serde_json::json!({
            "minecraft": "1.21.1", "libraries": [], "data": {},
            "processors": [{ "jar": jar, "classpath": [], "args": [arg] }]
        }))
        .unwrap()
    }

    #[test]
    fn an_unknown_placeholder_is_an_error() {
        assert!(steps(&one_processor("a:b:1", "{NOPE}"), &ctx()).is_err());
    }

    #[test]
    fn coordinates_cannot_leave_the_libraries_folder() {
        assert!(steps(&one_processor("a:b:../../x", "x"), &ctx()).is_err());
        assert!(steps(&one_processor("a:b:1", "[a:b:..:x]"), &ctx()).is_err());
    }
}
