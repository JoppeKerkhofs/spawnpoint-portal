pub mod instances;
pub mod state;

use state::AppState;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            instances::instances,
            instances::instance_get,
            instances::instance_mods,
            instances::instance_create,
            instances::instance_delete,
            instances::instance_add_mod,
            instances::instance_remove_mod,
            instances::instance_set_memory,
            instances::instance_check_manual,
            instances::instance_launch,
            instances::join,
            instances::minecraft_versions,
            instances::open_mods_folder,
            instances::open_instance_folder,
        ])
        .setup(|app| {
            app.manage(AppState {
                client: portal_core::http_client(),
                paths: portal_core::Paths::new()?,
                busy: Mutex::new(HashSet::new()),
                pairing: Mutex::new(None),
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
