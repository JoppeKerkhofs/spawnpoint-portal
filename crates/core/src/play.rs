use crate::download::{Progress, ProgressTx, blocking};
use crate::gamefiles::{install, version_json};
use crate::instance::{self, Instance};
use crate::launch::{arguments, arguments_joined, command, extract_natives, supported, LaunchOptions, Profile};
use crate::loader::{forge_supported, profile, resolve, Loader, LoaderKind};
use crate::version::merge;
use crate::{java, Paths, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRequest {
    pub minecraft: String,
    pub game_dir: PathBuf,
    pub profile: Profile,
    pub server: Option<String>,
    pub memory_mb: u32,
    pub loader: Option<Loader>,
}

pub const SYNC_STAGES: [&str; 2] = ["check", "mods"];

pub fn launch_stages(loader: Option<LoaderKind>) -> Vec<&'static str> {
    let mut stages = vec!["java", "libraries", "assets"];
    if matches!(loader, Some(LoaderKind::NeoForge | LoaderKind::Forge)) {
        stages.extend(["loader libraries", "loader"]);
    }
    stages
}

pub async fn start(client: &Client, paths: &Paths, request: &StartRequest, progress: &ProgressTx) -> Result<(std::process::Child, bool)> {
    let version = version_json(client, paths, &request.minecraft).await?;
    if version.kind.as_deref().is_some_and(|k| !supported(k)) {
        return Err(crate::Error::Unsupported("Minecraft alpha and beta versions are not supported".into()));
    }
    if let (Some(Loader { kind: LoaderKind::Forge, .. }), Some(time)) = (&request.loader, &version.release_time) {
        forge_supported(time)?;
    }
    let game_dir = std::path::absolute(&request.game_dir)?;
    let opts = LaunchOptions {
        game_dir: game_dir.clone(),
        profile: request.profile.clone(),
        server: request.server.clone(),
        memory_mb: request.memory_mb,
    };
    // Refuses a version without launch arguments before anything is downloaded.
    arguments(paths, &version, &opts)?;
    let loader = match &request.loader {
        Some(l) => Some(resolve(client, &request.minecraft, l).await?),
        None => None,
    };
    let component = version.java_version.as_ref().map_or("jre-legacy", |j| j.component.as_str());
    let javaw = java::ensure(client, paths, component, progress).await?;
    install(client, paths, &version, Some(&game_dir), progress).await?;
    let version = match &loader {
        Some(Loader { kind, version: Some(loader_version) }) => {
            let loader_json = profile(client, paths, &request.minecraft, *kind, loader_version, &javaw, progress).await?;
            let merged = merge(version, loader_json);
            install(client, paths, &merged, Some(&game_dir), progress).await?;
            merged
        }
        Some(_) => return Err(crate::Error::Other("the loader has no version after resolving".into())),
        None => version,
    };
    let (args, joined) = arguments_joined(paths, &version, &opts)?;
    let _ = progress.send(Progress { step: 0, steps: 0, stage: "starting".into(), done: 0, total: 0 });
    let (root, extracting) = (paths.clone(), version.clone());
    blocking(move || extract_natives(&root, &extracting)).await?;
    std::fs::create_dir_all(&game_dir)?;
    Ok((command(&javaw, &args, &game_dir).spawn()?, joined))
}

pub async fn start_instance(
    client: &Client,
    paths: &Paths,
    instance: &Instance,
    profile: Profile,
    server: Option<String>,
    memory_mb: u32,
    progress: &ProgressTx,
) -> Result<(std::process::Child, bool)> {
    let request = StartRequest {
        minecraft: instance.minecraft.clone(),
        game_dir: instance::game_dir(paths, &instance.id),
        profile,
        server,
        memory_mb,
        loader: instance.loader.clone(),
    };
    start(client, paths, &request, progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::LoaderKind;

    #[test]
    fn launch_stages_per_loader() {
        assert_eq!(launch_stages(None), ["java", "libraries", "assets"]);
        assert_eq!(launch_stages(Some(LoaderKind::Fabric)), ["java", "libraries", "assets"]);
        assert_eq!(launch_stages(Some(LoaderKind::Forge)), ["java", "libraries", "assets", "loader libraries", "loader"]);
    }
}
