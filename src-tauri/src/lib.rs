pub mod instances;
pub mod panels;
pub mod state;
pub mod updates;

use state::AppState;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            instances::instances,
            instances::instance_get,
            instances::instance_mods,
            instances::instance_create,
            instances::instance_delete,
            instances::instance_add_mod,
            instances::instance_remove_mod,
            instances::instance_set_memory,
            instances::instance_set_picture,
            instances::mods_search,
            instances::mods_installed,
            instances::mods_install,
            instances::instance_check_manual,
            instances::instance_launch,
            instances::join,
            instances::running,
            instances::defaults,
            instances::minecraft_versions,
            instances::loader_versions,
            instances::open_mods_folder,
            instances::open_instance_folder,
            panels::servers,
            panels::panels,
            panels::panel_pair_start,
            panels::open_pairing_page,
            panels::panel_pair_poll,
            panels::panel_pair_cancel,
            panels::panel_remove,
            panels::settings_get,
            panels::settings_set,
            panels::open_page,
            updates::app_version,
            updates::update_check,
            updates::update_install,
        ])
        .setup(|app| {
            app.manage(AppState {
                client: portal_core::http_client(),
                paths: portal_core::Paths::new()?,
                busy: Mutex::new(HashSet::new()),
                running: Mutex::new(HashMap::new()),
                presence: Default::default(),
                pairing: Mutex::new(None),
                pairings: Default::default(),
            });
            instances::refresh_presence(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                instances::record_open_runs(app);
            }
        });
}
