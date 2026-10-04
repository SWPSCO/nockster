use serde_json::{Map, Value};
use std::{fs, io::Write, path::Path, sync::Mutex};
use tauri::{Manager, State};

#[derive(Default)]
pub struct WalletStorage(Mutex<()>);

type Result<T> = std::result::Result<T, String>;

fn read(path: &Path) -> Result<Map<String, Value>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            "Saved wallet data is unreadable. Preserve wallet.json for recovery.".into()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(_) => Err("Unable to read saved wallet data.".into()),
    }
}

fn write(path: &Path, data: &Map<String, Value>) -> Result<()> {
    let parent = path.parent().ok_or("Missing wallet directory")?;
    fs::create_dir_all(parent).map_err(|_| "Unable to create wallet directory")?;
    let mut pending =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "Unable to prepare wallet save")?;
    serde_json::to_writer(&mut pending, data).map_err(|_| "Unable to serialize wallet data")?;
    pending.flush().map_err(|_| "Unable to flush wallet data")?;
    pending
        .as_file()
        .sync_all()
        .map_err(|_| "Unable to sync wallet data")?;
    pending
        .persist(path)
        .map_err(|_| "Unable to finish wallet save")?;
    Ok(())
}

#[tauri::command]
pub fn desktop_storage_get(
    app: tauri::AppHandle,
    state: State<WalletStorage>,
    keys: Vec<String>,
) -> Result<Map<String, Value>> {
    let _guard = state
        .0
        .lock()
        .map_err(|_| "Wallet storage is unavailable")?;
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "Missing app data directory")?
        .join("wallet.json");
    let data = read(&path)?;
    Ok(keys
        .into_iter()
        .filter_map(|key| data.get(&key).cloned().map(|value| (key, value)))
        .collect())
}

#[tauri::command]
pub fn desktop_storage_set(
    app: tauri::AppHandle,
    state: State<WalletStorage>,
    items: Map<String, Value>,
) -> Result<()> {
    let _guard = state
        .0
        .lock()
        .map_err(|_| "Wallet storage is unavailable")?;
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "Missing app data directory")?
        .join("wallet.json");
    let mut data = read(&path)?;
    data.extend(items);
    write(&path, &data)
}

#[tauri::command]
pub fn desktop_storage_remove(
    app: tauri::AppHandle,
    state: State<WalletStorage>,
    keys: Vec<String>,
) -> Result<()> {
    let _guard = state
        .0
        .lock()
        .map_err(|_| "Wallet storage is unavailable")?;
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "Missing app data directory")?
        .join("wallet.json");
    let mut data = read(&path)?;
    for key in keys {
        data.remove(&key);
    }
    write(&path, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persists_ciphertext_and_metadata_across_atomic_replacements() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        assert!(read(&path).unwrap().is_empty());
        let mut data = Map::from_iter([("vault".into(), json!({"ciphertext":"encrypted"}))]);
        write(&path, &data).unwrap();
        data.insert("walletState".into(), json!({"name":"Savings"}));
        write(&path, &data).unwrap();
        assert_eq!(read(&path).unwrap(), data);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn unreadable_storage_is_an_error_and_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        for bytes in [
            b"partial JSON".as_slice(),
            b"[]".as_slice(),
            b"null".as_slice(),
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(read(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }
}
