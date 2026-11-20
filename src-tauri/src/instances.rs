use crate::state::{AppState, CmdResult, CommandError, claim, forward_progress};
use portal_core::download::numbered;
use portal_core::instance::{self, Instance, Listing, ModFile, Source};
use portal_core::launch::Profile;
use portal_core::loader::{self, Loader};
use portal_core::{Error, Paths, gamefiles, join as core_join, panel, play, settings};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GameExited {
    pub instance: String,
    pub code: Option<i32>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn seconds_played(started_at: u64, now: u64) -> u64 {
    now.saturating_sub(started_at)
}

fn forwarder(app: &AppHandle) -> portal_core::download::ProgressTx {
    let app = app.clone();
    forward_progress(move |p| {
        let _ = app.emit("progress", p);
    })
}

fn watch(app: AppHandle, paths: Paths, id: String, mut child: std::process::Child) {
    let started_at = now_secs();
    tauri::async_runtime::spawn(async move {
        let code = tokio::task::spawn_blocking(move || child.wait()).await.ok().and_then(|r| r.ok()).and_then(|s| s.code());
        let _ = instance::record_played(&paths, &id, started_at, seconds_played(started_at, now_secs()));
        let _ = app.emit("game-exited", GameExited { instance: id, code });
    });
}

#[tauri::command]
pub async fn instances(state: State<'_, AppState>) -> CmdResult<Listing> {
    Ok(instance::list_all(&state.paths)?)
}

#[tauri::command]
pub async fn instance_get(state: State<'_, AppState>, id: String) -> CmdResult<Instance> {
    Ok(instance::load(&state.paths, &id)?)
}

#[tauri::command]
pub async fn instance_mods(state: State<'_, AppState>, id: String) -> CmdResult<Vec<ModFile>> {
    let inst = instance::load(&state.paths, &id)?;
    Ok(instance::mods(&state.paths, &inst)?)
}

#[tauri::command]
pub async fn instance_create(state: State<'_, AppState>, name: String, minecraft: String, loader: Option<String>) -> CmdResult<Instance> {
    let loader = match loader {
        Some(text) => Some(loader::resolve(&state.client, &minecraft, &Loader::parse(&text)?).await?),
        None => None,
    };
    Ok(instance::create(&state.paths, &name, &minecraft, loader, Source::Manual)?)
}

#[tauri::command]
pub async fn instance_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let _claim = claim(&state.busy, &id)?;
    Ok(instance::delete(&state.paths, &id)?)
}

#[tauri::command]
pub async fn instance_add_mod(state: State<'_, AppState>, id: String, path: String) -> CmdResult<()> {
    let inst = instance::load(&state.paths, &id)?;
    Ok(instance::add_mod(&state.paths, &inst, Path::new(&path))?)
}

#[tauri::command]
pub async fn instance_remove_mod(state: State<'_, AppState>, id: String, name: String) -> CmdResult<()> {
    let mut inst = instance::load(&state.paths, &id)?;
    Ok(instance::remove_mod(&state.paths, &mut inst, &name)?)
}

#[tauri::command]
pub async fn instance_set_memory(state: State<'_, AppState>, id: String, mb: u32) -> CmdResult<Instance> {
    Ok(instance::set_memory(&state.paths, &id, mb)?)
}

#[tauri::command]
pub async fn instance_check_manual(state: State<'_, AppState>, id: String) -> CmdResult<Instance> {
    Ok(instance::check_manual(&state.paths, &id)?)
}

#[tauri::command]
pub async fn instance_launch(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<Instance> {
    let claim = claim(&state.busy, &id)?;
    let inst = instance::load(&state.paths, &id)?;
    let profile = Profile::offline(&settings::load(&state.paths).player_name);
    let stages = play::launch_stages(inst.loader.as_ref().map(|l| l.kind));
    let progress = numbered(stages, forwarder(&app));
    let child = play::start_instance(&state.client, &state.paths, &inst, profile, None, inst.memory_mb(), &progress).await?;
    drop(claim);
    watch(app, state.paths.clone(), id, child);
    Ok(inst)
}

#[tauri::command]
pub async fn join(app: AppHandle, state: State<'_, AppState>, panel_id: String, slug: String) -> CmdResult<Instance> {
    let _claim = claim(&state.busy, &format!("join:{panel_id}/{slug}"))?;
    let linked = panel::list(&state.paths)?
        .into_iter()
        .find(|p| p.id == panel_id)
        .ok_or_else(|| Error::Other(format!("no linked panel {panel_id}")))?;
    let profile = Profile::offline(&settings::load(&state.paths).player_name);
    let (child, inst, _) = core_join::join(&state.client, &state.paths, &linked, &slug, profile, &forwarder(&app)).await?;
    watch(app, state.paths.clone(), inst.id.clone(), child);
    Ok(inst)
}

#[tauri::command]
pub async fn minecraft_versions(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    Ok(gamefiles::supported_releases(&gamefiles::manifest(&state.client).await?))
}

pub fn open_folder(app: &AppHandle, path: &Path) -> CmdResult<()> {
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| CommandError { kind: "other".into(), message: e.to_string() })
}

#[tauri::command]
pub async fn open_mods_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let inst = instance::load(&state.paths, &id)?;
    let mods = instance::game_dir(&state.paths, &inst.id).join("mods");
    std::fs::create_dir_all(&mods).map_err(Error::from)?;
    open_folder(&app, &mods)
}

#[tauri::command]
pub async fn open_instance_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    open_folder(&app, &instance::folder(&state.paths, &id)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_played_never_goes_below_zero() {
        assert_eq!(seconds_played(100, 160), 60);
        assert_eq!(seconds_played(200, 100), 0);
    }

    #[test]
    fn game_exited_has_instance_and_code_only() {
        let v = serde_json::to_value(GameExited { instance: "atm10".into(), code: Some(0) }).unwrap();
        assert_eq!(v, serde_json::json!({ "instance": "atm10", "code": 0 }));
    }
}
