use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::download::{fetch, fetch_all, file_matches, get_ok, sha1_file, Download, Hash, Progress, ProgressTx};
use crate::loader::LoaderKind;
use crate::paths::check_relative;
use crate::version::{library_files, maven_path, Library, LibraryFile, VersionJson};
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

pub async fn newest_version(client: &Client, kind: LoaderKind, minecraft: &str) -> Result<String> {
    match kind {
        LoaderKind::Forge => newest_forge(&get_ok(client, FORGE_PROMOTIONS).await?.text().await?, minecraft),
        LoaderKind::NeoForge => newest_neoforge(&get_ok(client, NEOFORGE_VERSIONS).await?.text().await?, minecraft),
        LoaderKind::Fabric => Err(Error::Other("Fabric has no installer".into())),
    }
}

fn display_name(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::NeoForge => "NeoForge",
        LoaderKind::Forge => "Forge",
        LoaderKind::Fabric => "Fabric",
    }
}

pub async fn installer(
    client: &Client,
    paths: &Paths,
    kind: LoaderKind,
    minecraft: &str,
    version: &str,
) -> Result<(String, PathBuf)> {
    if kind == LoaderKind::Fabric {
        return Err(Error::Other("Fabric has no installer".into()));
    }
    let name = display_name(kind);
    check_relative(minecraft)?;
    check_relative(version)?;
    let path = paths.installers().join(format!("{}-{minecraft}-{version}.jar", kind.as_str()));
    let item = Download { url: installer_url(kind, minecraft, version), path: path.clone(), hash: None, size: None };
    fetch(client, &item).await.map_err(|e| match e {
        Error::Status { status: 404, .. } => {
            Error::Other(format!("{name} {version} does not exist for Minecraft {minecraft}"))
        }
        e => e,
    })?;
    Ok((version.to_string(), path))
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

fn output_path(key: &str, profile: &InstallProfile, ctx: &Context) -> Result<PathBuf> {
    let path = PathBuf::from(resolve(key, profile, ctx, false)?);
    let inside = path.starts_with(ctx.libraries) || path.starts_with(ctx.extracted);
    if !inside || path.components().any(|c| c == Component::ParentDir) {
        return Err(Error::Other(format!("output {key:?} is outside the launcher's folders")));
    }
    Ok(path)
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
                    .map(|(k, v)| Ok((output_path(k, profile, ctx)?, resolve(v, profile, ctx, false)?)))
                    .collect::<Result<_>>()?,
            })
        })
        .collect()
}

fn is_done(step: &Step) -> bool {
    !step.outputs.is_empty() && step.outputs.iter().all(|(path, sha1)| file_matches(path, &Hash::Sha1(sha1.clone())))
}

fn manifest_main_class(manifest: &str) -> Option<String> {
    let unfolded = manifest.replace("\r\n", "\n").replace("\n ", "");
    unfolded.lines().find_map(|l| l.strip_prefix("Main-Class:")).map(|v| v.trim().to_string())
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

type Installer = zip::ZipArchive<std::fs::File>;

fn read_json<T: serde::de::DeserializeOwned>(zip: &mut Installer, name: &str) -> Result<T> {
    Ok(serde_json::from_reader(zip.by_name(name)?)?)
}

// Copies unless dest already matches; Ok(false) when the installer has no such entry.
fn copy_out(zip: &mut Installer, name: &str, dest: &Path, sha1: Option<&str>) -> Result<bool> {
    let current = match sha1 {
        Some(h) => file_matches(dest, &Hash::Sha1(h.into())),
        None => dest.exists(),
    };
    if current {
        return Ok(true);
    }
    let mut entry = match zip.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::io::copy(&mut entry, &mut std::fs::File::create(&part)?)?;
    if let Some(expected) = sha1 {
        let actual = sha1_file(&part)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            return Err(Error::Hash { path: dest.to_path_buf(), expected: expected.into(), actual });
        }
    }
    std::fs::rename(&part, dest)?;
    Ok(true)
}

