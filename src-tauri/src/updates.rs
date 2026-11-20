use crate::instances::record_open_runs;
use crate::state::{AppState, CmdResult, CommandError, claim, may_install};
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

fn sorry<E>(_: E) -> CommandError {
    failed("Could not check for updates. Try again later.")
}

fn failed(e: impl ToString) -> CommandError {
    CommandError { kind: "other".into(), message: e.to_string() }
}

#[tauri::command]
pub async fn app_version(app: AppHandle) -> CmdResult<String> {
    Ok(app.package_info().version.to_string())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> CmdResult<Option<String>> {
    Ok(app.updater().map_err(sorry)?.check().await.map_err(sorry)?.map(|u| u.version))
}

#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    may_install(&state.running, &state.busy, &state.pairing)?;
    let held = claim(&state.busy, "update")?;
    let Some(update) = app.updater().map_err(failed)?.check().await.map_err(failed)? else { return Ok(()) };
    let bytes = update.download(|_, _| {}, || {}).await.map_err(failed)?;
    // The install ends in a hard exit: nothing may have started during the download.
    drop(held);
    may_install(&state.running, &state.busy, &state.pairing)?;
    record_open_runs(&app);
    update.install(bytes).map_err(failed)?;
    app.restart()
}
