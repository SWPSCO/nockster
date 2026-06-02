use crate::keys::pubkey_to_b58;
use crate::serial::{open, send_call};
use crate::ui;
use nockster_core::{
    Request, Response, FEATURE_ALL_KNOWN, FEATURE_BUILD_INFO, FEATURE_CHEETAH,
    FEATURE_DEVICE_REBOOT, FEATURE_FRAG, FEATURE_PIN_CHANGE_UI, FEATURE_RELEASE_INFO,
    FEATURE_SECURE_UPDATE, FEATURE_SECURITY_STATUS, FEATURE_SEED_LABELS, FEATURE_TOUCH_CALIBRATION,
    FEATURE_TOUCH_CALIBRATION_UI, FEATURE_TOUCH_DIAGNOSTICS, FEATURE_UPDATE_BOOT_STATUS,
    FEATURE_XPUB,
};
use std::fmt::Write as _;

pub fn run(port: &str, baud: u32, version: u8) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

    ui::header("device info");

    // Get device info
    let resp: Response = send_call(&mut *sp, 0x01, Request::GetInfo)?;
    match resp {
        Response::Info {
            proto_v,
            fw_major,
            fw_minor,
            features,
            has_seed,
            cheetah_pubs,
        } => {
            ui::kv("proto", ui::strong(&proto_v.to_string()));
            ui::kv("firmware", ui::strong(&format!("{fw_major}.{fw_minor}")));
            ui::kv("features", format_features(features));
            ui::kv("flags", ui::dim(&format!("0x{features:08x}")));
            if features & FEATURE_BUILD_INFO != 0 {
                let build_resp: Response = send_call(&mut *sp, 0x03, Request::GetBuildInfo)?;
                match build_resp {
                    Response::OkBuildInfo(build) => {
                        let dirty = if build.git_dirty { "-dirty" } else { "" };
                        ui::kv(
                            "build",
                            format!(
                                "{} · proto {} · git {}{}",
                                build.build_profile,
                                build.protocol_v,
                                build.git_commit,
                                ui::amber(dirty)
                            ),
                        );
                        ui::kv("tx-types", ui::dim(&build.tx_types_rev.to_string()));
                    }
                    other => anyhow::bail!("unexpected build info response: {other:?}"),
                }
            }
            if features & FEATURE_RELEASE_INFO != 0 {
                let release_resp: Response = send_call(&mut *sp, 0x04, Request::GetReleaseInfo)?;
                match release_resp {
                    Response::OkReleaseInfo(release) => {
                        ui::kv("release", ui::strong(&release.release_version.to_string()));
                    }
                    other => anyhow::bail!("unexpected release info response: {other:?}"),
                }
            }
            if has_seed {
                ui::subhead("keys");
                if cheetah_pubs.is_empty() {
                    ui::note("device locked; pubkeys withheld");
                } else {
                    for (idx, pubinfo) in cheetah_pubs.iter().enumerate() {
                        let pk_xy = (pubinfo.x, pubinfo.y);
                        let b58 = pubkey_to_b58(&pk_xy, version);
                        let path_display = format_path(pubinfo.path.as_slice());
                        ui::item(format!(
                            "slot {slot} {key}  {path}  {pk}",
                            slot = pubinfo.slot,
                            key = ui::dim(&format!("key[{idx:02}]")),
                            path = ui::strong(&path_display),
                            pk = ui::accent(&b58),
                        ));
                    }
                }
            }
        }
        other => anyhow::bail!("unexpected info response: {other:?}"),
    }

    // Get lock status
    let resp: Response = send_call(&mut *sp, 0x02, Request::GetLockStatus)?;
    match resp {
        Response::OkLockStatus {
            locked,
            attempts_remaining,
        } => {
            ui::subhead("status");
            let dot = if locked {
                ui::dot(ui::Health::Bad, "locked")
            } else {
                ui::dot(ui::Health::Good, "unlocked")
            };
            ui::kv("lock", dot);
            ui::kv("attempts", ui::strong(&attempts_remaining.to_string()));
        }
        other => anyhow::bail!("unexpected status response: {other:?}"),
    }

    Ok(())
}

pub(crate) fn format_features(features: u32) -> String {
    let mut names = Vec::new();
    if features & FEATURE_CHEETAH != 0 {
        names.push("cheetah".to_string());
    }
    if features & FEATURE_FRAG != 0 {
        names.push("frag".to_string());
    }
    if features & FEATURE_XPUB != 0 {
        names.push("xpub".to_string());
    }
    if features & FEATURE_SECURITY_STATUS != 0 {
        names.push("security-status".to_string());
    }
    if features & FEATURE_BUILD_INFO != 0 {
        names.push("build-info".to_string());
    }
    if features & FEATURE_TOUCH_CALIBRATION != 0 {
        names.push("touch-calibration".to_string());
    }
    if features & FEATURE_TOUCH_DIAGNOSTICS != 0 {
        names.push("touch-diagnostics".to_string());
    }
    if features & FEATURE_SEED_LABELS != 0 {
        names.push("seed-labels".to_string());
    }
    if features & FEATURE_PIN_CHANGE_UI != 0 {
        names.push("pin-change-ui".to_string());
    }
    if features & FEATURE_TOUCH_CALIBRATION_UI != 0 {
        names.push("touch-calibration-ui".to_string());
    }
    if features & FEATURE_SECURE_UPDATE != 0 {
        names.push("secure-update".to_string());
    }
    if features & FEATURE_RELEASE_INFO != 0 {
        names.push("release-info".to_string());
    }
    if features & FEATURE_UPDATE_BOOT_STATUS != 0 {
        names.push("update-boot-status".to_string());
    }
    if features & FEATURE_DEVICE_REBOOT != 0 {
        names.push("device-reboot".to_string());
    }

    let unknown = features & !FEATURE_ALL_KNOWN;
    if unknown != 0 {
        names.push(format!("unknown:0x{unknown:08x}"));
    }

    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_formatter_names_device_reboot() {
        let names = format_features(FEATURE_DEVICE_REBOOT);
        assert_eq!(names, "device-reboot");
    }
}

fn format_path(path: &[u32]) -> String {
    let mut out = String::from("m");
    for &component in path {
        let hardened = (component & 0x8000_0000) != 0;
        let index = component & 0x7FFF_FFFF;
        out.push('/');
        let _ = write!(out, "{}", index);
        if hardened {
            out.push('\'');
        }
    }
    out
}