fn check_minecraft(loader: &str, version: &str, installer_minecraft: &str, requested: &str) -> Result<()> {
    match installer_minecraft == requested {
        true => Ok(()),
        false => Err(Error::Other(format!("{loader} {version} is for Minecraft {installer_minecraft}, not {requested}"))),
    }
}

fn installed(marker: &Path, produced: &[PathBuf]) -> bool {
    marker.exists() && produced.iter().all(|p| p.exists())
}

fn main_class(jar: &Path) -> Result<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut manifest = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("META-INF/MANIFEST.MF")?, &mut manifest)?;
    manifest_main_class(&manifest).ok_or_else(|| Error::Other(format!("{} has no Main-Class", jar.display())))
}

// javaw shows failures in a dialog instead of stderr.
fn console_java(javaw: &Path) -> PathBuf {
    javaw.with_file_name("java.exe")
}

async fn run_step(java: &Path, step: &Step, cwd: &Path, loader: &str, number: usize) -> Result<()> {
    let classpath = std::iter::once(&step.jar).chain(&step.classpath).map(|p| text(p)).collect::<Vec<_>>().join(";");
    let out = tokio::process::Command::new(console_java(java))
        .arg("-cp")
        .arg(classpath)
        .arg(main_class(&step.jar)?)
        .args(&step.args)
        .current_dir(cwd)
        .output()
        .await?;
    if !out.status.success() {
        let all = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        return Err(Error::Other(format!("{loader} installer step {number} failed: {}", last_lines(&all, 20))));
    }
    for (path, sha1) in &step.outputs {
        if !file_matches(path, &Hash::Sha1(sha1.clone())) {
            return Err(Error::Other(format!("{loader} installer step {number} wrote a wrong {}", path.display())));
        }
    }
    Ok(())
}

// The profile's libraries, plus those of version.json that only the installer holds.
fn installer_libraries(profile: &InstallProfile, loader_json: &VersionJson) -> Result<Vec<LibraryFile>> {
    let listed = VersionJson { libraries: profile.libraries.clone(), ..loader_json.clone() };
    let mut files = library_files(&listed)?;
    files.extend(library_files(loader_json)?.into_iter().filter(|l| l.url.is_empty()));
    Ok(files)
}

