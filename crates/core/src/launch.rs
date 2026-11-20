use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::version::{allowed, library_files, Argument, Arguments, Features, OneOrMany, VersionJson};
use crate::paths::check_relative;
use crate::{Error, Paths, Result};

pub const DEFAULT_MEMORY_MB: u32 = 4096;
const QUICK_PLAY: &str = "is_quick_play_multiplayer";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
}

impl Profile {
    pub fn offline(name: &str) -> Profile {
        let hash = Sha1::digest(format!("OfflinePlayer:{name}").as_bytes());
        Profile {
            name: name.into(),
            uuid: hash[..16].iter().map(|b| format!("{b:02x}")).collect(),
            access_token: "0".into(),
            user_type: "legacy".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub game_dir: PathBuf,
    pub profile: Profile,
    pub server: Option<String>,
    pub memory_mb: u32,
}

pub fn supported(release_time: &str) -> bool {
    release_time >= "2013-09-19T15:52:37"
}

pub fn jar_path(paths: &Paths, version: &VersionJson) -> PathBuf {
    let j = version.inherits_from.as_deref().unwrap_or(&version.id);
    paths.versions().join(j).join(format!("{j}.jar"))
}

pub fn classpath(paths: &Paths, version: &VersionJson) -> Result<Vec<PathBuf>> {
    let mut cp: Vec<PathBuf> = library_files(version)?
        .into_iter()
        .filter(|f| !f.native)
        .map(|f| paths.libraries().join(PathBuf::from(f.path)))
        .collect();
    cp.push(jar_path(paths, version));
    Ok(cp)
}

fn has_quick_play(args: &[Argument]) -> bool {
    args.iter().any(|a| match a {
        Argument::Ruled { rules, .. } => rules
            .iter()
            .any(|r| r.features.as_ref().is_some_and(|f| f.contains_key(QUICK_PLAY))),
        Argument::Plain(_) => false,
    })
}

fn fill(text: &str, vars: &BTreeMap<&str, String>) -> String {
    vars.iter().fold(text.to_string(), |s, (k, v)| s.replace(&format!("${{{k}}}"), v))
}

fn expand(args: &[Argument], features: &Features, vars: &BTreeMap<&str, String>, out: &mut Vec<String>) {
    for arg in args {
        match arg {
            Argument::Plain(s) => out.push(fill(s, vars)),
            Argument::Ruled { rules, value } if allowed(rules, features) => match value {
                OneOrMany::One(s) => out.push(fill(s, vars)),
                OneOrMany::Many(v) => out.extend(v.iter().map(|s| fill(s, vars))),
            },
            Argument::Ruled { .. } => {}
        }
    }
}

pub fn arguments(paths: &Paths, version: &VersionJson, opts: &LaunchOptions) -> Result<Vec<String>> {
    let legacy;
    let args = match (&version.arguments, &version.minecraft_arguments) {
        (Some(args), _) => args,
        (None, Some(text)) => {
            let plain = |s: &str| Argument::Plain(s.into());
            legacy = Arguments {
                jvm: ["-Djava.library.path=${natives_directory}", "-Dminecraft.launcher.brand=${launcher_name}",
                    "-Dminecraft.launcher.version=${launcher_version}", "-cp", "${classpath}"].map(plain).into(),
                game: text.split_whitespace().map(plain).collect(),
            };
            &legacy
        }
        _ => return Err(Error::Unsupported("version has no launch arguments".into())),
    };
    let cp = classpath(paths, version)?
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(";");
    let quick_play = opts.server.is_some() && has_quick_play(&args.game);
    let mut features = Features::new();
    if quick_play {
        features.insert(QUICK_PLAY.to_string());
    }
    let host = opts.server.clone().unwrap_or_default();
    let vars = BTreeMap::from([
        ("auth_player_name", opts.profile.name.clone()),
        ("version_name", jar_path(paths, version).file_stem().unwrap_or_default().to_string_lossy().into_owned()),
        ("game_directory", opts.game_dir.to_string_lossy().into_owned()),
        ("assets_root", paths.assets().to_string_lossy().into_owned()),
        ("assets_index_name", version.assets.clone().unwrap_or_default()),
        ("auth_uuid", opts.profile.uuid.clone()),
        ("auth_access_token", opts.profile.access_token.clone()),
        ("clientid", String::new()),
        ("auth_xuid", String::new()),
        ("user_type", opts.profile.user_type.clone()),
        ("version_type", "release".into()),
        ("natives_directory", paths.natives(&version.id).to_string_lossy().into_owned()),
        ("launcher_name", "spawnpoint-portal".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", cp),
        ("classpath_separator", ";".into()),
        ("library_directory", paths.libraries().to_string_lossy().into_owned()),
        ("quickPlayMultiplayer", format!("{host}:25565")),
        ("user_properties", "{}".into()),
        ("game_assets", paths.assets().join("virtual").join(version.assets.as_deref().unwrap_or_default()).to_string_lossy().into_owned()),
        ("auth_session", format!("token:{}:{}", opts.profile.access_token, opts.profile.uuid)),
    ]);

    let mut out = vec![format!("-Xmx{}M", opts.memory_mb)];
    if let Some(log) = version.logging.as_ref().and_then(|l| l.client.as_ref()) {
        check_relative(&log.file.id)?;
        let path = paths.assets().join("log_configs").join(&log.file.id);
        out.push(log.argument.replace("${path}", &path.to_string_lossy()));
    }
    expand(&args.jvm, &features, &vars, &mut out);
    out.push(version.main_class.clone());
    expand(&args.game, &features, &vars, &mut out);
    if let (Some(host), false) = (&opts.server, quick_play) {
        out.extend(["--server".into(), host.clone(), "--port".into(), "25565".into()]);
    }
    Ok(out)
}

pub fn extract_natives(paths: &Paths, version: &VersionJson) -> Result<()> {
    version.check_ids()?;
    let dest = paths.natives(&version.id);
    std::fs::create_dir_all(&dest)?;
    // ponytail: excludes of all libraries apply to every native jar; only META-INF/ occurs in practice.
    let excludes: Vec<&str> = version
        .libraries
        .iter()
        .filter_map(|l| l.extract.as_ref())
        .flat_map(|e| e.exclude.iter().map(String::as_str))
        .collect();
    for lib in library_files(version)?.into_iter().filter(|f| f.native) {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(paths.libraries().join(PathBuf::from(lib.path)))?)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(name) = entry.enclosed_name() else { continue };
            let text = name.to_string_lossy().replace('\\', "/");
            if entry.is_dir() || text.starts_with("META-INF/") || excludes.iter().any(|e| text.starts_with(e)) {
                continue;
            }
            let target = dest.join(&name);
            if std::fs::metadata(&target).is_ok_and(|m| m.len() == entry.size()) {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(&target)?)?;
        }
    }
    Ok(())
}

pub fn command(java: &Path, args: &[String], game_dir: &Path) -> Command {
    let mut c = Command::new(java);
    c.args(args).current_dir(game_dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn v(name: &str) -> VersionJson {
        let text = match name {
            "1.21.1" => include_str!("../tests/fixtures/version-1.21.1.json"),
            "1.16.5" => include_str!("../tests/fixtures/version-1.16.5.json"),
            "1.7.10" => include_str!("../tests/fixtures/version-1.7.10.json"),
            "1.6.4" => include_str!("../tests/fixtures/version-1.6.4.json"),
            _ => include_str!("../tests/fixtures/version-1.12.2.json"),
        };
        serde_json::from_str(text).unwrap()
    }

    fn paths() -> Paths {
        Paths { root: r"C:\data root".into() }
    }

    fn opts(server: Option<&str>) -> LaunchOptions {
        LaunchOptions {
            game_dir: r"C:\games\my instance".into(),
            profile: Profile::offline("Steve"),
            server: server.map(String::from),
            memory_mb: DEFAULT_MEMORY_MB,
        }
    }

    fn after<'a>(a: &'a [String], flag: &str) -> &'a str {
        let i = a.iter().position(|x| x == flag).unwrap();
        &a[i + 1]
    }

