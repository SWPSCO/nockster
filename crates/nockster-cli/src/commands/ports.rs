//! `list-ports` — enumerate serial ports and the nockster HID device so users
//! don't have to guess what to pass to `--port`.

use crate::serial::{NOCKSTER_USB_PID, NOCKSTER_USB_VID};
use crate::ui;
use hidapi::HidApi;
use serialport::SerialPortType;

pub fn run() -> anyhow::Result<()> {
    ui::header("ports");

    ui::subhead("serial");
    match serialport::available_ports() {
        Ok(ports) => {
            // USB/Bluetooth ports are the ones a device actually shows up on;
            // legacy PCI/unknown `ttyS*` ports are summarized, not listed.
            let mut shown = 0usize;
            let mut other = 0usize;
            for p in &ports {
                let detail = match &p.port_type {
                    SerialPortType::UsbPort(info) => {
                        let id = format!("{:04x}:{:04x}", info.vid, info.pid);
                        let product = info.product.clone().unwrap_or_default();
                        format!("{} {}", ui::dim(&id), product)
                    }
                    SerialPortType::BluetoothPort => ui::dim("bluetooth"),
                    _ => {
                        other += 1;
                        continue;
                    }
                };
                ui::item(format!("{}  {}", ui::accent(&p.port_name), detail));
                shown += 1;
            }
            if shown == 0 {
                ui::note("no USB/Bluetooth serial ports");
            }
            if other > 0 {
                ui::note(&format!("(+{other} legacy/unknown ports hidden)"));
            }
        }
        Err(e) => ui::warn(&format!("serial enumeration failed: {e}")),
    }

    ui::subhead("hid");
    let want = format!("{NOCKSTER_USB_VID:04x}:{NOCKSTER_USB_PID:04x}");
    match HidApi::new() {
        Ok(api) => {
            let mut found = 0usize;
            for d in api.device_list() {
                if d.vendor_id() != NOCKSTER_USB_VID || d.product_id() != NOCKSTER_USB_PID {
                    continue;
                }
                found += 1;
                let product = d.product_string().unwrap_or("nockster");
                let serial = d.serial_number().unwrap_or("");
                ui::item(format!(
                    "{}  {}  {}",
                    ui::accent(&format!("hid:{}", want)),
                    ui::strong(product),
                    ui::dim(serial),
                ));
            }
            if found == 0 {
                ui::note(&format!(
                    "no nockster HID device found (looking for {want})"
                ));
            } else {
                ui::note("use --port hid (auto) or the hid:VID:PID selector above");
            }
        }
        Err(e) => ui::warn(&format!("hid init failed: {e}")),
    }

    Ok(())
}
