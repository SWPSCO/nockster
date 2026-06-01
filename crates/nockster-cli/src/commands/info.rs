use crate::keys::pubkey_to_b58;
use crate::serial::{open, send_call};
use nockster_core::{
    Request, Response, FEATURE_ALL_KNOWN, FEATURE_BUILD_INFO, FEATURE_CHEETAH, FEATURE_FRAG,
    FEATURE_PIN_CHANGE_UI, FEATURE_RELEASE_INFO, FEATURE_SECURE_UPDATE, FEATURE_SECURITY_STATUS,
    FEATURE_SEED_LABELS, FEATURE_TOUCH_CALIBRATION, FEATURE_TOUCH_CALIBRATION_UI,
    FEATURE_TOUCH_DIAGNOSTICS, FEATURE_UPDATE_BOOT_STATUS, FEATURE_XPUB,
};
use std::fmt::Write as _;

pub fn run(port: &str, baud: u32, version: u8) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

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
            println!(
                "info: proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}"
            );
            println!("features: {}", format_features(features));
            if features & FEATURE_BUILD_INFO != 0 {
                let build_resp: Response = send_call(&mut *sp, 0x03, Request::GetBuildInfo)?;
                match build_resp {
                    Response::OkBuildInfo(build) => {
                        let dirty = if build.git_dirty { "-dirty" } else { "" };
                        println!(
                            "build: profile={}, protocol_v={}, git={}{}",
                            build.build_profile, build.protocol_v, build.git_commit, dirty
                        );
                        println!("tx-types: {}", build.tx_types_rev);
                    }
                    other => anyhow::bail!("unexpected build info response: {other:?}"),
                }
            }
            if features & FEATURE_RELEASE_INFO != 0 {
                let release_resp: Response = send_call(&mut *sp, 0x04, Request::GetReleaseInfo)?;
                match release_resp {
                    Response::OkReleaseInfo(release) => {
                        println!("release: version={}", release.release_version);
                    }
                    other => anyhow::bail!("unexpected release info response: {other:?}"),
                }
            }
            if has_seed {
                if cheetah_pubs.is_empty() {
                    println!("  (device locked; pubkeys withheld)");
                } else {
                    for (idx, pubinfo) in cheetah_pubs.iter().enumerate() {
                        let pk_xy = (pubinfo.x, pubinfo.y);
                        let b58 = pubkey_to_b58(&pk_xy, version);
                        let path_display = format_path(pubinfo.path.as_slice());
                        println!(
                            "  slot[{slot}] key[{idx:02}]: path={} pubkey(v{})={}",
                            path_display,
                            version,
                            b58,
                            slot = pubinfo.slot
                        );
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
            println!(
                "status: {}",
                if locked {
                    "🔒 locked"
                } else {
                    "🔓 unlocked"
                }
            );
            println!("attempts remaining: {}", attempts_remaining);
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