    #[test]
    fn vanilla_1_21_1() {
        let a = arguments(&paths(), &v("1.21.1"), &opts(None)).unwrap();
        assert_eq!(a[0], "-Xmx4096M");
        let main = a.iter().position(|x| x == "net.minecraft.client.main.Main").unwrap();
        let cp = a.iter().position(|x| x == "-cp").unwrap();
        assert!(cp < main);
        assert!(a[cp + 1].ends_with(r"meta\versions\1.21.1\1.21.1.jar") && a[cp + 1].contains(';'));
        assert_eq!(after(&a, "--username"), "Steve");
        assert_eq!(after(&a, "--gameDir"), r"C:\games\my instance");
        assert_eq!(after(&a, "--assetIndex"), v("1.21.1").assets.unwrap());
        assert!(!a.iter().any(|x| x.contains("${")));
        assert!(!a.iter().any(|x| x == "--demo" || x == "--width" || x.starts_with("--quickPlay")));
        assert!(!a.iter().any(|x| x == "-XstartOnFirstThread"));
        assert!(a.iter().any(|x| x.starts_with("-Dlog4j.configurationFile=")));
    }

    #[test]
    fn version_name_is_the_name_of_the_client_jar() {
        let mut version = v("1.21.1");
        version.id = "neoforge-1".into();
        version.inherits_from = Some("1.21.1".into());
        version.arguments.as_mut().unwrap().jvm.push(Argument::Plain("-DignoreList=${version_name}.jar".into()));
        let a = arguments(&paths(), &version, &opts(None)).unwrap();
        assert!(a.contains(&"-DignoreList=1.21.1.jar".to_string()));
    }

