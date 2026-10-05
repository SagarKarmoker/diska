mod clean;
mod commands;
mod error;
mod junk;
mod model;
mod paths;
mod scan;
mod volumes;

use commands::ScanState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ScanState::default())
        .invoke_handler(tauri::generate_handler![
            commands::system_info,
            commands::list_volumes,
            commands::default_scan_roots,
            commands::start_scan,
            commands::cancel_scan,
            commands::detect_junk,
            commands::preview_clean,
            commands::dry_run_clean,
            commands::clean,
        ])
        .run(tauri::generate_context!())
        .expect("error while running diska");
}
