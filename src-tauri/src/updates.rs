use crate::state::{AppState, CmdResult, CommandError, claim, may_install, refuse_while_playing};
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

fn failed(e: impl ToString) -> CommandError {
    CommandError { kind: "other".into(), message: e.to_string() }
}

#[tauri::command]
pub async fn app_version(app: AppHandle) -> CmdResult<String> {
    Ok(app.package_info().version.to_string())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> CmdResult<Option<String>> {
    Ok(app.updater().map_err(failed)?.check().await.map_err(failed)?.map(|u| u.version))
}

#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    may_install(&state.running, &state.busy)?;
    let _claim = claim(&state.busy, "update")?;
    let Some(update) = app.updater().map_err(failed)?.check().await.map_err(failed)? else { return Ok(()) };
    let bytes = update.download(|_, _| {}, || {}).await.map_err(failed)?;
    // A game may have been started during the download.
    refuse_while_playing(&state.running)?;
    update.install(bytes).map_err(failed)?;
    app.restart()
}