    #[test]
    fn neoforge_on_1_21_1_leaves_no_placeholder_and_ignores_the_client_jar() {
        let neoforge: VersionJson = serde_json::from_str(include_str!("../tests/fixtures/neoforge-version.json")).unwrap();
        let a = arguments(&paths(), &crate::version::merge(v("1.21.1"), neoforge), &opts(None)).unwrap();
        assert!(!a.iter().any(|x| x.contains("${")));
        assert!(a.iter().any(|x| x.starts_with("-DignoreList=") && x.ends_with("1.21.1.jar")));
    }

    #[test]
    fn vanilla_1_16_5_has_no_placeholders_left() {
        let a = arguments(&paths(), &v("1.16.5"), &opts(None)).unwrap();
        assert!(!a.iter().any(|x| x.contains("${")));
    }

    #[test]
    fn quick_play_from_1_20() {
        let a = arguments(&paths(), &v("1.21.1"), &opts(Some("mc.example.org"))).unwrap();
        assert_eq!(after(&a, "--quickPlayMultiplayer"), "mc.example.org:25565");
        assert!(!a.iter().any(|x| x == "--server"));
    }

    #[test]
    fn server_and_port_before_1_20() {
        let a = arguments(&paths(), &v("1.16.5"), &opts(Some("mc.example.org"))).unwrap();
        assert_eq!(after(&a, "--server"), "mc.example.org");
        assert_eq!(after(&a, "--port"), "25565");
    }

    #[test]
    fn legacy_1_12_2() {
        let a = arguments(&paths(), &v("1.12.2"), &opts(Some("mc.example.org"))).unwrap();
        assert_eq!(a[0], "-Xmx4096M");
        assert!(a.iter().any(|x| x.starts_with("-Djava.library.path=") && x.ends_with(r"meta\natives\1.12.2")));
        assert!(a.iter().any(|x| x.starts_with("-Dlog4j.configurationFile=")));
        let main = a.iter().position(|x| x == "net.minecraft.client.main.Main").unwrap();
        assert!(a.iter().position(|x| x == "-cp").unwrap() < main);
        assert_eq!(after(&a, "--username"), "Steve");
        assert_eq!(after(&a, "--gameDir"), r"C:\games\my instance");
        assert_eq!(after(&a, "--accessToken"), "0");
        assert_eq!(after(&a, "--server"), "mc.example.org");
        assert_eq!(after(&a, "--port"), "25565");
        assert!(!a.iter().any(|x| x.contains("${")));
    }

