use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::download::{blocking, fetch, fetch_all, file_matches, get_ok, sha1_file, Download, Hash, Progress, ProgressTx};
use crate::loader::LoaderKind;
use crate::paths::{check_id, check_relative};
use crate::version::{library_files, maven_path, Library, LibraryFile, VersionJson};
use crate::{Error, Paths, Result};

const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/";
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
    let all = neoforge_builds(versions_json, minecraft)?;
    Ok(all.iter().find(|v| !v.contains("-beta")).unwrap_or(&all[0]).clone())
}

// Newest first; never empty.
pub fn neoforge_builds(versions_json: &str, minecraft: &str) -> Result<Vec<String>> {
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
        .rev()
        .filter(|v| v.starts_with(&prefix) && check_id(v).is_ok())
        .collect();
    match all.is_empty() {
        true => Err(Error::Other(format!("NeoForge has no build for Minecraft {minecraft}"))),
        false => Ok(all),
    }
}

// From the Maven metadata, newest first; never empty. 1.7.10-10.13.4.1614-1.7.10 is listed as 10.13.4.1614.
pub fn forge_builds(metadata_xml: &str, minecraft: &str) -> Result<Vec<String>> {
    let prefix = format!("{minecraft}-");
    let number = |v: &String| v.split('.').map(|p| p.parse().unwrap_or(0)).collect::<Vec<u32>>();
    let mut builds: Vec<String> = metadata_xml
        .split("<version>")
        .skip(1)
        .filter_map(|rest| rest.split("</version>").next()?.strip_prefix(&prefix)?.split('-').next())
        .filter(|v| check_id(v).is_ok())
        .map(String::from)
        .collect();
    builds.sort_by_key(|v| std::cmp::Reverse(number(v)));
    builds.dedup();
    match builds.is_empty() {
        true => Err(Error::Other(format!("Forge has no build for Minecraft {minecraft}"))),
        false => Ok(builds),
    }
}