pub async fn install(
    client: &Client,
    paths: &Paths,
    kind: LoaderKind,
    minecraft: &str,
    version: &str,
    java: &Path,
    progress: &ProgressTx,
) -> Result<VersionJson> {
    let (version, jar) = installer(client, paths, kind, minecraft, version).await?;
    let mut zip: Installer = zip::ZipArchive::new(std::fs::File::open(&jar)?)?;
    let profile: InstallProfile = read_json(&mut zip, "install_profile.json")?;
    let loader_json: VersionJson = read_json(&mut zip, "version.json")?;
    check_minecraft(display_name(kind), &version, &profile.minecraft, minecraft)?;
    loader_json.check_ids()?;
    let file = paths.versions().join(&loader_json.id).join(format!("{}.json", loader_json.id));
    std::fs::create_dir_all(file.parent().unwrap_or(&paths.versions()))?;
    std::fs::write(&file, serde_json::to_vec(&loader_json)?)?;

    let libraries = paths.libraries();
    let extracted = paths.installers().join(format!("{}-{minecraft}-{version}", kind.as_str()));
    let minecraft_jar = paths.versions().join(&profile.minecraft).join(format!("{}.jar", profile.minecraft));
    let ctx = Context { libraries: &libraries, minecraft_jar: &minecraft_jar, installer: &jar, extracted: &extracted };
    let steps = steps(&profile, &ctx)?;
    let mut downloads = Vec::new();
    for lib in installer_libraries(&profile, &loader_json)? {
        let dest = under(&libraries, &lib.path)?;
        if !lib.url.is_empty() {
            downloads.push(Download { url: lib.url, path: dest, hash: lib.sha1.map(Hash::Sha1), size: lib.size });
        } else {
            copy_out(&mut zip, &format!("maven/{}", lib.path), &dest, lib.sha1.as_deref())?;
        }
    }
    // Most NeoForge processors declare no outputs, so a marker records that all of them ran.
    let marker = extracted.join("done");
    let produced = profile
        .data
        .values()
        .filter_map(|e| e.client.strip_prefix('[')?.strip_suffix(']'))
        .map(|coordinate| library(&ctx, coordinate))
        .collect::<Result<Vec<_>>>()?;
    if installed(&marker, &produced) || steps.iter().all(is_done) {
        return Ok(loader_json);
    }
    fetch_all(client, "loader", downloads, progress).await?;

    for entry in profile.data.values() {
        if let Some(inner) = entry.client.strip_prefix('/') {
            if !copy_out(&mut zip, inner, &under(&extracted, inner)?, None)? {
                return Err(Error::Other(format!("the installer has no {inner}")));
            }
        }
    }

    let meta = paths.root.join("meta");
    let total = steps.len() as u64;
    for (i, step) in steps.iter().enumerate() {
        if !is_done(step) {
            run_step(java, step, &meta, kind.as_str(), i + 1).await?;
        }
        let _ = progress.send(Progress { stage: "loader".into(), done: i as u64 + 1, total });
    }
    std::fs::create_dir_all(&extracted)?;
    std::fs::write(&marker, "")?;
    Ok(loader_json)
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

    fn with_outputs(key: &str) -> InstallProfile {
        serde_json::from_value(serde_json::json!({
            "minecraft": "1.21.1", "libraries": [], "data": {},
            "processors": [{ "jar": "a:b:1", "classpath": [], "args": [], "outputs": { key: "'abc'" } }]
        }))
        .unwrap()
    }

    #[test]
    fn output_keys_stay_inside_the_launcher_folders() {
        for bad in [r"C:\Windows\x", r"..\..\x", "{ROOT}/../x", "{LIBRARY_DIR}/../x"] {
            assert!(steps(&with_outputs(bad), &ctx()).is_err(), "{bad}");
        }
        assert!(steps(&with_outputs("[a:b:1]"), &ctx()).is_ok());
        assert!(steps(&with_outputs("{LIBRARY_DIR}/a/b.jar"), &ctx()).is_ok());
    }

    #[test]
    fn processors_are_filtered_by_side() {
        let p: InstallProfile = serde_json::from_value(serde_json::json!({
            "minecraft": "1.21.1", "libraries": [], "data": {},
            "processors": [
                { "sides": ["server"], "jar": "a:server:1", "classpath": [], "args": [] },
                { "sides": ["client"], "jar": "a:client:1", "classpath": [], "args": [] },
                { "jar": "a:both:1", "classpath": [], "args": [] }
            ]
        }))
        .unwrap();
        let jars: Vec<String> = steps(&p, &ctx()).unwrap().iter().map(|s| text(&s.jar)).collect();
        assert_eq!(jars.len(), 2);
        assert!(jars[0].contains("client") && jars[1].contains("both"), "{jars:?}");
    }

    fn step_with(outputs: Vec<(PathBuf, String)>) -> Step {
        Step { jar: PathBuf::new(), classpath: vec![], args: vec![], outputs }
    }

    #[test]
    fn a_step_with_right_outputs_is_skipped() {
        let file = std::env::temp_dir().join(format!("portal-forge-{}.txt", std::process::id()));
        std::fs::write(&file, "abc").unwrap();
        let sha = "a9993e364706816aba3e25717850c26c9cd0d89d".to_string();
        let result = (
            is_done(&step_with(vec![(file.clone(), sha.clone())])),
            is_done(&step_with(vec![(file.clone(), "0".repeat(40))])),
            is_done(&step_with(vec![(file.clone(), sha.clone()), (file.with_extension("missing"), sha)])),
            is_done(&step_with(vec![])),
        );
        let _ = std::fs::remove_file(&file);
        assert_eq!(result, (true, false, false, false));
    }

    #[test]
    fn main_class_survives_wrapped_manifest_lines() {
        let m = "Manifest-Version: 1.0
Main-Class: net.neoforged.installertools.
 ConsoleTool
Other: x
";
        assert_eq!(manifest_main_class(m).as_deref(), Some("net.neoforged.installertools.ConsoleTool"));
        assert_eq!(manifest_main_class("Manifest-Version: 1.0
"), None);
    }

    #[test]
    fn version_json_libraries_held_by_the_installer_are_listed() {
        let profile = fixture("forge-1.16.5-install_profile.json");
        let version: VersionJson = serde_json::from_value(serde_json::json!({
            "id": "x", "libraries": [
                { "name": "a:forge:1", "downloads": { "artifact": { "path": "a/forge/1/forge-1.jar", "url": "" } } },
                { "name": "a:other:1", "downloads": { "artifact": { "path": "a/other/1/other-1.jar", "url": "http://u" } } }
            ]
        }))
        .unwrap();
        let paths: Vec<String> = installer_libraries(&profile, &version).unwrap().into_iter().map(|l| l.path).collect();
        assert!(paths.contains(&"a/forge/1/forge-1.jar".to_string()));
        assert!(!paths.contains(&"a/other/1/other-1.jar".to_string()));
    }

    #[test]
    fn processors_run_with_the_console_java() {
        assert_eq!(console_java(Path::new(r"C:\j\bin\javaw.exe")), PathBuf::from(r"C:\j\bin\java.exe"));
    }

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!("portal-forge-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }

    fn installer_with(dir: &Path, entry: &str, body: &[u8]) -> Installer {
        let path = dir.join("installer.jar");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        w.start_file(entry, zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut w, body).unwrap();
        w.finish().unwrap();
        zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
    }

    const ABC: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    #[test]
    fn files_copied_out_of_the_installer_are_checked() {
        let dir = tmp("copy");
        let mut zip = installer_with(&dir.0, "maven/a/b.jar", b"abc");
        let dest = dir.0.join("out").join("b.jar");
        let wrong = "0".repeat(40);
        assert!(matches!(copy_out(&mut zip, "maven/a/b.jar", &dest, Some(&wrong)), Err(Error::Hash { .. })));
        assert!(!dest.exists());

        assert!(copy_out(&mut zip, "maven/a/b.jar", &dest, Some(ABC)).unwrap());
        assert_eq!(std::fs::read(&dest).unwrap(), b"abc");

        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        std::fs::File::options().write(true).open(&dest).unwrap().set_modified(old).unwrap();
        copy_out(&mut zip, "maven/a/b.jar", &dest, Some(ABC)).unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().modified().unwrap(), old);

        std::fs::write(&dest, "cut sh").unwrap();
        copy_out(&mut zip, "maven/a/b.jar", &dest, Some(ABC)).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"abc");

        assert!(!copy_out(&mut zip, "maven/none.jar", &dir.0.join("none.jar"), None).unwrap());
    }

    #[test]
    fn the_installer_must_be_for_the_requested_minecraft() {
        assert!(check_minecraft("NeoForge", "21.1.267", "1.21.1", "1.21.1").is_ok());
        let err = check_minecraft("NeoForge", "21.1.267", "1.21.1", "1.20.4").unwrap_err();
        assert_eq!(err.to_string(), "NeoForge 21.1.267 is for Minecraft 1.21.1, not 1.20.4");
    }

    #[test]
    fn a_marker_with_a_missing_file_is_not_done() {
        let dir = tmp("done");
        let (marker, jar) = (dir.0.join("done"), dir.0.join("client.jar"));
        assert!(!installed(&marker, &[jar.clone()]));
        std::fs::write(&marker, "").unwrap();
        assert!(!installed(&marker, &[jar.clone()]));
        std::fs::write(&jar, "x").unwrap();
        assert!(installed(&marker, &[jar.clone()]));
        std::fs::remove_file(&marker).unwrap();
        assert!(!installed(&marker, &[jar]));
    }

    #[test]
    fn the_last_lines_of_output_are_kept() {
        let text = (1..=30).map(|n| format!("l{n}")).collect::<Vec<_>>().join("
");
        let tail = last_lines(&text, 20);
        assert!(tail.starts_with("l11
") && tail.ends_with("l30"));
    }
}
