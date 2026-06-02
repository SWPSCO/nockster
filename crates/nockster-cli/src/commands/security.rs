use crate::cli::SecurityArgs;
use crate::serial::{open, send_call};
use crate::ui;
use nockster_core::{
    Request, Response, SecurityStatus, HMAC_KEY_PURPOSE_DOWN_ALL, HMAC_KEY_PURPOSE_DOWN_DS,
    HMAC_KEY_PURPOSE_DOWN_JTAG, HMAC_KEY_PURPOSE_UP,
};

pub fn run(args: &SecurityArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    let status = read_security_status(&mut *sp)?;
    ui::header("security");
    print_status(&status);

    validate_expectations(&status, args)?;
    Ok(())
}

/// Coloured dot for a boolean where `true` is the hardened/desired state.
fn flag(value: bool) -> String {
    ui::yesno(value)
}

pub(crate) fn print_status(status: &SecurityStatus) {
    ui::subhead("security");
    if status.chip_security_available {
        ui::kv("mac", ui::strong(&format_mac(status.mac)));

        ui::subhead("boot");
        ui::kv("secure boot", flag(status.secure_boot));
        ui::kv("flash encrypt", flag(status.flash_encryption));
        ui::kv(
            "secure version",
            ui::strong(&status.secure_version.to_string()),
        );
        ui::kv(
            "flash crypt",
            ui::dim(&format!("0b{:03b}", status.flash_crypt_cnt & 0x07)),
        );

        ui::subhead("keys");
        ui::kv("purposes", format_key_purposes(status.key_purposes));
        ui::kv(
            "hmac slots",
            ui::strong(&format_slot_mask(status.hmac_key_slots)),
        );
        ui::kv(
            "user slots",
            ui::strong(&format_slot_mask(status.hmac_user_key_slots)),
        );
        ui::kv(
            "read-prot",
            ui::strong(&format_slot_mask(status.read_protected_key_slots)),
        );

        ui::subhead("debug");
        ui::kv("pad jtag", flag(status.pad_jtag_disabled));
        ui::kv("usb jtag", flag(status.usb_jtag_disabled));
        ui::kv(
            "soft jtag",
            format!(
                "{}  {}",
                flag(status.soft_jtag_disabled),
                ui::dim(&format!(
                    "bits=0b{:03b}",
                    status.soft_jtag_disable_bits & 0x07
                ))
            ),
        );
        ui::kv("usb serial/jtag", flag(status.usb_serial_jtag_disabled));

        ui::subhead("download");
        ui::kv("disabled", flag(status.download_mode_disabled));
        ui::kv("secure dl", flag(status.secure_download_enabled));
        ui::kv(
            "usb s/jtag dl",
            flag(status.usb_serial_jtag_download_disabled),
        );
        ui::kv("usb otg dl", flag(status.usb_otg_download_disabled));
        ui::kv("direct boot", flag(status.direct_boot_disabled));
        ui::kv("usb rom print", flag(status.usb_rom_print_disabled));

        ui::subhead("physical");
        ui::kv("power glitch", flag(status.power_glitch_enabled));
    } else {
        ui::note("chip: hidden (firmware built without chip-security)");
    }

    ui::subhead("nvs");
    ui::kv("initialized", flag(status.nvs_initialized));
    ui::kv("schema", ui::strong(&status.nvs_schema_version.to_string()));
    ui::kv("slots", ui::strong(&status.nvs_slot_count.to_string()));
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

    ui::ok("security validation: ok");
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
