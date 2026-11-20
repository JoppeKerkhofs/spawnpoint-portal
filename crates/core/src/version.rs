use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::paths::check_relative;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: String,
    pub arguments: Option<Arguments>,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    pub minecraft_arguments: Option<String>,
    pub release_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Ruled { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<BTreeMap<String, String>>,
    pub extract: Option<Extract>,
    #[serde(rename = "clientreq")]
    pub client_req: Option<bool>,
    #[serde(rename = "serverreq")]
    pub server_req: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<BTreeMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Downloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

impl VersionJson {
    pub fn check_ids(&self) -> Result<()> {
        check_relative(&self.id)?;
        self.inherits_from.as_deref().map_or(Ok(()), check_relative)
    }
}

pub type Features = BTreeSet<String>;

pub fn allowed(rules: &[Rule], features: &Features) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut result = false;
    for rule in rules {
        let os_ok = rule.os.as_ref().is_none_or(|os| {
            os.name.as_deref().is_none_or(|n| n == "windows")
                && os.arch.as_deref().is_none_or(|a| a == "x86_64")
        });
        let features_ok = rule.features.as_ref().is_none_or(|f| {
            f.iter().all(|(name, want)| features.contains(name) == *want)
        });
        if os_ok && features_ok {
            result = rule.action == "allow";
        }
    }
    result
}

