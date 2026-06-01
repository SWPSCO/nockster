use crate::cli::SecurityArgs;
use crate::serial::{open, send_call};
use nockster_core::{
    Request, Response, SecurityStatus, HMAC_KEY_PURPOSE_DOWN_ALL, HMAC_KEY_PURPOSE_DOWN_DS,
    HMAC_KEY_PURPOSE_DOWN_JTAG, HMAC_KEY_PURPOSE_UP,
};

pub fn run(args: &SecurityArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    let status = read_security_status(&mut *sp)?;
    print_status(&status);

    validate_expectations(&status, args)?;
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

fn read_security_status(sp: &mut dyn crate::serial::Link) -> anyhow::Result<SecurityStatus> {
    match send_call(sp, 0x05, Request::GetSecurityStatus)? {
        Response::OkSecurityStatus(status) => Ok(status),
        other => anyhow::bail!("unexpected security response: {other:?}"),
    }
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

fn validate_expectations(status: &SecurityStatus, args: &SecurityArgs) -> anyhow::Result<()> {
    if !has_expectations(args) {
        return Ok(());
    }

    let mut failures = Vec::new();
    let expect_production = args.expect_production_lockdown;
    let expect_chip_security = args.expect_chip_security
        || args.expect_hmac_up
        || args.expect_hmac_up_read_protected
        || args.expect_secure_boot
        || args.expect_flash_encryption
        || args.expect_jtag_disabled
        || args.expect_download_disabled
        || args.expect_direct_boot_disabled
        || args.expect_usb_rom_print_disabled
        || args.expect_power_glitch_protection
        || expect_production;

    if expect_chip_security && !status.chip_security_available {
        failures.push(
            "chip-security status is hidden; rebuild firmware with FW_PROFILE=chip-security"
                .to_string(),
        );
    }

    if args.expect_nvs_v2 && (!status.nvs_initialized || status.nvs_schema_version != 2) {
        failures.push(format!(
            "NVS schema v2 expected, got initialized={} schema_v={}",
            on_off(status.nvs_initialized),
            status.nvs_schema_version
        ));
    }

    if status.chip_security_available {
        if (args.expect_hmac_up || args.expect_hmac_up_read_protected || expect_production)
            && status.hmac_user_key_slots == 0
        {
            failures.push("no HMAC_UP eFuse key slot is provisioned".to_string());
        }

        if args.expect_hmac_up_read_protected || expect_production {
            let protected_hmac_up = status.hmac_user_key_slots & status.read_protected_key_slots;
            if protected_hmac_up == 0 {
                failures.push(format!(
                    "no HMAC_UP eFuse key slot is read-protected; hmac_up_slots={}, read_protected_slots={}",
                    format_slot_mask(status.hmac_user_key_slots),
                    format_slot_mask(status.read_protected_key_slots)
                ));
            }
        }

        if (args.expect_secure_boot || expect_production) && !status.secure_boot {
            failures.push("secure boot is not enabled".to_string());
        }

        if (args.expect_flash_encryption || expect_production) && !status.flash_encryption {
            failures.push(format!(
                "flash encryption is not enabled; flash_crypt_cnt=0b{:03b}",
                status.flash_crypt_cnt & 0x07
            ));
        }

        if args.expect_jtag_disabled || expect_production {
            if !status.pad_jtag_disabled {
                failures.push("pad JTAG is not disabled".to_string());
            }
            if !status.usb_jtag_disabled {
                failures.push("USB JTAG is not disabled".to_string());
            }
            if !status.soft_jtag_disabled {
                failures.push(format!(
                    "software JTAG is not disabled; soft_jtag_disable_bits=0b{:03b}",
                    status.soft_jtag_disable_bits & 0x07
                ));
            }
            if !status.usb_serial_jtag_disabled {
                failures.push("USB serial/JTAG peripheral is not disabled".to_string());
            }
        }

        if args.expect_download_disabled || expect_production {
            if !status.download_mode_disabled {
                failures.push("download mode is not disabled".to_string());
            }
            if !status.usb_serial_jtag_download_disabled {
                failures.push("USB serial/JTAG download mode is not disabled".to_string());
            }
            if !status.usb_otg_download_disabled {
                failures.push("USB OTG download mode is not disabled".to_string());
            }
        }

        if (args.expect_direct_boot_disabled || expect_production) && !status.direct_boot_disabled {
            failures.push("direct boot is not disabled".to_string());
        }

        if (args.expect_usb_rom_print_disabled || expect_production)
            && !status.usb_rom_print_disabled
        {
            failures.push("USB ROM printing is not disabled".to_string());
        }

        if args.expect_power_glitch_protection && !status.power_glitch_enabled {
            failures.push("power-glitch protection is not enabled".to_string());
        }
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "security validation failed:\n  - {}",
            failures.join("\n  - ")
        );
    }

    println!("security validation: ok");
    Ok(())
}

fn has_expectations(args: &SecurityArgs) -> bool {
    args.expect_chip_security
        || args.expect_hmac_up
        || args.expect_hmac_up_read_protected
        || args.expect_nvs_v2
        || args.expect_secure_boot
        || args.expect_flash_encryption
        || args.expect_jtag_disabled
        || args.expect_download_disabled
        || args.expect_direct_boot_disabled
        || args.expect_usb_rom_print_disabled
        || args.expect_power_glitch_protection
        || args.expect_production_lockdown
}
