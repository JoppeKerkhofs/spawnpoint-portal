use crate::state::{AppState, CmdResult, CommandError, Running, claim, end_run, forward_progress, list_running, refuse_if_running, shown};
use portal_core::download::numbered;
use portal_core::instance::{self, Instance, Listing, ModFile};
use portal_core::launch::{DEFAULT_MEMORY_MB, Profile};
use portal_core::loader::{self, Loader};
use portal_core::{Error, Paths, discord, gamefiles, join as core_join, mods, panel, play, settings};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GameExited {
    pub instance: String,
    pub code: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct Defaults {
    pub memory_choices_mb: Vec<u32>,
    pub default_memory_mb: u32,
}

fn defaults_value() -> Defaults {
    Defaults { memory_choices_mb: instance::MEMORY_CHOICES_MB.to_vec(), default_memory_mb: DEFAULT_MEMORY_MB }
}

#[tauri::command]
pub async fn defaults() -> CmdResult<Defaults> {
    Ok(defaults_value())
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

// Works out what to show on a blocking thread; the lock is taken first so the newest state is applied last.
pub fn refresh_presence(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut presence = state.presence.lock().unwrap_or_else(|e| e.into_inner());
        let shown = if settings::load(&state.paths).discord { shown(&state.running) } else { None };
        let activity = shown.and_then(|(id, at)| instance::load(&state.paths, &id).ok().map(|i| discord::activity(&i, at)));
        presence.set(activity);
    });
}

fn watch(app: AppHandle, paths: Paths, id: String, mut child: std::process::Child) {
    let started_at = now_secs();
    app.state::<AppState>().running.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), started_at);
    refresh_presence(&app);
    tauri::async_runtime::spawn(async move {
        let code = tokio::task::spawn_blocking(move || child.wait()).await.ok().and_then(|r| r.ok()).and_then(|s| s.code());
        let state = app.state::<AppState>();
        end_run(&state.running, &id, || {
            let _ = instance::record_played(&paths, &id, started_at, seconds_played(started_at, now_secs()));
        });
        refresh_presence(&app);
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
        Some(text) => Some(loader::resolve_new(&state.client, &minecraft, &Loader::parse(&text)?).await?),
        None => None,
    };
    Ok(instance::create_manual(&state.paths, &name, &minecraft, loader)?)
}

#[tauri::command]
pub async fn instance_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let _claim = claim(&state.busy, &id)?;
    refuse_if_running(&state.running, &id)?;
    let paths = state.paths.clone();
    tokio::task::spawn_blocking(move || instance::delete(&paths, &id)).await.map_err(|e| Error::Other(e.to_string()))??;
    Ok(())
}

#[tauri::command]
pub async fn instance_add_mod(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    refuse_if_running(&state.running, &id)?;
    let paths = state.paths.clone();
    let added = tokio::task::spawn_blocking(move || -> portal_core::Result<bool> {
        let inst = instance::load(&paths, &id)?;
        let Some(file) = app.dialog().file().add_filter("Mod", &["jar"]).blocking_pick_file() else { return Ok(false) };
        let file = file.into_path().map_err(|e| Error::Other(e.to_string()))?;
        instance::add_mod(&paths, &inst, &file)?;
        Ok(true)
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))??;
    Ok(added)
}

#[tauri::command]
pub async fn instance_remove_mod(state: State<'_, AppState>, id: String, name: String) -> CmdResult<()> {
    refuse_if_running(&state.running, &id)?;
    let mut inst = instance::load(&state.paths, &id)?;
    Ok(instance::remove_mod(&state.paths, &mut inst, &name)?)
}

#[tauri::command]
pub async fn mods_search(state: State<'_, AppState>, id: String, query: String, offset: usize) -> CmdResult<mods::Found> {
    let inst = instance::load(&state.paths, &id)?;
    Ok(mods::search(&state.client, mods::API, &inst, &query, offset).await?)
}

#[tauri::command]
pub async fn mods_installed(state: State<'_, AppState>, id: String) -> CmdResult<Vec<mods::Installed>> {
    let inst = instance::load(&state.paths, &id)?;
    Ok(mods::installed(&state.client, mods::API, &state.paths, &inst).await?)
}

#[tauri::command]
pub async fn mods_install(state: State<'_, AppState>, id: String, project: String) -> CmdResult<mods::Outcome> {
    let _claim = claim(&state.busy, &id)?;
    refuse_if_running(&state.running, &id)?;
    let inst = instance::load(&state.paths, &id)?;
    Ok(mods::install(&state.client, mods::API, &state.paths, &inst, &project).await?)
}

#[tauri::command]
pub async fn instance_set_picture(state: State<'_, AppState>, id: String, picture: Option<String>) -> CmdResult<Instance> {
    Ok(instance::set_picture(&state.paths, &id, picture)?)
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
    refuse_if_running(&state.running, &id)?;
    let lock = instance::lock(&id)?;
    let inst = instance::load(&state.paths, &id)?;
    let profile = Profile::offline(&settings::load(&state.paths).player_name);
    let stages = play::launch_stages(inst.loader.as_ref().map(|l| l.kind));
    let progress = numbered(stages, forwarder(&app));
    let (child, _) = play::start_instance(&state.client, &state.paths, &inst, profile, None, inst.memory_mb(), &progress).await?;
    watch(app, state.paths.clone(), id, child);
    drop(lock);
    drop(claim);
    Ok(inst)
}

#[derive(Debug, Serialize)]
pub struct Joined {
    pub instance: Instance,
    pub joined: bool,
}

#[tauri::command]
pub async fn join(app: AppHandle, state: State<'_, AppState>, panel_id: String, slug: String) -> CmdResult<Joined> {
    let _claim = claim(&state.busy, &format!("join:{panel_id}/{slug}"))?;
    let existing = core_join::find_instance(&state.paths, &panel_id, &slug)?;
    let _instance_claim = match &existing {
        Some(i) => {
            let held = claim(&state.busy, &i.id)?;
            refuse_if_running(&state.running, &i.id)?;
            Some(held)
        }
        None => None,
    };
    let linked = panel::list(&state.paths)?
        .into_iter()
        .find(|p| p.id == panel_id)
        .ok_or_else(|| Error::Other(format!("no linked panel {panel_id}")))?;
    let profile = Profile::offline(&settings::load(&state.paths).player_name);
    let (child, instance, _, joined) = core_join::join(&state.client, &state.paths, &linked, &slug, profile, &forwarder(&app)).await?;
    watch(app, state.paths.clone(), instance.id.clone(), child);
    Ok(Joined { instance, joined })
}

#[tauri::command]
pub async fn running(state: State<'_, AppState>) -> CmdResult<Vec<Running>> {
    Ok(list_running(&state.running))
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
    fn defaults_come_from_the_core() {
        let v = serde_json::to_value(defaults_value()).unwrap();
        assert_eq!(v, serde_json::json!({ "memory_choices_mb": [4096, 6144, 8192, 10240], "default_memory_mb": 4096 }));
    }

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