    #[test]
    fn legacy_1_7_10_user_properties() {
        let a = arguments(&paths(), &v("1.7.10"), &opts(None)).unwrap();
        assert_eq!(after(&a, "--userProperties"), "{}");
        assert!(!a.iter().any(|x| x.contains("${")));
    }

    #[test]
    fn legacy_1_6_4_uses_the_virtual_assets() {
        let a = arguments(&paths(), &v("1.6.4"), &opts(None)).unwrap();
        assert!(after(&a, "--assetsDir").ends_with(r"meta\assets\virtual\legacy"));
        assert!(after(&a, "--session").starts_with("token:0:"));
        assert!(!a.iter().any(|x| x.contains("${")));
    }

    #[test]
    fn a_version_with_no_arguments_at_all_is_refused() {
        let version: VersionJson = serde_json::from_str(r#"{"id":"x","mainClass":"m"}"#).unwrap();
        assert!(matches!(arguments(&paths(), &version, &opts(None)), Err(Error::Unsupported(_))));
    }

    #[test]
    fn cut_off_at_1_6_4() {
        assert!(supported("2013-09-19T15:52:37+00:00") && supported("2024-08-08T12:24:45+00:00"));
        assert!(!supported("2013-07-08T22:00:00+00:00"));
    }

    #[test]
    fn natives_of_an_escaping_version_id_are_refused() {
        let root = std::env::temp_dir().join(format!("portal-ids-{}", std::process::id()));
        let _guard = Tmp(root.clone());
        let paths = Paths { root: root.join("data") };
        let version: VersionJson = serde_json::from_str(r#"{"id":"../../../escaped","mainClass":"m"}"#).unwrap();
        assert!(extract_natives(&paths, &version).is_err());
        assert!(!root.join("escaped").exists() && !root.exists());
    }

    #[test]
    fn offline_profile_is_stable() {
        assert_eq!(Profile::offline("Steve").uuid, Profile::offline("Steve").uuid);
        assert_eq!(Profile::offline("Steve").uuid.len(), 32);
        assert_ne!(Profile::offline("Steve").uuid, Profile::offline("Alex").uuid);
    }

    #[test]
    fn natives_skip_meta_inf_and_escaping_entries() {
        let root = std::env::temp_dir().join(format!("portal-launch-{}", std::process::id()));
        let _guard = Tmp(root.clone());
        let paths = Paths { root: root.clone() };
        let version: VersionJson = serde_json::from_str(
            r#"{"id":"t","mainClass":"m","libraries":[{"name":"a:b:1","natives":{"windows":"natives-windows"},
            "downloads":{"classifiers":{"natives-windows":{"path":"n/n.jar","url":"u"}}}}]}"#,
        )
        .unwrap();
        let jar = paths.libraries().join("n").join("n.jar");
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        let mut w = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        for name in ["a.dll", "META-INF/x", "../evil.dll"] {
            w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"data").unwrap();
        }
        w.finish().unwrap();

        extract_natives(&paths, &version).unwrap();
        extract_natives(&paths, &version).unwrap();
        let dest = paths.natives("t");
        let names: Vec<_> = std::fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["a.dll"]);
        assert!(!dest.parent().unwrap().join("evil.dll").exists());
    }

    #[test]
    fn legacy_arguments_split_on_any_whitespace() {
        let mut version = v("1.12.2");
        version.minecraft_arguments = Some("--a  1 --b	2".into());
        let a = arguments(&paths(), &version, &opts(None)).unwrap();
        assert!(!a.iter().any(|x| x.is_empty()), "{a:?}");
        assert_eq!(after(&a, "--a"), "1");
        assert_eq!(after(&a, "--b"), "2");
    }
}
