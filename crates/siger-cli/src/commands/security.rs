use crate::serial::{open, send_call};
use siger_core::{
    Request, Response, SecurityStatus, HMAC_KEY_PURPOSE_DOWN_ALL, HMAC_KEY_PURPOSE_DOWN_DS,
    HMAC_KEY_PURPOSE_DOWN_JTAG, HMAC_KEY_PURPOSE_UP,
};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let resp = send_call(&mut *sp, 0x05, Request::GetSecurityStatus)?;
    match resp {
        Response::OkSecurityStatus(status) => print_status(&status),
        other => anyhow::bail!("unexpected security response: {other:?}"),
    }
    Ok(())
}

pub(crate) fn print_status(status: &SecurityStatus) {
    println!("security:");
    if status.chip_security_available {
        println!("  mac: {}", format_mac(status.mac));
        println!(
            "  boot: secure_boot={}, flash_encryption={}, secure_version={}, flash_crypt_cnt=0b{:03b}",
            on_off(status.secure_boot),
            on_off(status.flash_encryption),
            status.secure_version,
            status.flash_crypt_cnt & 0x07
        );
        println!(
            "  keys: purposes=[{}], hmac_slots={}, hmac_user_slots={}, read_protected_slots={}",
            format_key_purposes(status.key_purposes),
            format_slot_mask(status.hmac_key_slots),
            format_slot_mask(status.hmac_user_key_slots),
            format_slot_mask(status.read_protected_key_slots)
        );
        println!(
            "  debug: pad_jtag_disabled={}, usb_jtag_disabled={}, soft_jtag_disabled={} (bits=0b{:03b}), usb_serial_jtag_disabled={}",
            on_off(status.pad_jtag_disabled),
            on_off(status.usb_jtag_disabled),
            on_off(status.soft_jtag_disabled),
            status.soft_jtag_disable_bits & 0x07,
            on_off(status.usb_serial_jtag_disabled)
        );
        println!(
            "  download: disabled={}, secure_download={}, usb_serial_jtag_download_disabled={}, usb_otg_download_disabled={}, direct_boot_disabled={}, usb_rom_print_disabled={}",
            on_off(status.download_mode_disabled),
            on_off(status.secure_download_enabled),
            on_off(status.usb_serial_jtag_download_disabled),
            on_off(status.usb_otg_download_disabled),
            on_off(status.direct_boot_disabled),
            on_off(status.usb_rom_print_disabled)
        );
        println!(
            "  physical: power_glitch_enabled={}",
            on_off(status.power_glitch_enabled)
        );
    } else {
        println!("  chip: hidden (firmware built without chip-security)");
    }
    println!(
        "  nvs: initialized={}, schema_v={}, slots={}",
        on_off(status.nvs_initialized),
        status.nvs_schema_version,
        status.nvs_slot_count
    );
}

fn on_off(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn format_mac(mac: [u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

fn format_slot_mask(mask: u8) -> String {
    let slots: Vec<String> = (0..6)
        .filter(|slot| mask & (1 << slot) != 0)
        .map(|slot| slot.to_string())
        .collect();
    if slots.is_empty() {
        "-".to_string()
    } else {
        slots.join(",")
    }
}

fn format_key_purposes(purposes: [u8; 6]) -> String {
    purposes
        .iter()
        .enumerate()
        .map(|(slot, purpose)| format!("{}:{}", slot, purpose_name(*purpose)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn purpose_name(purpose: u8) -> String {
    match purpose {
        0 => "empty".to_string(),
        HMAC_KEY_PURPOSE_DOWN_ALL => "hmac-all".to_string(),
        HMAC_KEY_PURPOSE_DOWN_JTAG => "hmac-jtag".to_string(),
        HMAC_KEY_PURPOSE_DOWN_DS => "hmac-ds".to_string(),
        HMAC_KEY_PURPOSE_UP => "hmac-up".to_string(),
        other => other.to_string(),
    }
}
