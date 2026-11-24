// Horizon is Minecraft: Java Edition rewritten in Rust. Portal follows its guide for other
// launchers: docs/api/launch-guide.md in horizon-rust/horizon.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::download::{Download, Hash, Progress, ProgressTx, blocking, fetch, get_ok};
use crate::launch::Profile;
use crate::{Error, Paths, Result, gamefiles, java};

const RELEASE: &str = "https://github.com/horizon-rust/horizon/releases/download/nightly";
const ARCHIVE: &str = "horizon-windows-x64.zip";
// The one folder the archive unpacks to.
const FOLDER: &str = "horizon-windows-x64";
// Horizon's crates/horizon-release-sign/release-keys.txt.
const RELEASE_KEYS: [&str; 1] = ["MU4oiIQcCxkZvbwSjjXs7SySCuM6ywRQ3nGaJYZG0gw="];

// What a new instance shows; preparing asks the build itself which Minecraft it needs.
pub const MINECRAFT: &str = "26.3";
pub const STAGES: [&str; 5] = ["horizon", "java", "libraries", "assets", "prepare"];

fn builds(paths: &Paths) -> PathBuf {
    paths.root.join("horizon")
}

// The archive's SHA-256, once a release key signed the list. The signed text names the tag.
fn archive_sha256(sums: &str, signature: &str) -> Result<String> {
    let unsigned = || Error::Other("Horizon's download is not signed by Horizon".into());
    let signature = STANDARD.decode(signature.trim()).map_err(|_| unsigned())?;
    let signed = [b"horizon release nightly\n", sums.as_bytes()].concat();
    let by_key = |key: &&str| STANDARD.decode(key).is_ok_and(|k| UnparsedPublicKey::new(&ED25519, k).verify(&signed, &signature).is_ok());
    if !RELEASE_KEYS.iter().any(by_key) {
        return Err(unsigned());
    }
    sums.lines()
        .filter_map(|l| l.split_once("  "))
        .find(|(hash, name)| *name == ARCHIVE && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|(hash, _)| hash.to_string())
        .ok_or_else(|| Error::Other(format!("Horizon's download list has no {ARCHIVE}")))
}

fn installed(paths: &Paths) -> Option<PathBuf> {
    let dirs = std::fs::read_dir(builds(paths)).ok()?.flatten().map(|e| e.path().join(FOLDER));
    dirs.filter(|d| d.join(".installed").exists()).max_by_key(|d| d.join(".installed").metadata().and_then(|m| m.modified()).ok())
}

// ponytail: a build stays until its folder is deleted; add an update button when players need newer nightlies.
async fn build(client: &Client, paths: &Paths, progress: &ProgressTx) -> Result<PathBuf> {
    if let Some(dir) = installed(paths) {
        return Ok(dir);
    }
    let _ = progress.send(Progress { step: 0, steps: 0, stage: "horizon".into(), done: 0, total: 1 });
    let sums = get_ok(client, &format!("{RELEASE}/SHA256SUMS")).await?.text().await?;
    let signature = get_ok(client, &format!("{RELEASE}/SHA256SUMS.sig")).await?.text().await?;
    let sha256 = archive_sha256(&sums, &signature)?;
    let dir = builds(paths).join(&sha256);
    let archive = builds(paths).join(format!("{sha256}.zip"));
    fetch(client, &Download { url: format!("{RELEASE}/{ARCHIVE}"), path: archive.clone(), hash: Some(Hash::Sha256(sha256)), size: None, bearer: None }).await?;
    blocking(move || {
        zip::ZipArchive::new(std::fs::File::open(&archive)?)?.extract(&dir)?;
        let dir = dir.join(FOLDER);
        std::fs::write(dir.join(".installed"), b"")?;
        let _ = std::fs::remove_file(&archive);
        Ok(dir)
    })
    .await
}

fn tool(build: &Path, name: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(build.join(name));
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // No console window beside the game.
    #[cfg(windows)]
    c.creation_flags(0x0800_0000);
    c
}

