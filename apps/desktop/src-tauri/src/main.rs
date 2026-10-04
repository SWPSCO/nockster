#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod storage;

fn main() {
    tauri::Builder::default()
        .manage(storage::WalletStorage::default())
        .invoke_handler(tauri::generate_handler![
            storage::desktop_storage_get,
            storage::desktop_storage_set,
            storage::desktop_storage_remove
        ])
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("Unable to start Nockster");
}