pub fn maven_path(coordinate: &str) -> Result<String> {
    let (coords, ext) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    let parts: Vec<&str> = coords.split(':').collect();
    let (group, artifact, version, classifier) = match parts[..] {
        [g, a, v] => (g, a, v, None),
        [g, a, v, c] => (g, a, v, Some(c)),
        _ => return Err(Error::Other(format!("not a maven coordinate: {coordinate}"))),
    };
    let classifier = classifier.map(|c| format!("-{c}")).unwrap_or_default();
    Ok(format!(
        "{}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}",
        group.replace('.', "/")
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryFile {
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub native: bool,
}

fn from_artifact(a: &Artifact, fallback_path: impl FnOnce() -> Result<String>, native: bool) -> Result<LibraryFile> {
    Ok(LibraryFile {
        path: match &a.path {
            Some(p) => p.clone(),
            None => fallback_path()?,
        },
        url: a.url.clone(),
        sha1: a.sha1.clone(),
        size: a.size,
        native,
    })
}

pub fn library_files(version: &VersionJson) -> Result<Vec<LibraryFile>> {
    // Every caller builds paths from the ids and from these files, so one check here covers them all.
    version.check_ids()?;
    let no_features = Features::new();
    let mut files = Vec::new();
    for lib in &version.libraries {
        // Mojang's windows arm64/x86 natives carry no arch rule, only the name tells.
        let other_arch = lib.name.ends_with("-arm64") || lib.name.ends_with("-x86");
        if other_arch || !allowed(lib.rules.as_deref().unwrap_or(&[]), &no_features) {
            continue;
        }
        let Some(downloads) = &lib.downloads else {
            let base = lib.url.as_deref().unwrap_or("https://libraries.minecraft.net/");
            let path = maven_path(&lib.name)?;
            files.push(LibraryFile {
                url: format!("{base}{path}"),
                path,
                sha1: lib.sha1.clone(),
                size: None,
                native: false,
            });
            continue;
        };
        if let Some(a) = &downloads.artifact {
            files.push(from_artifact(a, || maven_path(&lib.name), false)?);
        }
        if let Some(classifier) = lib.natives.as_ref().and_then(|n| n.get("windows")) {
            let classifier = classifier.replace("${arch}", "64");
            if let Some(a) = downloads.classifiers.as_ref().and_then(|c| c.get(&classifier)) {
                files.push(from_artifact(a, || maven_path(&format!("{}:{classifier}", lib.name)), true)?);
            }
        }
    }
    files.iter().try_for_each(|f| check_relative(&f.path))?;
    Ok(files)
}

fn library_key(name: &str) -> String {
    let mut parts = name.split(':');
    let (group, artifact) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let classifier = parts.nth(1).unwrap_or("");
    format!("{group}:{artifact}:{classifier}")
}

pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let own: BTreeSet<String> = child.libraries.iter().map(|l| library_key(&l.name)).collect();
    let mut libraries = child.libraries;
    libraries.extend(parent.libraries.into_iter().filter(|l| !own.contains(&library_key(&l.name))));
    let arguments = match (parent.arguments, child.arguments) {
        (Some(mut p), Some(c)) => {
            p.game.extend(c.game);
            p.jvm.extend(c.jvm);
            Some(p)
        }
        (p, c) => c.or(p),
    };
    VersionJson {
        id: child.id,
        inherits_from: Some(parent.id),
        main_class: child.main_class,
        arguments,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        release_time: child.release_time.or(parent.release_time),
        libraries,
        java_version: child.java_version.or(parent.java_version),
        // An empty `logging: {}` must not hide the parent's log4j configuration.
        logging: child.logging.filter(|l| l.client.is_some()).or(parent.logging),
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> VersionJson {
        let text = match name {
            "1.21.1" => include_str!("../tests/fixtures/version-1.21.1.json"),
            "1.16.5" => include_str!("../tests/fixtures/version-1.16.5.json"),
            _ => include_str!("../tests/fixtures/version-1.12.2.json"),
        };
        serde_json::from_str(text).unwrap()
    }

    fn fixture(name: &str) -> VersionJson {
        let text = match name {
            "fabric-profile-1.21.1.json" => include_str!("../tests/fixtures/fabric-profile-1.21.1.json"),
            _ => unreachable!(),
        };
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn old_versions_pick_only_the_windows_natives() {
        let natives = |text: &str| -> Vec<String> {
            let version: VersionJson = serde_json::from_str(text).unwrap();
            library_files(&version).unwrap().into_iter().filter(|f| f.native).map(|f| f.path).collect()
        };
        let n = natives(include_str!("../tests/fixtures/version-1.7.10.json"));
        assert_eq!(n.len(), 4);
        assert!(n.iter().all(|p| p.ends_with("natives-windows.jar") || p.ends_with("natives-windows-64.jar")), "{n:?}");
        let n = natives(include_str!("../tests/fixtures/version-1.6.4.json"));
        assert_eq!(n.len(), 2);
        assert!(n.iter().all(|p| p.ends_with("natives-windows.jar")), "{n:?}");
    }

    #[test]
    fn fabric_over_vanilla() {
        let profile = fixture("fabric-profile-1.21.1.json");
        let m = merge(v("1.21.1"), profile.clone());
        assert_eq!(m.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(m.inherits_from.as_deref(), Some("1.21.1"));
        assert!(m.id.starts_with("fabric-loader-"));
        assert!(m.downloads.is_some() && m.asset_index.is_some() && m.java_version.is_some());
        let asm: Vec<_> = m.libraries.iter().filter(|l| l.name.starts_with("org.ow2.asm:asm:")).collect();
        assert_eq!(asm.len(), 1);
        assert!(m.libraries[0].name.contains("fabric") || m.libraries[0].url.is_some());
        let first_vanilla_only = m
            .libraries
            .iter()
            .position(|l| !profile.libraries.iter().any(|p| p.name == l.name))
            .unwrap();
        assert_eq!(first_vanilla_only, profile.libraries.len());
        let files = library_files(&m).unwrap();
        assert!(files.iter().any(|f| f.url.starts_with("https://maven.fabricmc.net/") && f.path.contains("fabric-loader")));
    }

    #[test]
    fn merge_rules() {
        let parent = v("1.21.1");
        let child: VersionJson = serde_json::from_str(
            r#"{"id":"c","mainClass":"m","libraries":[{"name":"a:b:2"},{"name":"a:b:2:nat"}],
            "arguments":{"game":["--x"],"jvm":["-y"]}}"#,
        )
        .unwrap();
        let mut parent = parent;
        parent.libraries.push(serde_json::from_str(r#"{"name":"a:b:1"}"#).unwrap());
        parent.libraries.push(serde_json::from_str(r#"{"name":"a:b:1:other"}"#).unwrap());
        let (pg, pj) = parent.arguments.as_ref().map(|a| (a.game.len(), a.jvm.len())).unwrap();
        let m = merge(parent, child);
        let names: Vec<_> = m.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(&names[..2], ["a:b:2", "a:b:2:nat"]);
        assert!(!names.contains(&"a:b:1") && names.contains(&"a:b:1:other"));
        let a = m.arguments.unwrap();
        assert_eq!((a.game.len(), a.jvm.len()), (pg + 1, pj + 1));
        assert!(matches!(a.game.last(), Some(Argument::Plain(s)) if s == "--x"));
        assert!(matches!(a.jvm.last(), Some(Argument::Plain(s)) if s == "-y"));
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn parses_the_three_versions() {
        assert_eq!(v("1.21.1").main_class, "net.minecraft.client.main.Main");
        assert_eq!(v("1.21.1").java_version.unwrap().component, "java-runtime-delta");
        assert!(v("1.16.5").arguments.is_some());
        assert!(v("1.12.2").arguments.is_none() && v("1.12.2").minecraft_arguments.is_some());
    }

    #[test]
    fn rules_are_evaluated() {
        let none = Features::new();
        assert!(allowed(&[], &none));
        assert!(allowed(&rules(r#"[{"action":"allow","os":{"name":"windows"}}]"#), &none));
        assert!(!allowed(&rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#), &none));
        assert!(!allowed(&rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"windows"}}]"#), &none));
        assert!(!allowed(&rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#), &none));
        let demo = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!allowed(&demo, &none));
        assert!(allowed(&demo, &Features::from(["is_demo_user".to_string()])));
    }

    #[test]
    fn maven_paths() {
        assert_eq!(maven_path("org.ow2.asm:asm:9.6").unwrap(), "org/ow2/asm/asm/9.6/asm-9.6.jar");
        assert_eq!(
            maven_path("net.neoforged:neoforge:21.1.90:client").unwrap(),
            "net/neoforged/neoforge/21.1.90/neoforge-21.1.90-client.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20.1@zip").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1.zip"
        );
        assert!(maven_path("nonsense").is_err());
    }

    #[test]
    fn libraries_for_windows_only() {
        let files = library_files(&v("1.21.1")).unwrap();
        assert!(files.iter().any(|f| f.path.contains("lwjgl") && f.path.ends_with("natives-windows.jar")));
        assert!(!files.iter().any(|f| f.path.contains("macos")
            || f.path.contains("linux")
            || f.path.contains("arm64")
            || f.path.contains("x86.jar")));
        let old = library_files(&v("1.16.5")).unwrap();
        assert!(old.iter().any(|f| f.native && f.path.ends_with("natives-windows.jar")));
        assert!(!old.iter().any(|f| f.path.contains("natives-macos") || f.path.contains("natives-osx")));
    }

    #[test]
    fn libraries_without_downloads_use_the_maven_url() {
        let version: VersionJson = serde_json::from_str(
            r#"{"id":"x","mainClass":"m","libraries":[{"name":"a.b:c:1","url":"https://maven.example/","sha1":"ff"},{"name":"a.b:d:2"}]}"#,
        )
        .unwrap();
        let files = library_files(&version).unwrap();
        assert_eq!(files[0].url, "https://maven.example/a/b/c/1/c-1.jar");
        assert_eq!(files[0].sha1.as_deref(), Some("ff"));
        assert_eq!(files[1].url, "https://libraries.minecraft.net/a/b/d/2/d-2.jar");
    }

    fn with_libraries(libraries: &str) -> VersionJson {
        serde_json::from_str(&format!(r#"{{"id":"x","mainClass":"m","libraries":{libraries}}}"#)).unwrap()
    }

    #[test]
    fn library_paths_must_stay_inside() {
        for path in [r"C:\evil.jar", "/evil.jar", "../evil.jar"] {
            let lib = format!(r#"[{{"name":"a:b:1","downloads":{{"artifact":{{"path":{path:?},"url":"u"}}}}}}]"#);
            assert!(library_files(&with_libraries(&lib)).is_err(), "{path}");
        }
        let lib = r#"[{"name":"a:../..:1","downloads":{"artifact":{"url":"u"}}}]"#;
        assert!(library_files(&with_libraries(lib)).is_err());
        assert!(library_files(&with_libraries(r#"[{"name":"a:../..:1"}]"#)).is_err());
    }

    #[test]
    fn version_ids_must_stay_inside() {
        for (id, parent) in [("..", "null"), ("ok", r#""C:\\x""#)] {
            let json = format!(r#"{{"id":{id:?},"inheritsFrom":{parent},"mainClass":"m"}}"#);
            let version: VersionJson = serde_json::from_str(&json).unwrap();
            assert!(library_files(&version).is_err(), "{id} {parent}");
        }
    }

    #[test]
    fn native_fallback_path_uses_the_arch() {
        let lib = r#"[{"name":"a.b:c:1","natives":{"windows":"natives-windows-${arch}"},
            "downloads":{"classifiers":{"natives-windows-64":{"url":"u"}}}}]"#;
        let files = library_files(&with_libraries(lib)).unwrap();
        assert_eq!(files[0].path, "a/b/c/1/c-1-natives-windows-64.jar");
        assert!(files[0].native);
    }

    #[test]
    fn an_empty_logging_object_keeps_the_parents_configuration() {
        let child: VersionJson = serde_json::from_str(r#"{"id":"c","mainClass":"m","logging":{}}"#).unwrap();
        let merged = merge(v("1.12.2"), child);
        assert!(merged.logging.and_then(|l| l.client).is_some());
    }
}
