use crate::download::ProgressTx;
use crate::gamefiles::{install, version_json};
use crate::launch::{arguments, command, extract_natives, LaunchOptions, Profile};
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
}

pub async fn start(client: &Client, paths: &Paths, request: &StartRequest, progress: &ProgressTx) -> Result<std::process::Child> {
    let version = version_json(client, paths, &request.minecraft).await?;
    let game_dir = std::path::absolute(&request.game_dir)?;
    let opts = LaunchOptions {
        game_dir: game_dir.clone(),
        profile: request.profile.clone(),
        server: request.server.clone(),
        memory_mb: request.memory_mb,
    };
    let args = arguments(paths, &version, &opts)?;
    install(client, paths, &version, progress).await?;
    let component = version.java_version.as_ref().map_or("jre-legacy", |j| j.component.as_str());
    let javaw = java::ensure(client, paths, component, progress).await?;
    extract_natives(paths, &version)?;
    std::fs::create_dir_all(&game_dir)?;
    Ok(command(&javaw, &args, &game_dir).spawn()?)
}
