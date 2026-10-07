#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hardware;
mod storage;
mod updates;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(storage::WalletStorage::default())
        .manage(hardware::HardwareState::default())
        .manage(updates::AppUpdates::default())
        .invoke_handler(tauri::generate_handler![
            storage::desktop_storage_get,
            storage::desktop_storage_set,
            storage::desktop_storage_remove,
            hardware::hardware_devices,
            hardware::hardware_open,
            hardware::hardware_close,
            hardware::hardware_write,
            hardware::hardware_read,
            hardware::hardware_protect_update,
            updates::desktop_update_status,
            updates::desktop_update_check,
            updates::desktop_update_install
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<hardware::HardwareState>().is_updating() {
                    api.prevent_close();
                }
            }
        })
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("Unable to start Nockster");
}
