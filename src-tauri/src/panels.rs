use crate::state::{AppState, CmdResult, CommandError, Kept, PairingView, claim, safe_url};
use portal_core::join::all_servers;
use portal_core::panel::{self, Panel, Server};
use portal_core::settings::{self, Settings};
use portal_core::{Error, Result};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Serialize)]
pub struct PanelServers {
    pub panel: Panel,
    pub servers: Vec<Server>,
    pub error: Option<CommandError>,
}

#[derive(Debug, Serialize)]
pub struct Linked {
    pub panel: Panel,
    pub missing: Vec<String>,
}

fn linked(panel: Panel, abilities: &[String]) -> Linked {
    Linked { panel, missing: panel::missing_abilities(abilities).into_iter().map(String::from).collect() }
}

fn panel_servers(results: Vec<(Panel, Result<Vec<Server>>)>) -> Vec<PanelServers> {
    results
        .into_iter()
        .map(|(panel, r)| match r {
            Ok(servers) => PanelServers { panel, servers, error: None },
            Err(e) => PanelServers { panel, servers: vec![], error: Some(e.into()) },
        })
        .collect()
}

fn open(app: &AppHandle, url: &str) -> CmdResult<()> {
    app.opener().open_url(url, None::<&str>).map_err(|e| CommandError { kind: "other".into(), message: e.to_string() })
}

#[tauri::command]
pub async fn servers(state: State<'_, AppState>) -> CmdResult<Vec<PanelServers>> {
    Ok(panel_servers(all_servers(&state.paths).await?))
}

#[tauri::command]
pub async fn panels(state: State<'_, AppState>) -> CmdResult<Vec<Panel>> {
    Ok(panel::list(&state.paths)?)
}

#[tauri::command]
pub async fn panel_pair_start(state: State<'_, AppState>, address: String, allow_http: bool) -> CmdResult<PairingView> {
    let address = panel::normalize_address(&address, allow_http)?;
    let pairing = panel::pair_start(&address, &panel::device_name(std::env::var("COMPUTERNAME").ok())).await?;
    let view = PairingView::new(&address, &pairing);
    *state.pairing.lock().unwrap_or_else(|e| e.into_inner()) = Some(Kept { address, pairing, allow_http });
    Ok(view)
}

#[tauri::command]
pub async fn open_pairing_page(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let url = match &*state.pairing.lock().unwrap_or_else(|e| e.into_inner()) {
        Some(k) => format!("{}/settings/api-tokens", k.address),
        None => return Err(Error::Other("no pairing is running".into()).into()),
    };
    open(&app, &url)
}

#[tauri::command]
pub async fn panel_pair_poll(state: State<'_, AppState>) -> CmdResult<Option<Linked>> {
    let Ok(_polling) = claim(&state.busy, "pairing") else { return Ok(None) };
    let (address, pairing, allow_http) = match &*state.pairing.lock().unwrap_or_else(|e| e.into_inner()) {
        Some(k) => (k.address.clone(), k.pairing.clone(), k.allow_http),
        None => return Err(Error::Other("no pairing is running".into()).into()),
    };
    let Some(granted) = panel::pair_poll(&address, &pairing).await? else { return Ok(None) };
    let added = panel::add(&state.paths, &address, panel::host(&address), &granted.token, allow_http)?;
    *state.pairing.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(Some(linked(added, &granted.abilities)))
}

#[tauri::command]
pub async fn panel_pair_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    *state.pairing.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

#[tauri::command]
pub async fn panel_remove(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(panel::remove(&state.paths, &id)?)
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(settings::load(&state.paths))
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, player_name: String) -> CmdResult<Settings> {
    let new = Settings { player_name };
    settings::save(&state.paths, &new)?;
    Ok(new)
}

#[tauri::command]
pub async fn open_page(app: AppHandle, url: String) -> CmdResult<()> {
    open(&app, safe_url(&url)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(id: &str) -> Panel {
        Panel { id: id.into(), address: format!("https://{id}.example.org"), name: id.into() }
    }

    #[test]
    fn a_linked_panel_reports_the_missing_abilities_and_nothing_else() {
        let l = linked(panel("a"), &["servers:read".to_string()]);
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json["missing"], serde_json::json!(["servers:join"]));
        assert_eq!(json.as_object().unwrap().len(), 2);
        assert!(linked(panel("a"), &["servers:read".to_string(), "servers:join".to_string()]).missing.is_empty());
    }

    #[test]
    fn each_panel_keeps_its_place_with_its_servers_or_its_error() {
        let out = panel_servers(vec![
            (panel("a"), Err(Error::Unreachable { panel: "a".into() })),
            (panel("b"), Ok(vec![])),
        ]);
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json[0]["panel"]["id"], "a");
        assert_eq!(json[0]["error"], serde_json::json!({ "kind": "unreachable", "message": "panel a: cannot be reached" }));
        assert_eq!(json[1]["panel"]["id"], "b");
        assert!(json[1]["error"].is_null() && json[1]["servers"].as_array().unwrap().is_empty());
    }
}
