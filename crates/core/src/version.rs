use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

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
    Ok(files)
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
}