// One stdout line of `horizon-prepare --progress json`: how far it is, or why it failed.
fn prepare_line(line: &str) -> Option<std::result::Result<u64, String>> {
    let v: Value = serde_json::from_str(line).ok()?;
    if let Some(e) = v["error"].as_str() {
        return Some(Err(e.to_string()));
    }
    v["fraction"].as_f64().map(|f| Ok((f.clamp(0.0, 1.0) * 100.0) as u64))
}

// Turns the player's Minecraft files into what the client reads. The first time takes minutes.
async fn prepare(client: &Client, paths: &Paths, build: &Path, progress: &ProgressTx) -> Result<PathBuf> {
    let resources = build.join("resources");
    if tool(build, "horizon-prepare.exe").arg("--check").arg("--output").arg(&resources).status().await?.success() {
        return Ok(resources);
    }
    let asked = tool(build, "horizon-prepare.exe").arg("--launch-api").stdout(Stdio::piped()).output().await?;
    let api: Value = serde_json::from_slice(&asked.stdout)?;
    let minecraft = api["minecraft"].as_str().ok_or_else(|| Error::Other("this Horizon build does not say which Minecraft it needs".into()))?;
    let version = gamefiles::version_json(client, paths, minecraft).await?;
    let server = version.downloads.as_ref().and_then(|d| d.server.clone()).ok_or_else(|| Error::Other(format!("Minecraft {minecraft} has no server download")))?;
    let component = version.java_version.as_ref().map_or("jre-legacy", |j| j.component.as_str());
    let javaw = java::ensure(client, paths, component, progress).await?;
    gamefiles::install(client, paths, &version, None, progress).await?;
    let folder = paths.versions().join(minecraft);
    let server_jar = folder.join("server.jar");
    fetch(client, &Download { url: server.url, path: server_jar.clone(), hash: server.sha1.map(Hash::Sha1), size: server.size, bearer: None }).await?;

    let _ = progress.send(Progress { step: 0, steps: 0, stage: "prepare".into(), done: 0, total: 100 });
    let mut run = tool(build, "horizon-prepare.exe");
    run.args(["--prepare", "--complete", "--progress", "json"]).arg("--output").arg(&resources);
    run.arg("--version-json").arg(folder.join(format!("{minecraft}.json"))).arg("--client-jar").arg(crate::launch::jar_path(paths, &version));
    run.arg("--libraries-dir").arg(paths.libraries()).arg("--assets-dir").arg(paths.assets()).arg("--server-jar").arg(server_jar);
    // javaw.exe is in the runtime's bin folder.
    run.arg("--java-home").arg(javaw.parent().and_then(Path::parent).unwrap_or(&javaw));
    let mut child = run.current_dir(build).stdout(Stdio::piped()).spawn()?;
    let mut lines = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
    let mut failure = None;
    while let Some(line) = lines.next_line().await? {
        match prepare_line(&line) {
            Some(Ok(done)) => drop(progress.send(Progress { step: 0, steps: 0, stage: "prepare".into(), done, total: 100 })),
            Some(Err(reason)) => failure = Some(reason),
            None => {}
        }
    }
    if !child.wait().await?.success() {
        return Err(Error::Other(format!("Horizon could not prepare the game files: {}", failure.unwrap_or_else(|| "horizon-prepare stopped".into()))));
    }
    Ok(resources)
}

// A content mod is off until horizon-mods.txt has its `mod.<id>:true` line.
pub fn enable_mod(game_dir: &Path, id: &str) -> Result<()> {
    let file = game_dir.join("horizon-mods.txt");
    let text = match std::fs::read_to_string(&file) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        read => read.unwrap_or_default(),
    };
    let key = format!("mod.{id}:");
    let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with(&key)).map(String::from).collect();
    lines.push(format!("{key}true"));
    std::fs::create_dir_all(game_dir)?;
    Ok(std::fs::write(file, lines.join("\n") + "\n")?)
}

