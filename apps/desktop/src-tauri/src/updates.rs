use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use serde::Serialize;
use std::{fs, io::Write, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATER_PROTOCOL: u64 = 1;

fn requires_installer(manifest: &serde_json::Value) -> bool {
    manifest
        .get("minimumUpdaterProtocol")
        .and_then(|v| v.as_u64())
        .is_none_or(|version| version > UPDATER_PROTOCOL || version == 0)
        || manifest
            .get("manualInstallRequired")
            .and_then(|v| v.as_bool())
            != Some(false)
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    phase: &'static str,
    version: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    error: Option<String>,
}

#[derive(Default)]
pub struct AppUpdates(Mutex<Pending>);

#[derive(Default)]
struct Pending {
    status: UpdateStatus,
    update: Option<Update>,
}

fn public_key(app: &tauri::AppHandle) -> String {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|config| config.get("pubkey"))
        .and_then(|key| key.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("desktop-update/package"))
}

// A cached installer is untrusted disk input. Verify both signatures and the bound
// version again immediately before installation; Update::install does not do this.
fn verify_package(
    bytes: &[u8],
    signature: &str,
    public_key: &str,
    version: &str,
) -> Result<(), String> {
    let decode = |value: &str| -> Result<String, String> {
        String::from_utf8(STANDARD.decode(value.trim()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    };
    let key = PublicKey::decode(&decode(public_key)?).map_err(|e| e.to_string())?;
    let signature = Signature::decode(&decode(signature)?).map_err(|e| e.to_string())?;
    key.verify(bytes, &signature, true)
        .map_err(|_| "Update signature is invalid".to_string())?;
    let signed_version = signature
        .trusted_comment()
        .split('\t')
        .find_map(|v| v.strip_prefix("version:"));
    if signed_version != Some(version) {
        return Err("Update signature does not match the release version".into());
    }
    Ok(())
}

fn save_package(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing update cache directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn desktop_update_status(state: State<AppUpdates>) -> UpdateStatus {
    state.0.lock().unwrap().status.clone()
}

#[tauri::command]
pub async fn desktop_update_check(
    app: tauri::AppHandle,
    state: State<'_, AppUpdates>,
) -> Result<UpdateStatus, String> {
    {
        let mut pending = state.0.lock().unwrap();
        if matches!(
            pending.status.phase,
            "checking" | "downloading" | "ready" | "installing"
        ) {
            return Ok(pending.status.clone());
        }
        if cfg!(debug_assertions) || public_key(&app).is_empty() {
            pending.status.phase = "disabled";
            return Ok(pending.status.clone());
        }
        pending.status = UpdateStatus {
            phase: "checking",
            ..Default::default()
        };
    }
    let result = prepare_update(&app, &state).await;
    if let Err(error) = result {
        let mut pending = state.0.lock().unwrap();
        pending.status.phase = "error";
        pending.status.error = Some(error);
    }
    Ok(state.0.lock().unwrap().status.clone())
}

async fn prepare_update(app: &tauri::AppHandle, state: &AppUpdates) -> Result<(), String> {
    let mut update = match app
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
    {
        Some(update) => update,
        None => {
            let _ = fs::remove_file(cache_path(app)?);
            state.0.lock().unwrap().status.phase = "current";
            return Ok(());
        }
    };
    if requires_installer(&update.raw_json) {
        let mut pending = state.0.lock().unwrap();
        pending.status.phase = "manual";
        pending.status.version = Some(update.version);
        return Ok(());
    }
    if update.download_url.scheme() != "https"
        || update.download_url.host_str() != Some("bin.aeroe.io")
        || !update.download_url.path().starts_with("/fletch/updates/")
    {
        return Err("Update download URL is not trusted".into());
    }
    update.timeout = Some(Duration::from_secs(15 * 60));
    let path = cache_path(app)?;
    let key = public_key(app);
    let cached = fs::read(&path)
        .ok()
        .filter(|bytes| verify_package(bytes, &update.signature, &key, &update.version).is_ok());
    if cached.is_none() {
        {
            let mut pending = state.0.lock().unwrap();
            pending.status.phase = "downloading";
            pending.status.version = Some(update.version.clone());
        }
        let bytes = update
            .download(
                |chunk, total| {
                    let mut pending = state.0.lock().unwrap();
                    pending.status.downloaded += chunk as u64;
                    pending.status.total = total;
                },
                || {},
            )
            .await
            .map_err(|e| e.to_string())?;
        verify_package(&bytes, &update.signature, &key, &update.version)?;
        save_package(&path, &bytes)?;
    }
    let mut pending = state.0.lock().unwrap();
    pending.status.phase = "ready";
    pending.status.version = Some(update.version.clone());
    pending.update = Some(update);
    Ok(())
}

#[tauri::command]
pub async fn desktop_update_install(
    app: tauri::AppHandle,
    state: State<'_, AppUpdates>,
) -> Result<(), String> {
    let hardware = app.state::<crate::hardware::HardwareState>();
    hardware.begin_update()?;
    let update = {
        let mut pending = state.0.lock().unwrap();
        let Some(update) = pending.update.take() else {
            hardware.finish_update();
            return Err("No verified update is ready".into());
        };
        pending.status.phase = "installing";
        update
    };
    let install_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let bytes = fs::read(cache_path(&install_app)?).map_err(|e| e.to_string())?;
        verify_package(
            &bytes,
            &update.signature,
            &public_key(&install_app),
            &update.version,
        )?;
        update.install(bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    match result {
        Ok(()) => app.restart(),
        Err(error) => {
            hardware.finish_update();
            let mut pending = state.0.lock().unwrap();
            pending.status.phase = "error";
            pending.status.error = Some(error.clone());
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &str = include_str!("../tests/fixtures/updater.pub");
    const SIGNATURE: &str = include_str!("../tests/fixtures/updater.sig");
    const PACKAGE: &[u8] = b"Nockster updater test package\n";

    #[test]
    fn cache_requires_authentic_bytes_and_matching_signed_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("updates/package");
        save_package(&path, PACKAGE).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(verify_package(&bytes, SIGNATURE, KEY, "9.8.7").is_ok());
        assert!(verify_package(&bytes, SIGNATURE, KEY, "9.8.8").is_err());
        save_package(&path, b"tampered").unwrap();
        assert!(verify_package(&fs::read(&path).unwrap(), SIGNATURE, KEY, "9.8.7").is_err());
        let signature = String::from_utf8(STANDARD.decode(SIGNATURE.trim()).unwrap()).unwrap();
        let forged = STANDARD.encode(signature.replace("version:9.8.7", "version:9.8.8"));
        assert!(verify_package(PACKAGE, &forged, KEY, "9.8.8").is_err());
    }

    #[test]
    fn incompatible_or_manual_releases_require_an_installer() {
        assert!(!requires_installer(
            &serde_json::json!({"minimumUpdaterProtocol": 1, "manualInstallRequired": false})
        ));
        assert!(requires_installer(
            &serde_json::json!({"minimumUpdaterProtocol": 2, "manualInstallRequired": false})
        ));
        assert!(requires_installer(
            &serde_json::json!({"minimumUpdaterProtocol": 1, "manualInstallRequired": true})
        ));
        assert!(requires_installer(&serde_json::json!({})));
    }

    #[test]
    fn firmware_and_app_installation_are_mutually_exclusive() {
        let hardware = crate::hardware::HardwareState::default();
        assert!(hardware.begin_update().is_ok());
        assert!(hardware.is_updating());
        assert!(hardware.begin_update().is_err());
        hardware.finish_update();
        assert!(!hardware.is_updating());
        assert!(hardware.begin_update().is_ok());
    }
}