pub async fn versions(client: &Client, kind: LoaderKind, minecraft: &str) -> Result<Vec<String>> {
    match kind {
        LoaderKind::Forge => {
            let xml = get_ok(client, &format!("{FORGE_MAVEN}net/minecraftforge/forge/maven-metadata.xml")).await?.text().await?;
            forge_builds(&xml, minecraft)
        }
        LoaderKind::NeoForge => neoforge_builds(&get_ok(client, NEOFORGE_VERSIONS).await?.text().await?, minecraft),
        LoaderKind::Fabric => Err(Error::Other("Fabric has no installer".into())),
    }
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
        LoaderKind::NeoForge if minecraft == "1.20.1" => format!(
            "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-{version}/forge-1.20.1-{version}-installer.jar"
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

// The metadata lists builds such as 1.7.2-10.12.2.1161-mc172 whose name the promotions do not give.
fn metadata_version(xml: &str, minecraft: &str, version: &str) -> Option<String> {
    let exact = format!("{minecraft}-{version}");
    let prefix = format!("{exact}-");
    xml.split("<version>")
        .skip(1)
        .filter_map(|rest| rest.split("</version>").next())
        .filter(|v| !v.contains(|c: char| matches!(c, '/' | '\\' | '?' | '#') || c.is_whitespace()))
        .find(|v| *v == exact || v.starts_with(&prefix))
        .map(String::from)
}

pub async fn installer(
    client: &Client,
    paths: &Paths,
    kind: LoaderKind,
    minecraft: &str,
    version: &str,
    maven: &str,
) -> Result<(String, PathBuf)> {
    if kind == LoaderKind::Fabric {
        return Err(Error::Other("Fabric has no installer".into()));
    }
    let name = display_name(kind);
    check_relative(minecraft)?;
    check_relative(version)?;
    let path = paths.installers().join(format!("{}-{minecraft}-{version}.jar", kind.as_str()));
    let mut item = Download { url: installer_url(kind, minecraft, version).replacen(FORGE_MAVEN, maven, 1), path: path.clone(), hash: None, size: None, bearer: None };
    let mut found = fetch(client, &item).await;
    if kind == LoaderKind::Forge && matches!(found, Err(Error::Status { status: 404, .. })) {
        let metadata = get_ok(client, &format!("{maven}net/minecraftforge/forge/maven-metadata.xml")).await;
        if let Some(entry) = metadata_version(&metadata?.text().await?, minecraft, version) {
            check_relative(&entry)?;
            item.url = format!("{maven}net/minecraftforge/forge/{entry}/forge-{entry}-installer.jar");
            found = fetch(client, &item).await;
        }
    }
    found.map_err(|e| match e {
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

pub enum Installer {
    Processors(InstallProfile),
    Legacy(LegacyProfile),
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyProfile {
    pub install: LegacyInstall,
    #[serde(rename = "versionInfo")]
    pub version: VersionJson,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyInstall {
    pub path: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub minecraft: String,
}

pub fn read_profile(json: &str) -> Result<Installer> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    if value.get("install").is_some() && value.get("versionInfo").is_some() {
        Ok(Installer::Legacy(serde_json::from_value(value)?))
    } else if value.get("processors").is_some() || value.get("spec").is_some() {
        Ok(Installer::Processors(serde_json::from_value(value)?))
    } else {
        Err(Error::Other("unknown installer format".into()))
    }
}

// What the game needs from a legacy profile; natives and the vanilla release time come from Minecraft's own JSON.
fn client_version(profile: &LegacyProfile) -> VersionJson {
    let mut version = profile.version.clone();
    version.libraries.retain(|l| l.client_req != Some(false) && l.natives.is_none());
    version.release_time = None;
    version.logging = None;
    version
}

fn https(url: &str) -> Result<String> {
    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("http://files.minecraftforge.net/maven/") {
        Ok(format!("{FORGE_MAVEN}{rest}"))
    } else if lower.starts_with("https://") {
        Ok(url.to_string())
    } else if lower.starts_with("http://") {
        Ok(format!("https://{}", &url["http://".len()..]))
    } else {
        Err(Error::Other(format!("library url {url:?} is not https")))
    }
}

pub fn legacy_libraries(profile: &LegacyProfile) -> Result<Vec<LibraryFile>> {
    let mut version = client_version(profile);
    version.libraries.retain(|l| l.name != profile.install.path);
    let mut files = library_files(&version)?;
    for f in &mut files {
        f.url = https(&f.url)?;
    }
    Ok(files)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

type Archive = zip::ZipArchive<std::fs::File>;

fn read_json<T: serde::de::DeserializeOwned>(zip: &mut Archive, name: &str) -> Result<T> {
    Ok(serde_json::from_reader(zip.by_name(name)?)?)
}

// Copies unless dest already matches; Ok(false) when the installer has no such entry.
fn copy_out(zip: &mut Archive, name: &str, dest: &Path, sha1: Option<&str>) -> Result<bool> {
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

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

async fn run_step(java: &Path, step: &Step, cwd: &Path, loader: &str, number: usize) -> Result<()> {
    let classpath = std::iter::once(&step.jar).chain(&step.classpath).map(|p| text(p)).collect::<Vec<_>>().join(";");
    let jar = step.jar.clone();
    let main = blocking(move || main_class(&jar)).await?;
    let mut command = tokio::process::Command::new(console_java(java));
    command.arg("-cp").arg(classpath).arg(main).args(&step.args).current_dir(cwd);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let out = command.output().await?;
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

fn folders(paths: &Paths, kind: LoaderKind, minecraft: &str, version: &str, profile: &InstallProfile) -> (PathBuf, PathBuf, PathBuf) {
    (
        paths.libraries(),
        paths.installers().join(format!("{}-{minecraft}-{version}", kind.as_str())),
        paths.versions().join(&profile.minecraft).join(format!("{}.jar", profile.minecraft)),
    )
}

const MIRRORS: [&str; 2] = ["https://maven.minecraftforge.net/", "https://libraries.minecraft.net/"];

// Old Scala libraries exist on some mavens only as .pack.xz, which this launcher cannot unpack.
async fn fetch_libraries(client: &Client, libraries: &Path, files: Vec<LibraryFile>, mirrors: &[&str], what: &str, progress: &ProgressTx) -> Result<()> {
    let items = files
        .into_iter()
        .map(|f| Ok(Download { url: f.url, path: under(libraries, &f.path)?, hash: None, size: None, bearer: None }))
        .collect::<Result<Vec<_>>>()?;
    loop {
        let url = match fetch_all(client, "loader libraries", items.clone(), progress).await {
            Err(Error::Status { url, status: 404 }) => url,
            result => return result,
        };
        let item = items.iter().find(|i| i.url == url).ok_or(Error::Status { url, status: 404 })?;
        let relative = item.path.strip_prefix(libraries).unwrap_or(&item.path).to_string_lossy().replace("\\", "/");
        let mut found = false;
        for mirror in mirrors {
            match fetch(client, &Download { url: format!("{mirror}{relative}"), ..item.clone() }).await {
                Ok(()) => {
                    found = true;
                    break;
                }
                Err(Error::Status { status: 404, .. }) => {}
                Err(e) => return Err(e),
            }
        }
        if !found {
            let name = item.path.file_name().unwrap_or_default().to_string_lossy();
            return Err(Error::Unsupported(format!("{what} needs a library this launcher cannot unpack: {name}")));
        }
    }
}

async fn install_legacy(
    client: &Client,
    paths: &Paths,
    minecraft: &str,
    version: &str,
    jar: &Path,
    profile: LegacyProfile,
    progress: &ProgressTx,
) -> Result<VersionJson> {
    check_minecraft("Forge", version, &profile.install.minecraft, minecraft)?;
    let loader_json = client_version(&profile);
    loader_json.check_ids()?;
    check_relative(&profile.install.file_path)?;
    let forge = under(&paths.libraries(), &maven_path(&profile.install.path)?)?;
    let file = paths.versions().join(&loader_json.id).join(format!("{}.json", loader_json.id));
    let (own_jar, own_profile, own_json) = (jar.to_path_buf(), profile.clone(), loader_json.clone());
    blocking(move || {
        let mut zip: Archive = zip::ZipArchive::new(std::fs::File::open(own_jar)?)?;
        if !copy_out(&mut zip, &own_profile.install.file_path, &forge, None)? {
            return Err(Error::Other(format!("the installer has no {}", own_profile.install.file_path)));
        }
        std::fs::create_dir_all(file.parent().unwrap_or(Path::new("")))?;
        std::fs::write(&file, serde_json::to_vec(&own_json)?)?;
        Ok(())
    })
    .await?;
    let what = format!("Forge {version} for Minecraft {minecraft}");
    fetch_libraries(client, &paths.libraries(), legacy_libraries(&profile)?, &MIRRORS, &what, progress).await?;
    let _ = progress.send(Progress { step: 0, steps: 0, stage: "loader".into(), done: 1, total: 1 });
    Ok(loader_json)
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
    let (version, jar) = installer(client, paths, kind, minecraft, version, FORGE_MAVEN).await?;
    let own_jar = jar.clone();
    let text = blocking(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut zip::ZipArchive::new(std::fs::File::open(own_jar)?)?.by_name("install_profile.json")?, &mut text)?;
        Ok(text)
    })
    .await?;
    let profile = match read_profile(&text)? {
        Installer::Legacy(legacy) => return install_legacy(client, paths, minecraft, &version, &jar, legacy, progress).await,
        Installer::Processors(profile) => profile,
    };
    let (own_paths, own_jar, own_minecraft, own_version) = (paths.clone(), jar.clone(), minecraft.to_string(), version.clone());
    let (profile, loader_json, steps, downloads) = blocking(move || {
        let mut zip: Archive = zip::ZipArchive::new(std::fs::File::open(&own_jar)?)?;
        let loader_json: VersionJson = read_json(&mut zip, "version.json")?;
        check_minecraft(display_name(kind), &own_version, &profile.minecraft, &own_minecraft)?;
        loader_json.check_ids()?;
        let file = own_paths.versions().join(&loader_json.id).join(format!("{}.json", loader_json.id));
        std::fs::create_dir_all(file.parent().unwrap_or(&own_paths.versions()))?;
        std::fs::write(&file, serde_json::to_vec(&loader_json)?)?;

        let (libraries, extracted, minecraft_jar) = folders(&own_paths, kind, &own_minecraft, &own_version, &profile);
        let ctx = Context { libraries: &libraries, minecraft_jar: &minecraft_jar, installer: &own_jar, extracted: &extracted };
        let steps = steps(&profile, &ctx)?;
        let mut downloads = Vec::new();
        for lib in installer_libraries(&profile, &loader_json)? {
            let dest = under(&libraries, &lib.path)?;
            if !lib.url.is_empty() {
                downloads.push(Download { url: lib.url, path: dest, hash: lib.sha1.map(Hash::Sha1), size: lib.size, bearer: None });
            } else {
                copy_out(&mut zip, &format!("maven/{}", lib.path), &dest, lib.sha1.as_deref())?;
            }
        }
        Ok((profile, loader_json, steps, downloads))
    })
    .await?;
    let (libraries, extracted, minecraft_jar) = folders(paths, kind, minecraft, &version, &profile);
    let ctx = Context { libraries: &libraries, minecraft_jar: &minecraft_jar, installer: &jar, extracted: &extracted };
    // Most NeoForge processors declare no outputs, so a marker records that all of them ran.
    let marker = extracted.join("done");
    let produced = profile
        .data
        .values()
        .filter_map(|e| e.client.strip_prefix('[')?.strip_suffix(']'))
        .map(|coordinate| library(&ctx, coordinate))
        .collect::<Result<Vec<_>>>()?;
    if installed(&marker, &produced) || steps.iter().all(is_done) {
        let n = steps.len() as u64;
        let send = |stage: &str, done, total| progress.send(Progress { step: 0, steps: 0, stage: stage.into(), done, total });
        let _ = send("loader libraries", 0, 0);
        let _ = send("loader", n, n);
        return Ok(loader_json);
    }
    fetch_all(client, "loader libraries", downloads, progress).await?;

    let inner_names: Vec<String> = profile.data.values().filter_map(|e| e.client.strip_prefix('/')).map(String::from).collect();
    let (own_jar, own_extracted) = (jar.clone(), extracted.clone());
    blocking(move || {
        let mut zip: Archive = zip::ZipArchive::new(std::fs::File::open(&own_jar)?)?;
        for inner in &inner_names {
            if !copy_out(&mut zip, inner, &under(&own_extracted, inner)?, None)? {
                return Err(Error::Other(format!("the installer has no {inner}")));
            }
        }
        Ok(())
    })
    .await?;

    let meta = paths.root.join("meta");
    let total = steps.len() as u64;
    for (i, step) in steps.iter().enumerate() {
        if !is_done(step) {
            run_step(java, step, &meta, kind.as_str(), i + 1).await?;
        }
        let _ = progress.send(Progress { step: 0, steps: 0, stage: "loader".into(), done: i as u64 + 1, total });
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
    fn builds_are_listed_newest_first() {
        let xml = "<versions><version>1.7.10-10.13.4.1558-1.7.10</version><version>1.7.10-10.13.4.1614-1.7.10</version>\
            <version>1.7.10-10.13.4.1614-new</version><version>1.7.10_pre4-10.12.2.1149-prerelease</version>\
            <version>1.7.10-10.13.4.999</version><version>1.7.10-a/b</version><version>1.20.1-47.4.28</version></versions>";
        assert_eq!(forge_builds(xml, "1.7.10").unwrap(), ["10.13.4.1614", "10.13.4.1558", "10.13.4.999"]);
        assert_eq!(forge_builds(xml, "1.20.1").unwrap(), ["47.4.28"]);
        assert!(forge_builds(xml, "1.7").is_err());
        let j = include_str!("../../tests/fixtures/neoforge-versions.json");
        let listed = neoforge_builds(j, "1.21.1").unwrap();
        assert_eq!(listed[0], "21.1.267");
        assert!(listed.len() > 1 && listed.iter().all(|v| v.starts_with("21.1.")));
    }

    #[test]
    fn installer_url_with_the_minecraft_suffix() {
        assert_eq!(installer_url(LoaderKind::Forge, "1.7.10", "10.13.4.1614-1.7.10"), "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-installer.jar");
    }

    #[test]
    fn installer_urls() {
        assert_eq!(installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.90"), "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.90/neoforge-21.1.90-installer.jar");
        assert_eq!(installer_url(LoaderKind::NeoForge, "1.20.1", "47.1.84"), "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.84/forge-1.20.1-47.1.84-installer.jar");
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

    fn installer_with(dir: &Path, entry: &str, body: &[u8]) -> Archive {
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

    const OLD_17: &str = include_str!("../../tests/fixtures/forge-1.7.10-install_profile.json");
    const OLD_112: &str = include_str!("../../tests/fixtures/forge-1.12.2-old-install_profile.json");
    const OLD_16: &str = include_str!("../../tests/fixtures/forge-1.6.4-install_profile.json");

    fn legacy(json: &str) -> LegacyProfile {
        let Installer::Legacy(p) = read_profile(json).unwrap() else { panic!() };
        p
    }

    #[test]
    fn the_installer_is_read_by_its_content() {
        for json in [OLD_17, OLD_112, OLD_16] {
            assert!(matches!(read_profile(json).unwrap(), Installer::Legacy(_)));
        }
        assert!(matches!(read_profile(include_str!("../../tests/fixtures/neoforge-install_profile.json")).unwrap(), Installer::Processors(_)));
        assert!(matches!(read_profile(include_str!("../../tests/fixtures/forge-1.16.5-install_profile.json")).unwrap(), Installer::Processors(_)));
        assert!(read_profile("{}").is_err());
    }

    #[test]
    fn legacy_libraries_skip_forge_itself_and_server_only() {
        let p = legacy(OLD_17);
        let libs = legacy_libraries(&p).unwrap();
        assert!(!libs.iter().any(|l| l.path.contains("minecraftforge/forge/")));
        assert!(libs.iter().any(|l| l.path.contains("launchwrapper") && l.url.starts_with("https://libraries.minecraft.net/")));
        assert!(libs.iter().all(|l| l.url.starts_with("https://")));
        assert_eq!(p.version.main_class, "net.minecraft.launchwrapper.Launch");
        let new = legacy_libraries(&legacy(OLD_112)).unwrap();
        assert!(!new.iter().any(|l| l.path.contains("jline") || l.path.contains("/jna/")));
        assert!(new.iter().any(|l| l.path.contains("trove4j")));
        let oldest = legacy_libraries(&legacy(OLD_16)).unwrap();
        assert!(!oldest.iter().any(|l| l.path.contains("lwjgl-platform") || l.path.contains("nightly") || l.native));
    }

    #[test]
    fn old_hosts_are_rewritten_to_https() {
        let mut p = legacy(OLD_17);
        p.version.libraries = serde_json::from_value(serde_json::json!([
            { "name": "a:b:1", "url": "http://files.minecraftforge.net/maven/" },
            { "name": "a:c:1", "url": "http://example.com/m/" }
        ]))
        .unwrap();
        let urls: Vec<String> = legacy_libraries(&p).unwrap().into_iter().map(|l| l.url).collect();
        assert_eq!(urls, ["https://maven.minecraftforge.net/a/b/1/b-1.jar", "https://example.com/m/a/c/1/c-1.jar"]);
    }

    #[test]
    fn a_legacy_profile_merges_with_its_minecraft() {
        let cases = [
            (OLD_16, include_str!("../../tests/fixtures/version-1.6.4.json")),
            (OLD_17, include_str!("../../tests/fixtures/version-1.7.10.json")),
            (OLD_112, include_str!("../../tests/fixtures/version-1.12.2.json")),
        ];
        let natives = |v: &VersionJson| library_files(v).unwrap().iter().filter(|f| f.native).count();
        for (profile, parent) in cases {
            let parent: VersionJson = serde_json::from_str(parent).unwrap();
            let merged = crate::version::merge(parent.clone(), client_version(&legacy(profile)));
            let id = &parent.id;
            assert_eq!(merged.main_class, "net.minecraft.launchwrapper.Launch", "{id}");
            assert_eq!(merged.release_time, parent.release_time, "{id}");
            assert!(crate::launch::supported(merged.kind.as_deref().unwrap()), "{id}");
            assert!(merged.minecraft_arguments.as_deref().unwrap().contains("--tweakClass"), "{id}");
            assert!(merged.downloads.is_some() && merged.asset_index.is_some(), "{id}");
            assert_eq!(natives(&merged), natives(&parent), "{id}");
            assert!(library_files(&merged).unwrap().iter().any(|f| f.path.contains("minecraftforge/")), "{id}");
        }
    }

    fn lib_file(url: String) -> LibraryFile {
        LibraryFile { path: "a/b/1/b-1.jar".into(), url, sha1: None, size: None, native: false }
    }

    #[tokio::test]
    async fn a_library_missing_from_its_maven_is_looked_for_on_the_others() {
        use crate::download::testing::{serve_routes, tmp_dir};
        let dir = tmp_dir();
        let (nothing, _) = crate::download::testing::serve(404, b"").await;
        let (other, _, _) = serve_routes(vec![("/a/b/1/b-1.jar", 200, b"abc".to_vec(), String::new())]).await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let client = crate::http_client();
        let file = || vec![lib_file(format!("{nothing}/a/b/1/b-1.jar"))];

        fetch_libraries(&client, &dir, file(), &[&format!("{nothing}/"), &format!("{other}/")], "Forge 1 for Minecraft 1.7.10", &tx).await.unwrap();
        assert_eq!(std::fs::read(dir.join("a/b/1/b-1.jar")).unwrap(), b"abc");

        std::fs::remove_file(dir.join("a/b/1/b-1.jar")).unwrap();
        let err = fetch_libraries(&client, &dir, file(), &[&format!("{nothing}/")], "Forge 1 for Minecraft 1.7.10", &tx).await.unwrap_err();
        assert!(matches!(&err, Error::Unsupported(m) if m == "Forge 1 for Minecraft 1.7.10 needs a library this launcher cannot unpack: b-1.jar"), "{err}");
    }

    #[tokio::test]
    async fn the_legacy_install_is_not_redone_and_is_redone_when_files_are_missing() {
        let dir = tmp("legacy");
        let profile = r#"{"install":{"path":"a:forge:1","filePath":"forge-universal.jar","minecraft":"1.7.10"},
            "versionInfo":{"id":"1.7.10-Forge1","mainClass":"m","releaseTime":"1960-01-01T00:00:00-0700","libraries":[{"name":"a:forge:1"}]}}"#;
        let jar = dir.0.join("installer.jar");
        installer_with(&dir.0, "forge-universal.jar", b"abc");
        let paths = Paths { root: dir.0.join("data") };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let client = crate::http_client();
        let run = || install_legacy(&client, &paths, "1.7.10", "1", &jar, legacy(profile), &tx);

        let version = run().await.unwrap();
        assert_eq!(version.release_time, None);
        let forge = paths.libraries().join("a/forge/1/forge-1.jar");
        assert_eq!(std::fs::read(&forge).unwrap(), b"abc");
        assert!(paths.versions().join("1.7.10-Forge1/1.7.10-Forge1.json").exists());

        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000);
        std::fs::File::options().write(true).open(&forge).unwrap().set_modified(old).unwrap();
        run().await.unwrap();
        assert_eq!(std::fs::metadata(&forge).unwrap().modified().unwrap(), old);

        std::fs::remove_file(&forge).unwrap();
        run().await.unwrap();
        assert_eq!(std::fs::read(&forge).unwrap(), b"abc");
    }

    #[test]
    fn the_legacy_installer_must_be_for_the_requested_minecraft() {
        let err = check_minecraft("Forge", "10.13.4.1614", &legacy(OLD_17).install.minecraft, "1.12.2").unwrap_err();
        assert_eq!(err.to_string(), "Forge 10.13.4.1614 is for Minecraft 1.7.10, not 1.12.2");
    }

    #[test]
    fn the_metadata_scan_matches_whole_versions_only() {
        let xml = "<versioning><versions><version>1.7.2-10.12.2.11610</version><version>1.7.2-10.12.2.1161-mc172</version>            <version>1.10-12.18.0.2000-1.10.0</version><version>1.7.10-10.13.4.1614-1.7.10</version><version>1.8-11.14.0.1299</version></versions></versioning>";
        assert_eq!(metadata_version(xml, "1.7.2", "10.12.2.1161").as_deref(), Some("1.7.2-10.12.2.1161-mc172"));
        assert_eq!(metadata_version(xml, "1.10", "12.18.0.2000").as_deref(), Some("1.10-12.18.0.2000-1.10.0"));
        assert_eq!(metadata_version(xml, "1.7.10", "10.13.4.1614").as_deref(), Some("1.7.10-10.13.4.1614-1.7.10"));
        assert_eq!(metadata_version(xml, "1.8", "11.14.0.1299").as_deref(), Some("1.8-11.14.0.1299"));
        assert_eq!(metadata_version(xml, "1.7.2", "10.12.2.116"), None);
        for bad in ["1.7.2-1.0/../x", "1.7.2-1.0\\x", "1.7.2-1.0?x", "1.7.2-1.0#x", "1.7.2-1.0 x"] {
            assert_eq!(metadata_version(&format!("<version>{bad}</version>"), "1.7.2", "1.0"), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn an_installer_published_under_another_name_is_found_in_the_metadata() {
        use crate::download::testing::{serve_routes, tmp_dir};
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("data") };
        let xml = b"<version>1.7.2-10.12.2.11610</version><version>1.7.2-10.12.2.1161-mc172</version>".to_vec();
        let jar = "1.7.2-10.12.2.1161-mc172/forge-1.7.2-10.12.2.1161-mc172-installer.jar";
        let (base, _, _) = serve_routes(vec![
            ("/net/minecraftforge/forge/maven-metadata.xml", 200, xml, String::new()),
            (jar, 200, b"jar".to_vec(), String::new()),
        ])
        .await;
        let maven = format!("{base}/");
        let (version, path) = installer(&crate::http_client(), &paths, LoaderKind::Forge, "1.7.2", "10.12.2.1161", &maven).await.unwrap();
        assert_eq!(version, "10.12.2.1161");
        assert_eq!(std::fs::read(path).unwrap(), b"jar");

        let err = installer(&crate::http_client(), &paths, LoaderKind::Forge, "1.7.2", "9.9.9.9", &maven).await.unwrap_err();
        assert_eq!(err.to_string(), "Forge 9.9.9.9 does not exist for Minecraft 1.7.2");
    }

    #[test]
    fn a_legacy_profile_never_replaces_the_vanilla_logging() {
        let parent: VersionJson = serde_json::from_str(include_str!("../../tests/fixtures/version-1.12.2.json")).unwrap();
        let mut profile = legacy(OLD_112);
        assert!(profile.version.logging.is_some());
        let merged = crate::version::merge(parent.clone(), client_version(&profile));
        assert_eq!(merged.logging.as_ref().unwrap().client.as_ref().unwrap().argument, parent.logging.as_ref().unwrap().client.as_ref().unwrap().argument);
        profile.version.logging = parent.logging.clone();
        assert!(client_version(&profile).logging.is_none());
        let paths = Paths { root: r"C:\data root".into() };
        let opts = crate::launch::LaunchOptions { game_dir: r"C:\g".into(), profile: crate::launch::Profile::offline("Steve"), server: None, memory_mb: 2048 };
        let args = crate::launch::arguments(&paths, &merged, &opts).unwrap();
        assert!(args.iter().any(|a| a.starts_with("-Dlog4j.configurationFile")), "{args:?}");
    }

    #[test]
    fn only_https_library_urls_are_accepted() {
        let with_url = |url: &str| {
            let mut p = legacy(OLD_17);
            p.version.libraries = serde_json::from_value(serde_json::json!([{ "name": "a:b:1", "url": url }])).unwrap();
            legacy_libraries(&p)
        };
        assert_eq!(with_url("HTTP://example.com/m/").unwrap()[0].url, "https://example.com/m/a/b/1/b-1.jar");
        assert_eq!(with_url("HTTP://files.minecraftforge.net/maven/").unwrap()[0].url, "https://maven.minecraftforge.net/a/b/1/b-1.jar");
        assert!(with_url("ftp://example.com/m/").is_err());
        assert!(with_url("//example.com/m/").is_err());
    }

    #[tokio::test]
    async fn only_a_404_everywhere_means_cannot_unpack() {
        use crate::download::testing::{serve, tmp_dir};
        let dir = tmp_dir();
        let (missing, _) = serve(404, b"").await;
        let (broken, _) = serve(500, b"").await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let client = crate::http_client();
        let file = vec![lib_file(format!("{missing}/a/b/1/b-1.jar"))];
        let err = fetch_libraries(&client, &dir, file, &[&format!("{missing}/"), &format!("{broken}/")], "Forge", &tx).await.unwrap_err();
        assert!(matches!(err, Error::Status { status: 500, .. }), "{err}");
    }
}