// Horizon's session JSON (docs/api/launch.md); its optional xuid is left out.
fn session_json(profile: &Profile) -> String {
    json!({"name": profile.name, "uuid": profile.uuid, "access_token": profile.access_token, "user_type": profile.user_type}).to_string()
}

pub async fn launch(client: &Client, paths: &Paths, game_dir: &Path, profile: &Profile, server: Option<&str>, progress: &ProgressTx) -> Result<(std::process::Child, bool)> {
    // Only the Windows build of Horizon is fetched (ARCHIVE).
    if !cfg!(windows) {
        return Err(Error::Unsupported("Horizon runs on Windows only for now".into()));
    }
    let build = build(client, paths, progress).await?;
    let resources = prepare(client, paths, &build, progress).await?;
    let _ = progress.send(Progress { step: 0, steps: 0, stage: "starting".into(), done: 0, total: 0 });
    std::fs::create_dir_all(game_dir)?;
    let mut game = tool(&build, "horizon-client.exe").into_std();
    game.arg("--resources").arg(resources).arg("--game-dir").arg(game_dir).arg("--home").arg(game_dir);
    game.arg("--session-stdin").stdin(Stdio::piped()).current_dir(game_dir);
    if let Some(host) = server {
        game.args(["--quickPlayMultiplayer", host]);
    }
    let mut child = game.spawn()?;
    // The token never goes on the command line; dropping the pipe closes it, which the game waits for.
    child.stdin.take().expect("stdin is piped").write_all(session_json(profile).as_bytes())?;
    Ok((child, server.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &str = include_str!("../tests/fixtures/horizon-SHA256SUMS");
    const SIGNATURE: &str = include_str!("../tests/fixtures/horizon-SHA256SUMS.sig");

    #[test]
    fn a_signed_list_gives_the_windows_archive_and_a_changed_one_is_refused() {
        assert_eq!(archive_sha256(SUMS, SIGNATURE).unwrap(), "479c3b70c2a7f2b83b86489d6686d6ce5a9dad7334ac3349eb383c6d637adfe5");
        let swapped = SUMS.replace("479c3b70", "00000000");
        for (sums, signature) in [(swapped.as_str(), SIGNATURE), (SUMS, "AAAA"), (SUMS, "not base64"), (SUMS, "")] {
            assert!(archive_sha256(sums, signature).is_err());
        }
    }

    #[test]
    fn the_session_is_the_json_horizon_reads() {
        let profile = Profile { name: "Alex".into(), uuid: "069a79f444e94726a5befca90e38aaf5".into(), access_token: "tok".into(), user_type: "msa".into() };
        let want = json!({"name": "Alex", "uuid": "069a79f444e94726a5befca90e38aaf5", "access_token": "tok", "user_type": "msa"});
        assert_eq!(serde_json::from_str::<Value>(&session_json(&profile)).unwrap(), want);
    }

    #[test]
    fn prepare_lines_give_progress_or_the_reason_it_failed() {
        assert_eq!(prepare_line(r#"{"fraction": 0.42}"#), Some(Ok(42)));
        assert_eq!(prepare_line(r#"{"fraction": 7}"#), Some(Ok(100)));
        assert_eq!(prepare_line(r#"{"error": "no server jar"}"#), Some(Err("no server jar".into())));
        assert_eq!(prepare_line(r#"{"message": "reading"}"#), None);
        assert_eq!(prepare_line("not json"), None);
    }

    #[test]
    fn a_mod_is_turned_on_once_and_other_lines_stay() {
        let dir = crate::download::testing::tmp_dir();
        std::fs::write(dir.join("horizon-mods.txt"), "mod.other:false\nmod.lights:false\n").unwrap();
        enable_mod(&dir, "lights").unwrap();
        enable_mod(&dir, "lights").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("horizon-mods.txt")).unwrap(), "mod.other:false\nmod.lights:true\n");
        enable_mod(&dir.join("new"), "a").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("new/horizon-mods.txt")).unwrap(), "mod.a:true\n");
    }
}
