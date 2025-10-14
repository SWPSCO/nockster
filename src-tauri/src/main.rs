use std::sync::Mutex;
use std::time::Duration;
use serialport::SerialPort;

struct SerialState {
    port: Option<Box<dyn SerialPort>>,
}

impl SerialState {
    fn new() -> Self {
        Self { port: None }
    }
}

#[tauri::command]
fn list_serial_ports() -> Result<Vec<String>, String> {
    serialport::available_ports()
        .map(|ports| ports.iter().map(|p| p.port_name.clone()).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn connect_serial(
    state: tauri::State<Mutex<SerialState>>,
    port: String,
    baud_rate: u32
) -> Result<(), String> {
    let mut serial_state = state.lock().unwrap();
    let port = serialport::new(port, baud_rate)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| e.to_string())?;
    serial_state.port = Some(port);
    Ok(())
}

#[tauri::command]
fn disconnect_serial(state: tauri::State<Mutex<SerialState>>) -> Result<(), String> {
    let mut serial_state = state.lock().unwrap();
    serial_state.port = None;
    Ok(())
}

#[tauri::command]
fn serial_write(state: tauri::State<Mutex<SerialState>>, data: Vec<u8>) -> Result<(), String> {
    let mut serial_state = state.lock().unwrap();
    if let Some(port) = &mut serial_state.port {
        port.write_all(&data).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Not connected".to_string())
    }
}

#[tauri::command]
fn serial_read(state: tauri::State<Mutex<SerialState>>) -> Result<Vec<u8>, String> {
    let mut serial_state = state.lock().unwrap();
    if let Some(port) = &mut serial_state.port {
        let mut buf = vec![0u8; 1024];
        match port.read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(vec![]),
            Err(e) => Err(e.to_string()),
        }
    } else {
        Err("Not connected".to_string())
    }
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(SerialState::new()))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;

                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_serial_ports,
            connect_serial,
            disconnect_serial,
            serial_write,
            serial_read
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}