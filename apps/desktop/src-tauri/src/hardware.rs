use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

const VENDOR_ID: u16 = 0x303a;
const PRODUCT_ID: u16 = 0x2001;
const REPORT_SIZE: usize = 64;
const PAYLOAD_SIZE: usize = REPORT_SIZE - 2;
const MAX_WRITE: usize = 65536;

#[derive(Default)]
struct Connection {
    generation: u64,
    device: Option<HidDevice>,
}

#[derive(Clone, Default)]
pub struct HardwareState {
    connection: Arc<Mutex<Connection>>,
    updating: Arc<AtomicBool>,
}

impl HardwareState {
    pub fn is_updating(&self) -> bool {
        self.updating.load(Ordering::SeqCst)
    }
}

#[tauri::command]
pub fn hardware_protect_update(state: tauri::State<'_, HardwareState>, active: bool) {
    state.updating.store(active, Ordering::SeqCst);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDescriptor {
    id: String,
    name: String,
    serial: Option<String>,
}

fn identifier(info: &hidapi::DeviceInfo) -> String {
    info.path()
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn report(payload: &[u8]) -> Result<[u8; REPORT_SIZE], String> {
    if payload.is_empty() || payload.len() > PAYLOAD_SIZE {
        return Err("Invalid hardware report length".into());
    }
    let mut bytes = [0; REPORT_SIZE];
    bytes[0] = 1;
    bytes[1] = payload.len() as u8;
    bytes[2..2 + payload.len()].copy_from_slice(payload);
    Ok(bytes)
}

fn payload(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.len() < 2
        || bytes[0] != 1
        || bytes[1] as usize > PAYLOAD_SIZE
        || bytes[1] as usize + 2 > bytes.len()
    {
        return Err("Invalid response from the hardware wallet".into());
    }
    Ok(bytes[2..2 + bytes[1] as usize].to_vec())
}

async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn hardware_devices() -> Result<Vec<DeviceDescriptor>, String> {
    blocking(|| {
        let api = HidApi::new().map_err(|error| error.to_string())?;
        Ok(api
            .device_list()
            .filter(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID)
            .map(|info| DeviceDescriptor {
                id: identifier(info),
                name: info.product_string().unwrap_or("Nockster").to_owned(),
                serial: info.serial_number().map(str::to_owned),
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn hardware_open(
    state: tauri::State<'_, HardwareState>,
    id: String,
) -> Result<u64, String> {
    let state = state.inner().clone();
    blocking(move || {
        let api = HidApi::new().map_err(|error| error.to_string())?;
        let info = api.device_list().find(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID && identifier(info) == id)
            .ok_or("Nockster is no longer connected. Refresh the device list.")?;
        let mut connection = state.connection.lock().map_err(|_| "Hardware connection unavailable")?;
        if connection.device.is_some() { return Err("Disconnect the current Nockster first".into()); }
        let device = info.open_device(&api).map_err(|error| format!("Unable to open Nockster. Close other apps using it and check USB permissions: {error}"))?;
        connection.generation = connection.generation.checked_add(1).ok_or("Restart Nockster to reconnect")?;
        connection.device = Some(device);
        Ok(connection.generation)
    }).await
}

#[tauri::command]
pub async fn hardware_close(
    state: tauri::State<'_, HardwareState>,
    session: u64,
) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || {
        let mut connection = state
            .connection
            .lock()
            .map_err(|_| "Hardware connection unavailable")?;
        if connection.generation == session {
            connection.device = None;
            state.updating.store(false, Ordering::SeqCst);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn hardware_write(
    state: tauri::State<'_, HardwareState>,
    session: u64,
    data: Vec<u8>,
) -> Result<(), String> {
    if data.is_empty() || data.len() > MAX_WRITE {
        return Err("Invalid hardware message size".into());
    }
    let state = state.inner().clone();
    blocking(move || {
        let mut connection = state
            .connection
            .lock()
            .map_err(|_| "Hardware connection unavailable")?;
        if connection.generation != session {
            return Err("Hardware session ended".into());
        }
        let result = (|| {
            let device = connection
                .device
                .as_ref()
                .ok_or("Connect your Nockster first")?;
            for chunk in data.chunks(PAYLOAD_SIZE) {
                let bytes = report(chunk)?;
                let written = device.write(&bytes).map_err(|error| error.to_string())?;
                if written != bytes.len() {
                    return Err("Incomplete hardware write. Reconnect your Nockster.".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Ok(())
        })();
        if result.is_err() {
            connection.device = None;
            state.updating.store(false, Ordering::SeqCst);
        }
        result
    })
    .await
}

#[tauri::command]
pub async fn hardware_read(
    state: tauri::State<'_, HardwareState>,
    session: u64,
) -> Result<Vec<u8>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let mut connection = state
            .connection
            .lock()
            .map_err(|_| "Hardware connection unavailable")?;
        if connection.generation != session {
            return Err("Hardware session ended".into());
        }
        let result = (|| {
            let device = connection.device.as_ref().ok_or("Nockster disconnected")?;
            let mut bytes = [0; REPORT_SIZE];
            let count = device
                .read_timeout(&mut bytes, 25)
                .map_err(|error| error.to_string())?;
            payload(&bytes[..count])
        })();
        if result.is_err() {
            connection.device = None;
            state.updating.store(false, Ordering::SeqCst);
        }
        result
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_messages_round_trip_with_padding() {
        for length in [1, 61, 62, 63, 124, 1024, 65536] {
            let input: Vec<u8> = (0..length).map(|n| (n % 256) as u8).collect();
            let output: Vec<u8> = input
                .chunks(PAYLOAD_SIZE)
                .flat_map(|chunk| payload(&report(chunk).unwrap()).unwrap())
                .collect();
            assert_eq!(input, output);
        }
    }

    #[test]
    fn malformed_reports_are_rejected_without_truncating_payloads() {
        assert!(report(&[]).is_err());
        assert!(report(&[0; 63]).is_err());
        for bytes in [&[1][..], &[2, 1, 4], &[1, 63, 4], &[1, 2, 4]] {
            assert!(payload(bytes).is_err());
        }
        assert!(payload(&[]).unwrap().is_empty());
        assert!(payload(&[1, 0]).unwrap().is_empty());
    }
}
