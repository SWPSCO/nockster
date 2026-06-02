use crate::cli::{
    UpdateArgs, UpdateCmd, UpdateDeviceInstallArgs, UpdateDeviceStreamVerifyArgs,
    UpdateDeviceVerifyArgs, UpdateIndexArgs, UpdateKeygenArgs, UpdatePubkeyArgs, UpdateSignArgs,
    UpdateStatusArgs, UpdateTrustArgs, UpdateVerifyArgs,
};
use crate::commands::reboot::request_device_reboot;
use crate::serial::{open, send_call, Link};
use crate::ui;
use anyhow::{anyhow, Context, Result};
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use nockster_core::update::{
    pubkey_sha256, update_manifest_digest, verify_update_bundle_signature,
    verify_update_image_digest, verify_update_manifest_policy, UpdateManifest,
    UpdateManifestPolicy, UpdateManifestPolicyError, MAX_UPDATE_CHUNK_LEN,
    UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47, UPDATE_MANIFEST_VERSION,
    UPDATE_SIGNATURE_SCHEME,
};
use nockster_core::{
    Request, Response, UpdateBootStatus, UpdateStatus, ERR_UNSUPPORTED_VERSION,
    UPDATE_OTA_STATE_ABORTED, UPDATE_OTA_STATE_INVALID, UPDATE_OTA_STATE_NEW,
    UPDATE_OTA_STATE_PENDING_VERIFY, UPDATE_OTA_STATE_UNAVAILABLE, UPDATE_OTA_STATE_UNDEFINED,
    UPDATE_OTA_STATE_UNKNOWN, UPDATE_OTA_STATE_VALID, UPDATE_SLOT_NONE, UPDATE_SLOT_OTA0,
    UPDATE_SLOT_OTA1, UPDATE_SLOT_UNKNOWN,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use zeroize::Zeroize;

#[derive(Debug, Serialize, Deserialize)]
struct UpdateBundleJson {
    format: String,
    signature_scheme: String,
    manifest: UpdateManifestJson,
    signing_pubkey_sec1_hex: String,
    signature_hex: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateManifestJson {
    manifest_version: u8,
    release_version: u32,
    image_size: u32,
    image_sha256_hex: String,
    signing_pubkey_sha256_hex: String,
    hardware_target: String,
    build_profile: String,
    protocol_v: u8,
    git_commit: String,
    tx_types_rev: String,
}

#[derive(Debug, Serialize)]
struct UpdateReleaseIndexJson {
    format: &'static str,
    bundle_url: String,
    firmware_url: String,
    release_version: u32,
    image_size: u32,
    image_sha256_hex: String,
    hardware_target: String,
    build_profile: String,
    protocol_v: u8,
    git_commit: String,
    tx_types_rev: String,
}

pub fn run(args: &UpdateArgs) -> Result<()> {
    match &args.cmd {
        UpdateCmd::Keygen(args) => keygen(args),
        UpdateCmd::Sign(args) => sign(args),
        UpdateCmd::Index(args) => index(args),
        UpdateCmd::Verify(args) => verify(args),
        UpdateCmd::Trust(args) => trust(args),
        UpdateCmd::DeviceVerify(args) => device_verify(args),
        UpdateCmd::Status(args) => update_status(args),
        UpdateCmd::DeviceStreamVerify(args) => device_stream_verify(args),
        UpdateCmd::DeviceInstall(args) => device_install(args),
        UpdateCmd::Pubkey(args) => pubkey(args),
    }
}

fn keygen(args: &UpdateKeygenArgs) -> Result<()> {
    let mut key_bytes = [0u8; 32];
    let signing_key = loop {
        OsRng.fill_bytes(&mut key_bytes);
        if let Ok(signing_key) = SigningKey::from_bytes((&key_bytes).into()) {
            break signing_key;
        }
        key_bytes.zeroize();
    };

    let write_result = write_secret_key_file(&args.out, &key_bytes, args.raw);
    key_bytes.zeroize();
    write_result?;

    let verifying_key = signing_key.verifying_key();
    let pubkey_sec1 = verifying_key.to_encoded_point(true);
    let pubkey_bytes = pubkey_sec1.as_bytes();

    ui::header("update keygen");
    ui::kv("format", ui::strong(if args.raw { "raw" } else { "hex" }));
    ui::kv("pubkey sec1", ui::dim(&hex::encode(pubkey_bytes)));
    ui::kv(
        "trusted hash",
        ui::accent(&hex::encode(pubkey_sha256(pubkey_bytes))),
    );
    ui::ok(&format!("wrote release signing key: {}", args.out.display()));
    Ok(())
}

fn sign(args: &UpdateSignArgs) -> Result<()> {
    let mut key_bytes = read_secret_key_file(&args.signing_key_file)?;
    let signing_key = SigningKey::from_bytes((&key_bytes).into());
    key_bytes.zeroize();
    let signing_key = signing_key.map_err(|_| anyhow!("invalid release signing key"))?;
    let verifying_key = signing_key.verifying_key();
    let pubkey_sec1 = verifying_key.to_encoded_point(true);
    let pubkey_bytes = pubkey_sec1.as_bytes();
    let trusted_hash = pubkey_sha256(pubkey_bytes);

    let (image_sha256, image_size) = file_sha256(&args.firmware)?;
    let image_size_u32 =
        u32::try_from(image_size).map_err(|_| anyhow!("firmware image is too large"))?;
    let release_metadata = resolve_release_metadata(args)?;

    let manifest = UpdateManifest::new(
        args.release_version,
        image_size_u32,
        image_sha256,
        trusted_hash,
        &args.hardware_target,
        &args.build_profile,
        args.protocol_v,
        &release_metadata.git_commit,
        &release_metadata.tx_types_rev,
    )
    .map_err(|e| anyhow!("invalid update manifest: {e:?}"))?;

    let digest = update_manifest_digest(&manifest)
        .map_err(|e| anyhow!("failed to encode update manifest: {e:?}"))?;
    let signature: Signature = signing_key
        .sign_prehash(&digest)
        .map_err(|e| anyhow!("failed to sign update manifest: {e}"))?;
    let signature_bytes = signature.to_bytes();

    let bundle = UpdateBundleJson {
        format: "nockster-update-bundle-v1".to_string(),
        signature_scheme: UPDATE_SIGNATURE_SCHEME.to_string(),
        manifest: manifest_to_json(&manifest),
        signing_pubkey_sec1_hex: hex::encode(pubkey_bytes),
        signature_hex: hex::encode(signature_bytes),
    };

    let json = serde_json::to_string_pretty(&bundle)?;
    fs::write(&args.out, json).with_context(|| format!("write {}", args.out.display()))?;

    ui::header("update sign");
    ui::kv("pubkey sec1", ui::dim(&hex::encode(pubkey_bytes)));
    ui::kv("trusted hash", ui::accent(&hex::encode(trusted_hash)));
    ui::kv("git commit", ui::strong(&release_metadata.git_commit));
    ui::kv("tx-types", ui::dim(&release_metadata.tx_types_rev));
    ui::ok(&format!("wrote update bundle: {}", args.out.display()));
    Ok(())
}

fn index(args: &UpdateIndexArgs) -> Result<()> {
    let (manifest, _, _) = read_bundle(&args.bundle)?;
    verify_firmware_file_matches_manifest(&args.firmware, &manifest)?;
    let bundle_url = resolve_artifact_url(args.bundle_url.as_deref(), &args.bundle, "bundle")?;
    let firmware_url =
        resolve_artifact_url(args.firmware_url.as_deref(), &args.firmware, "firmware")?;
    let index = release_index_from_manifest(&manifest, bundle_url, firmware_url);
    let json = serde_json::to_string_pretty(&index)?;
    fs::write(&args.out, json).with_context(|| format!("write {}", args.out.display()))?;

    ui::header("update index");
    ui::kv("release", ui::strong(&manifest.release_version.to_string()));
    ui::kv("bundle url", ui::accent(&index.bundle_url));
    ui::kv("firmware url", ui::accent(&index.firmware_url));
    ui::ok(&format!("wrote release index: {}", args.out.display()));
    Ok(())
}

fn verify(args: &UpdateVerifyArgs) -> Result<()> {
    let (manifest, bundled_pubkey, signature) = read_bundle(&args.bundle)?;
    let trusted_hash = parse_hex_32(&args.trusted_pubkey_sha256, "trusted pubkey hash")?;
    let (image_sha256, image_size) = file_sha256(&args.firmware)?;
    let image_size_u32 =
        u32::try_from(image_size).map_err(|_| anyhow!("firmware image is too large"))?;

    verify_update_image_digest(&manifest, &image_sha256, image_size_u32)
        .map_err(|e| anyhow!("image digest check failed: {e:?}"))?;
    verify_update_bundle_signature(&manifest, &signature, &bundled_pubkey, &trusted_hash)
        .map_err(|e| anyhow!("bundle signature check failed: {e:?}"))?;

    ui::header("update verify");
    ui::kv("release", ui::strong(&manifest.release_version.to_string()));
    ui::kv("image sha256", ui::dim(&hex::encode(manifest.image_sha256)));
    ui::kv("trusted hash", ui::accent(&hex::encode(trusted_hash)));
    ui::ok("update bundle verified");
    Ok(())
}

fn trust(args: &UpdateTrustArgs) -> Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    match send_call(&mut *sp, 0x5500, Request::GetUpdateTrust)? {
        Response::OkUpdateTrust(trust) => {
            ui::header("update trust");
            ui::kv("configured", ui::yesno(trust.configured));
            ui::kv("trusted hash", ui::accent(&hex::encode(trust.pubkey_sha256)));
            Ok(())
        }
        other => Err(anyhow!("unexpected update trust response: {other:?}")),
    }
}

fn device_verify(args: &UpdateDeviceVerifyArgs) -> Result<()> {
    let (manifest, bundled_pubkey, signature) = read_bundle(&args.bundle)?;
    let mut sp = open(&args.port, args.baud)?;
    ui::header("update device-verify");
    preflight_device_update_policy(&mut *sp, &manifest)?;
    match send_call(
        &mut *sp,
        0x5501,
        Request::VerifyUpdateManifest {
            manifest,
            signature64: signature,
            signing_pubkey_sec1: bundled_pubkey,
        },
    )? {
        Response::Ok => {
            ui::ok("device accepted update manifest signature");
            Ok(())
        }
        Response::Err { code } => Err(anyhow!("device rejected update manifest: code {code}")),
        other => Err(anyhow!("unexpected update verify response: {other:?}")),
    }
}

fn update_status(args: &UpdateStatusArgs) -> Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    ui::header("update status");
    let status = expect_update_status(
        send_call(&mut *sp, 0x5502, Request::GetUpdateStatus)?,
        "read update status",
    )?;
    print_update_status(&status);
    validate_stream_status(&status, args)?;

    let mut boot_status = None;
    match send_call(&mut *sp, 0x5503, Request::GetUpdateBootStatus)? {
        Response::OkUpdateBootStatus(status) => {
            print_update_boot_status(&status);
            boot_status = Some(status);
        }
        Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        } => ui::note("update boot: unsupported by firmware"),
        Response::Err { code } => {
            return Err(anyhow!(
                "read update boot status: device returned error code {code}"
            ));
        }
        other => {
            return Err(anyhow!(
                "read update boot status: unexpected response: {other:?}"
            ))
        }
    }

    validate_boot_status(boot_status.as_ref(), args)?;
    Ok(())
}

fn device_stream_verify(args: &UpdateDeviceStreamVerifyArgs) -> Result<()> {
    stream_update_to_device(
        &args.port,
        args.baud,
        &args.bundle,
        &args.firmware,
        args.chunk_size,
        false,
        false,
    )
}

fn device_install(args: &UpdateDeviceInstallArgs) -> Result<()> {
    stream_update_to_device(
        &args.port,
        args.baud,
        &args.bundle,
        &args.firmware,
        args.chunk_size,
        true,
        args.reboot,
    )
}

fn stream_update_to_device(
    port: &str,
    baud: u32,
    bundle: &Path,
    firmware: &Path,
    chunk_size: usize,
    write_flash: bool,
    reboot_after_install: bool,
) -> Result<()> {
    if chunk_size == 0 || chunk_size > MAX_UPDATE_CHUNK_LEN {
        return Err(anyhow!(
            "--chunk-size must be between 1 and {MAX_UPDATE_CHUNK_LEN}"
        ));
    }

    let (manifest, bundled_pubkey, signature) = read_bundle(bundle)?;
    verify_firmware_file_matches_manifest(firmware, &manifest)?;
    let mut sp = open(port, baud)?;
    ui::header(if write_flash {
        "update install"
    } else {
        "update stream-verify"
    });
    preflight_device_update_policy(&mut *sp, &manifest)?;

    let begin = expect_update_status(
        send_call(
            &mut *sp,
            0x5510,
            Request::BeginUpdate {
                manifest: manifest.clone(),
                signature64: signature,
                signing_pubkey_sec1: bundled_pubkey,
                write_flash,
            },
        )?,
        "begin update stream",
    )?;
    validate_stream_begin_status(&begin, &manifest)?;
    if write_flash {
        ui::ok(&format!(
            "device accepted update install: release {} · {} bytes",
            begin.release_version, begin.image_size
        ));
    } else {
        ui::ok(&format!(
            "device accepted update manifest: release {} · {} bytes",
            begin.release_version, begin.image_size
        ));
    }

    let progress = ui::Progress::new(
        if write_flash { "flashing" } else { "verifying" },
        begin.image_size as u64,
    );
    progress.set(0);

    let mut file =
        fs::File::open(firmware).with_context(|| format!("open {}", firmware.display()))?;
    let mut offset = 0u32;
    let mut buf = vec![0u8; chunk_size];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", firmware.display()))?;
        if n == 0 {
            break;
        }
        let expected_offset = offset
            .checked_add(n as u32)
            .ok_or_else(|| anyhow!("update stream offset overflow"))?;
        let chunk = buf[..n].to_vec();
        let status = match send_call(&mut *sp, 0x5511, Request::UpdateChunk { offset, chunk })? {
            Response::OkUpdateStatus(status) => status,
            Response::Err { code } => {
                let _ = send_call(&mut *sp, 0x5513, Request::CancelUpdate);
                return Err(anyhow!("device rejected update chunk: code {code}"));
            }
            other => {
                let _ = send_call(&mut *sp, 0x5513, Request::CancelUpdate);
                return Err(anyhow!("unexpected update chunk response: {other:?}"));
            }
        };
        if let Err(err) = validate_stream_chunk_status(&status, &manifest, expected_offset) {
            progress.done();
            let _ = send_call(&mut *sp, 0x5513, Request::CancelUpdate);
            return Err(err);
        }
        offset = status.bytes_received;
        progress.set(offset as u64);
    }
    progress.done();

    let finish = expect_update_status(
        send_call(&mut *sp, 0x5512, Request::FinishUpdate)?,
        "finish update stream",
    )?;
    validate_stream_finish_status(&finish, &manifest)?;

    if write_flash {
        ui::ok("device installed and activated streamed firmware image");
        confirm_install_activation(&mut *sp)?;
    } else {
        ui::ok("device verified streamed firmware image");
    }
    ui::kv("release", ui::strong(&finish.release_version.to_string()));
    ui::kv("image size", ui::strong(&finish.image_size.to_string()));
    ui::kv("bytes received", ui::strong(&finish.bytes_received.to_string()));
    if write_flash {
        if reboot_after_install {
            request_device_reboot(&mut *sp)?;
        } else {
            ui::info("reboot the device to boot the new OTA slot");
        }
    }
    Ok(())
}

fn confirm_install_activation(sp: &mut dyn Link) -> Result<()> {
    let status = match send_call(sp, 0x5514, Request::GetUpdateBootStatus)? {
        Response::OkUpdateBootStatus(status) => status,
        Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        } => {
            return Err(anyhow!(
                "post-install activation validation failed: update boot status is unsupported"
            ));
        }
        Response::Err { code } => {
            return Err(anyhow!(
                "post-install activation validation failed: device returned error code {code}"
            ));
        }
        other => {
            return Err(anyhow!(
                "post-install activation validation failed: unexpected response: {other:?}"
            ));
        }
    };

    print_update_boot_status(&status);
    let mut failures = Vec::new();
    if !status.partition_table_ok {
        failures.push("partition table is not readable".to_string());
    }
    if !status.ota_data_present {
        failures.push("otadata partition is missing".to_string());
    }
    if !status.ota0_present || !status.ota1_present {
        failures.push("both OTA app slots must be present".to_string());
    }
    if !matches!(status.current_slot, UPDATE_SLOT_OTA0 | UPDATE_SLOT_OTA1) {
        failures.push(format!(
            "selected boot slot is {}, expected ota_0 or ota_1",
            slot_name(status.current_slot)
        ));
    }
    if status.ota_state != UPDATE_OTA_STATE_NEW {
        failures.push(format!(
            "selected OTA image state is {}, expected new",
            ota_state_name(status.ota_state)
        ));
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "post-install activation validation failed:\n  - {}",
            failures.join("\n  - ")
        );
    }

    ui::ok("update activation: ok");
    Ok(())
}

fn pubkey(args: &UpdatePubkeyArgs) -> Result<()> {
    let mut key_bytes = read_secret_key_file(&args.signing_key_file)?;
    let signing_key = SigningKey::from_bytes((&key_bytes).into());
    key_bytes.zeroize();
    let signing_key = signing_key.map_err(|_| anyhow!("invalid release signing key"))?;
    let verifying_key = signing_key.verifying_key();
    let pubkey_sec1 = verifying_key.to_encoded_point(true);
    let pubkey_bytes = pubkey_sec1.as_bytes();

    ui::header("update pubkey");
    ui::kv("pubkey sec1", ui::dim(&hex::encode(pubkey_bytes)));
    ui::kv(
        "trusted hash",
        ui::accent(&hex::encode(pubkey_sha256(pubkey_bytes))),
    );
    Ok(())
}

fn expect_update_status(response: Response, context: &str) -> Result<UpdateStatus> {
    match response {
        Response::OkUpdateStatus(status) => Ok(status),
        Response::Err { code } => Err(anyhow!("{context}: device returned error code {code}")),
        other => Err(anyhow!("{context}: unexpected response: {other:?}")),
    }
}

fn validate_stream_begin_status(status: &UpdateStatus, manifest: &UpdateManifest) -> Result<()> {
    validate_update_status_shape(
        status,
        manifest,
        "begin update stream",
        true,
        true,
        false,
        0,
    )
}

fn validate_stream_chunk_status(
    status: &UpdateStatus,
    manifest: &UpdateManifest,
    expected_bytes_received: u32,
) -> Result<()> {
    validate_update_status_shape(
        status,
        manifest,
        "stream update chunk",
        true,
        true,
        false,
        expected_bytes_received,
    )
}

fn validate_stream_finish_status(status: &UpdateStatus, manifest: &UpdateManifest) -> Result<()> {
    validate_update_status_shape(
        status,
        manifest,
        "finish update stream",
        false,
        true,
        true,
        manifest.image_size,
    )
}

fn validate_update_status_shape(
    status: &UpdateStatus,
    manifest: &UpdateManifest,
    context: &str,
    expected_active: bool,
    expected_manifest_verified: bool,
    expected_image_verified: bool,
    expected_bytes_received: u32,
) -> Result<()> {
    let mut failures = Vec::new();
    if status.active != expected_active {
        failures.push(format!(
            "active is {}, expected {}",
            yes_no(status.active),
            yes_no(expected_active)
        ));
    }
    if status.manifest_verified != expected_manifest_verified {
        failures.push(format!(
            "manifest_verified is {}, expected {}",
            yes_no(status.manifest_verified),
            yes_no(expected_manifest_verified)
        ));
    }
    if status.image_verified != expected_image_verified {
        failures.push(format!(
            "image_verified is {}, expected {}",
            yes_no(status.image_verified),
            yes_no(expected_image_verified)
        ));
    }
    if status.release_version != manifest.release_version {
        failures.push(format!(
            "release_version is {}, expected {}",
            status.release_version, manifest.release_version
        ));
    }
    if status.image_size != manifest.image_size {
        failures.push(format!(
            "image_size is {}, expected {}",
            status.image_size, manifest.image_size
        ));
    }
    if status.bytes_received != expected_bytes_received {
        failures.push(format!(
            "bytes_received is {}, expected {}",
            status.bytes_received, expected_bytes_received
        ));
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "{context}: invalid device update status:\n  - {}",
            failures.join("\n  - ")
        );
    }

    Ok(())
}

fn print_update_status(status: &UpdateStatus) {
    ui::subhead("stream");
    ui::kv("active", ui::yesno(status.active));
    ui::kv("manifest ok", ui::yesno(status.manifest_verified));
    ui::kv("image ok", ui::yesno(status.image_verified));
    ui::kv("release", ui::strong(&status.release_version.to_string()));
    ui::kv("received", ui::strong(&status.bytes_received.to_string()));
    ui::kv("image size", ui::strong(&status.image_size.to_string()));
}

fn print_update_boot_status(status: &UpdateBootStatus) {
    ui::subhead("boot");
    ui::kv("partition tbl", ui::yesno(status.partition_table_ok));
    ui::kv("otadata", ui::yesno(status.ota_data_present));
    ui::kv(
        "ota0",
        format!(
            "{}  {}",
            ui::yesno(status.ota0_present),
            ui::dim(&format!(
                "offset=0x{:x} size={}",
                status.ota0_offset, status.ota0_size
            ))
        ),
    );
    ui::kv(
        "ota1",
        format!(
            "{}  {}",
            ui::yesno(status.ota1_present),
            ui::dim(&format!(
                "offset=0x{:x} size={}",
                status.ota1_offset, status.ota1_size
            ))
        ),
    );
    ui::kv(
        "selected",
        ui::strong(&format!(
            "current={} · next={} · state={}",
            slot_name(status.current_slot),
            slot_name(status.next_slot),
            ota_state_name(status.ota_state)
        )),
    );
}

fn validate_stream_status(status: &UpdateStatus, args: &UpdateStatusArgs) -> Result<()> {
    if args.expect_idle && status.active {
        anyhow::bail!(
            "update validation failed:\n  - expected idle update stream, but stream is active"
        );
    }
    Ok(())
}

fn validate_boot_status(status: Option<&UpdateBootStatus>, args: &UpdateStatusArgs) -> Result<()> {
    if !has_boot_expectations(args) {
        return Ok(());
    }

    let Some(status) = status else {
        anyhow::bail!(
            "update validation failed:\n  - update boot status is unsupported by this firmware"
        );
    };

    let mut failures = Vec::new();
    if args.expect_ota_ready {
        if !status.partition_table_ok {
            failures.push("partition table is not readable".to_string());
        }
        if !status.ota_data_present {
            failures.push("otadata partition is missing".to_string());
        }
        if !status.ota0_present {
            failures.push("ota_0 partition is missing".to_string());
        }
        if !status.ota1_present {
            failures.push("ota_1 partition is missing".to_string());
        }
    }

    if let Some(expected) = args.expect_current_slot.as_deref() {
        let expected = parse_slot(expected, "expected current slot")?;
        if status.current_slot != expected {
            failures.push(format!(
                "current slot is {}, expected {}",
                slot_name(status.current_slot),
                slot_name(expected)
            ));
        }
    }

    if let Some(expected) = args.expect_next_slot.as_deref() {
        let expected = parse_slot(expected, "expected next slot")?;
        if status.next_slot != expected {
            failures.push(format!(
                "next slot is {}, expected {}",
                slot_name(status.next_slot),
                slot_name(expected)
            ));
        }
    }

    if let Some(expected) = args.expect_ota_state.as_deref() {
        let expected = parse_ota_state(expected)?;
        if status.ota_state != expected {
            failures.push(format!(
                "OTA image state is {}, expected {}",
                ota_state_name(status.ota_state),
                ota_state_name(expected)
            ));
        }
    }

    if !failures.is_empty() {
        anyhow::bail!("update validation failed:\n  - {}", failures.join("\n  - "));
    }

    ui::ok("update validation: ok");
    Ok(())
}

fn has_boot_expectations(args: &UpdateStatusArgs) -> bool {
    args.require_boot_status
        || args.expect_ota_ready
        || args.expect_current_slot.is_some()
        || args.expect_next_slot.is_some()
        || args.expect_ota_state.is_some()
}

fn parse_slot(value: &str, label: &str) -> Result<u8> {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', '_'], "");
    match normalized.as_str() {
        "factory" | "none" => Ok(UPDATE_SLOT_NONE),
        "ota0" | "slot0" | "0" => Ok(UPDATE_SLOT_OTA0),
        "ota1" | "slot1" | "1" => Ok(UPDATE_SLOT_OTA1),
        "unknown" => Ok(UPDATE_SLOT_UNKNOWN),
        _ => Err(anyhow!(
            "{label} must be one of factory, none, ota0, ota1, or unknown"
        )),
    }
}

fn parse_ota_state(value: &str) -> Result<u8> {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', '_'], "");
    match normalized.as_str() {
        "new" => Ok(UPDATE_OTA_STATE_NEW),
        "pendingverify" | "pending" => Ok(UPDATE_OTA_STATE_PENDING_VERIFY),
        "valid" => Ok(UPDATE_OTA_STATE_VALID),
        "invalid" => Ok(UPDATE_OTA_STATE_INVALID),
        "aborted" => Ok(UPDATE_OTA_STATE_ABORTED),
        "unavailable" => Ok(UPDATE_OTA_STATE_UNAVAILABLE),
        "unknown" => Ok(UPDATE_OTA_STATE_UNKNOWN),
        "undefined" => Ok(UPDATE_OTA_STATE_UNDEFINED),
        _ => Err(anyhow!(
            "--expect-ota-state must be one of new, pending-verify, valid, invalid, aborted, undefined, unavailable, or unknown"
        )),
    }
}

fn slot_name(slot: u8) -> &'static str {
    match slot {
        UPDATE_SLOT_NONE => "factory/none",
        UPDATE_SLOT_OTA0 => "ota_0",
        UPDATE_SLOT_OTA1 => "ota_1",
        UPDATE_SLOT_UNKNOWN => "unknown",
        _ => "invalid",
    }
}

fn ota_state_name(state: u8) -> &'static str {
    match state {
        UPDATE_OTA_STATE_NEW => "new",
        UPDATE_OTA_STATE_PENDING_VERIFY => "pending-verify",
        UPDATE_OTA_STATE_VALID => "valid",
        UPDATE_OTA_STATE_INVALID => "invalid",
        UPDATE_OTA_STATE_ABORTED => "aborted",
        UPDATE_OTA_STATE_UNAVAILABLE => "unavailable",
        UPDATE_OTA_STATE_UNKNOWN => "unknown",
        UPDATE_OTA_STATE_UNDEFINED => "undefined",
        _ => "invalid",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

struct ReleaseMetadata {
    git_commit: String,
    tx_types_rev: String,
}

fn resolve_release_metadata(args: &UpdateSignArgs) -> Result<ReleaseMetadata> {
    let git_commit = match args.git_commit.as_deref() {
        Some(value) => value.to_string(),
        None => {
            let commit = git_output(["rev-parse", "HEAD"])
                .ok_or_else(|| anyhow!("could not derive --git-commit; pass it explicitly"))?;
            if git_dirty() {
                ui::warn(
                    "working tree has uncommitted changes; bundle git_commit records HEAD only",
                );
            }
            commit
        }
    };

    let tx_types_rev = match args.tx_types_rev.as_deref() {
        Some(value) => value.to_string(),
        None => {
            let manifest = find_workspace_manifest_with_tx_types().ok_or_else(|| {
                anyhow!("could not derive --tx-types-rev from Cargo.toml; pass it explicitly")
            })?;
            tx_types_rev_from_manifest(&manifest).ok_or_else(|| {
                anyhow!(
                    "could not derive --tx-types-rev from {}; pass it explicitly",
                    manifest.display()
                )
            })?
        }
    };

    Ok(ReleaseMetadata {
        git_commit,
        tx_types_rev,
    })
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_string())
}

fn git_dirty() -> bool {
    let tracked_dirty = Command::new("git")
        .args(["diff", "--quiet", "--ignore-submodules"])
        .status()
        .map(|status| !status.success())
        .unwrap_or(false);

    let staged_dirty = Command::new("git")
        .args(["diff", "--cached", "--quiet", "--ignore-submodules"])
        .status()
        .map(|status| !status.success())
        .unwrap_or(false);

    tracked_dirty || staged_dirty
}

fn find_workspace_manifest_with_tx_types() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("Cargo.toml");
        if tx_types_rev_from_manifest(&candidate).is_some() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn tx_types_rev_from_manifest(path: &Path) -> Option<String> {
    let workspace = fs::read_to_string(path).ok()?;
    tx_types_rev_from_toml(&workspace)
}

fn tx_types_rev_from_toml(workspace: &str) -> Option<String> {
    for line in workspace.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("tx-types =") {
            continue;
        }
        let rev_start = trimmed.find("rev = \"")? + "rev = \"".len();
        let rev_tail = &trimmed[rev_start..];
        let rev_end = rev_tail.find('"')?;
        return Some(rev_tail[..rev_end].to_string());
    }
    None
}

fn verify_firmware_file_matches_manifest(firmware: &Path, manifest: &UpdateManifest) -> Result<()> {
    let (image_sha256, image_size) = file_sha256(firmware)?;
    let image_size_u32 =
        u32::try_from(image_size).map_err(|_| anyhow!("firmware image is too large"))?;
    verify_update_image_digest(manifest, &image_sha256, image_size_u32)
        .map_err(|e| anyhow!("firmware file does not match signed manifest: {e:?}"))
}

fn preflight_device_update_policy(
    sp: &mut dyn crate::serial::Link,
    manifest: &UpdateManifest,
) -> Result<()> {
    let release_version = read_device_release_version(sp)?;
    let build_info = read_device_build_info(sp)?;

    match (release_version, build_info) {
        (Some(current_release), Some((build_profile, protocol_v))) => {
            let policy = UpdateManifestPolicy {
                current_release_version: current_release,
                hardware_target: UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
                current_build_profile: &build_profile,
                protocol_v,
            };
            verify_update_manifest_policy(manifest, &policy).map_err(|err| {
                update_policy_preflight_error(
                    err,
                    manifest,
                    Some(current_release),
                    Some((&build_profile, protocol_v)),
                )
            })?;
            ui::kv(
                "preflight",
                ui::dim(&format!(
                    "release {current_release}→{} · profile {build_profile}→{} · proto {protocol_v}",
                    manifest.release_version,
                    manifest.build_profile.as_str(),
                )),
            );
        }
        (Some(current_release), None) => {
            validate_release_advances(manifest.release_version, current_release)?;
            ui::kv(
                "preflight",
                ui::dim(&format!(
                    "release {current_release}→{}",
                    manifest.release_version
                )),
            );
        }
        (None, Some((build_profile, protocol_v))) => {
            validate_update_manifest_compatibility(manifest, &build_profile, protocol_v).map_err(
                |err| {
                    update_policy_preflight_error(
                        err,
                        manifest,
                        None,
                        Some((&build_profile, protocol_v)),
                    )
                },
            )?;
            ui::warn(
                "device does not report release info; firmware will still enforce rollback policy",
            );
            ui::kv(
                "preflight",
                ui::dim(&format!(
                    "profile {build_profile}→{} · proto {protocol_v}",
                    manifest.build_profile.as_str(),
                )),
            );
        }
        (None, None) => {
            ui::warn(
                "device does not report release/build info; firmware will still enforce update policy",
            );
        }
    }

    Ok(())
}

fn read_device_release_version(sp: &mut dyn crate::serial::Link) -> Result<Option<u32>> {
    match send_call(sp, 0x550f, Request::GetReleaseInfo)? {
        Response::OkReleaseInfo(release) => Ok(Some(release.release_version)),
        Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        } => Ok(None),
        Response::Err { code } => Err(anyhow!(
            "device release preflight failed: device returned error code {code}"
        )),
        other => Err(anyhow!(
            "device release preflight failed: unexpected response: {other:?}"
        )),
    }
}

fn read_device_build_info(sp: &mut dyn crate::serial::Link) -> Result<Option<(String, u8)>> {
    match send_call(sp, 0x5514, Request::GetBuildInfo)? {
        Response::OkBuildInfo(build) => Ok(Some((
            build.build_profile.as_str().to_string(),
            build.protocol_v,
        ))),
        Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        } => Ok(None),
        Response::Err { code } => Err(anyhow!(
            "device build preflight failed: device returned error code {code}"
        )),
        other => Err(anyhow!(
            "device build preflight failed: unexpected response: {other:?}"
        )),
    }
}

fn validate_release_advances(bundle_release: u32, device_release: u32) -> Result<()> {
    if bundle_release <= device_release {
        return Err(anyhow!(
            "update rollback blocked: bundle release_version {bundle_release} is not newer than device release_version {device_release}"
        ));
    }
    Ok(())
}

fn validate_update_manifest_compatibility(
    manifest: &UpdateManifest,
    device_build_profile: &str,
    device_protocol_v: u8,
) -> Result<(), UpdateManifestPolicyError> {
    let release_floor = manifest.release_version.saturating_sub(1);
    let policy = UpdateManifestPolicy {
        current_release_version: release_floor,
        hardware_target: UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
        current_build_profile: device_build_profile,
        protocol_v: device_protocol_v,
    };
    match verify_update_manifest_policy(manifest, &policy) {
        Ok(()) => Ok(()),
        Err(UpdateManifestPolicyError::RollbackVersion) if manifest.release_version == 0 => {
            Err(UpdateManifestPolicyError::UnsupportedManifest)
        }
        Err(UpdateManifestPolicyError::RollbackVersion) => Ok(()),
        Err(err) => Err(err),
    }
}

fn update_policy_preflight_error(
    err: UpdateManifestPolicyError,
    manifest: &UpdateManifest,
    device_release: Option<u32>,
    device_build: Option<(&str, u8)>,
) -> anyhow::Error {
    match err {
        UpdateManifestPolicyError::RollbackVersion => {
            let current = device_release.unwrap_or(0);
            anyhow!(
                "update rollback blocked: bundle release_version {} is not newer than device release_version {}",
                manifest.release_version,
                current
            )
        }
        UpdateManifestPolicyError::UnsupportedManifest => {
            let (device_profile, device_protocol) = device_build.unwrap_or(("unknown", 0));
            anyhow!(
                "update bundle is incompatible with this device: bundle target={}, profile={}, protocol_v={}, image_size={} bytes; device expected target={}, profile={}, protocol_v={}",
                manifest.hardware_target.as_str(),
                manifest.build_profile.as_str(),
                manifest.protocol_v,
                manifest.image_size,
                UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
                device_profile,
                device_protocol
            )
        }
    }
}

fn manifest_to_json(manifest: &UpdateManifest) -> UpdateManifestJson {
    UpdateManifestJson {
        manifest_version: manifest.manifest_version,
        release_version: manifest.release_version,
        image_size: manifest.image_size,
        image_sha256_hex: hex::encode(manifest.image_sha256),
        signing_pubkey_sha256_hex: hex::encode(manifest.signing_pubkey_sha256),
        hardware_target: manifest.hardware_target.as_str().to_string(),
        build_profile: manifest.build_profile.as_str().to_string(),
        protocol_v: manifest.protocol_v,
        git_commit: manifest.git_commit.as_str().to_string(),
        tx_types_rev: manifest.tx_types_rev.as_str().to_string(),
    }
}

fn release_index_from_manifest(
    manifest: &UpdateManifest,
    bundle_url: String,
    firmware_url: String,
) -> UpdateReleaseIndexJson {
    UpdateReleaseIndexJson {
        format: "nockster-release-index-v1",
        bundle_url,
        firmware_url,
        release_version: manifest.release_version,
        image_size: manifest.image_size,
        image_sha256_hex: hex::encode(manifest.image_sha256),
        hardware_target: manifest.hardware_target.as_str().to_string(),
        build_profile: manifest.build_profile.as_str().to_string(),
        protocol_v: manifest.protocol_v,
        git_commit: manifest.git_commit.as_str().to_string(),
        tx_types_rev: manifest.tx_types_rev.as_str().to_string(),
    }
}

fn default_artifact_url(path: &Path, label: &str) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("{label} path must include a UTF-8 file name"))?;
    if name.trim().is_empty() {
        anyhow::bail!("{label} path must include a non-empty file name");
    }
    Ok(name.to_string())
}

fn explicit_url_scheme(value: &str) -> Option<&str> {
    let scheme_end = value.find(':')?;
    let first_delim = value
        .find(|c| matches!(c, '/' | '?' | '#'))
        .unwrap_or(value.len());
    if scheme_end > first_delim {
        return None;
    }
    let scheme = &value[..scheme_end];
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
    {
        return None;
    }
    Some(scheme)
}

fn http_url_host(value: &str, label: &str) -> Result<String> {
    let Some((_, rest)) = value.split_once("://") else {
        anyhow::bail!("{label} URL must include // after the scheme");
    };
    let authority = rest
        .split(|c| matches!(c, '/' | '?' | '#'))
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        anyhow::bail!("{label} URL must include a host");
    }
    if authority.contains('@') {
        anyhow::bail!("{label} URL must not include credentials");
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            anyhow::bail!("{label} URL has an invalid IPv6 host");
        };
        return Ok(rest[..end].to_ascii_lowercase());
    }
    Ok(authority
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase())
}

fn is_local_update_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn validate_browser_artifact_url(value: &str, label: &str) -> Result<()> {
    if value.starts_with("//") {
        anyhow::bail!("{label} URL must not be protocol-relative; use an explicit https:// URL");
    }

    let Some(scheme) = explicit_url_scheme(value) else {
        return Ok(());
    };
    match scheme.to_ascii_lowercase().as_str() {
        "https" => {
            http_url_host(value, label)?;
            Ok(())
        }
        "http" => {
            let host = http_url_host(value, label)?;
            if is_local_update_host(&host) {
                Ok(())
            } else {
                anyhow::bail!("{label} URL must use HTTPS, except for localhost testing");
            }
        }
        _ => anyhow::bail!("{label} URL must use http or https"),
    }
}

fn resolve_artifact_url(value: Option<&str>, path: &Path, label: &str) -> Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() => {
            let value = value.trim().to_string();
            validate_browser_artifact_url(&value, label)?;
            Ok(value)
        }
        Some(_) => Err(anyhow!("{label} URL must be non-empty")),
        None => default_artifact_url(path, label),
    }
}

fn manifest_from_json(json: &UpdateManifestJson) -> Result<UpdateManifest> {
    if json.manifest_version != UPDATE_MANIFEST_VERSION {
        return Err(anyhow!(
            "unsupported update manifest version: {}",
            json.manifest_version
        ));
    }

    let image_sha256 = parse_hex_32(&json.image_sha256_hex, "image sha256")?;
    let signing_pubkey_sha256 =
        parse_hex_32(&json.signing_pubkey_sha256_hex, "signing pubkey sha256")?;
    UpdateManifest::new(
        json.release_version,
        json.image_size,
        image_sha256,
        signing_pubkey_sha256,
        &json.hardware_target,
        &json.build_profile,
        json.protocol_v,
        &json.git_commit,
        &json.tx_types_rev,
    )
    .map_err(|e| anyhow!("invalid update manifest: {e:?}"))
}

fn read_bundle(path: &Path) -> Result<(UpdateManifest, Vec<u8>, [u8; 64])> {
    let bundle_data =
        fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let bundle: UpdateBundleJson =
        serde_json::from_str(&bundle_data).with_context(|| format!("parse {}", path.display()))?;

    if bundle.format != "nockster-update-bundle-v1" {
        return Err(anyhow!(
            "unsupported update bundle format: {}",
            bundle.format
        ));
    }
    if bundle.signature_scheme != UPDATE_SIGNATURE_SCHEME {
        return Err(anyhow!(
            "unsupported update signature scheme: {}",
            bundle.signature_scheme
        ));
    }

    let manifest = manifest_from_json(&bundle.manifest)?;
    let bundled_pubkey =
        parse_compressed_sec1_pubkey(&bundle.signing_pubkey_sec1_hex, "signing pubkey")?;
    let signature = parse_hex_64(&bundle.signature_hex, "signature")?;
    Ok((manifest, bundled_pubkey, signature))
}

fn file_sha256(path: &Path) -> Result<([u8; 32], u64)> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut h = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0u8; 8192];

    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        h.update(&buf[..n]);
    }

    let digest = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok((out, total))
}

fn read_secret_key_file(path: &Path) -> Result<[u8; 32]> {
    reject_repo_local_secret_path(path)?;
    let mut data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if data.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(&data);
        data.zeroize();
        return Ok(out);
    }

    let parsed = match std::str::from_utf8(&data) {
        Ok(text) => parse_secret_key_hex(text),
        Err(err) => Err(err)
            .with_context(|| format!("{} is not raw 32-byte key or UTF-8 hex", path.display())),
    };
    data.zeroize();
    parsed
}

fn write_secret_key_file(path: &Path, key: &[u8; 32], raw: bool) -> Result<()> {
    reject_repo_local_secret_path(path)?;
    prepare_secret_key_parent(path)?;
    let mut file = create_new_secret_file(path)?;
    if raw {
        file.write_all(key)
            .with_context(|| format!("write {}", path.display()))?;
    } else {
        let mut hex_buf = [0u8; 65];
        encode_hex_key(key, &mut hex_buf);
        let write_result = file
            .write_all(&hex_buf)
            .with_context(|| format!("write {}", path.display()));
        hex_buf.zeroize();
        write_result?;
    }
    file.sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    Ok(())
}

fn reject_repo_local_secret_path(path: &Path) -> Result<()> {
    let Some(repo_root) = git_output(["rev-parse", "--show-toplevel"]) else {
        return Ok(());
    };

    let repo_root = normalize_existing_path(Path::new(&repo_root))?;
    let target = normalize_output_path(path)?;
    if path_starts_with(&target, &repo_root) {
        anyhow::bail!(
            "release signing key path must live outside the repo: {}",
            target.display()
        );
    }
    Ok(())
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("resolve {}", path.display()))
}

fn normalize_output_path(path: &Path) -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("read current directory")?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    if absolute.exists() {
        return normalize_existing_path(&absolute);
    }

    let file_name = absolute
        .file_name()
        .ok_or_else(|| anyhow!("release signing key path must include a file name"))?;
    let mut probe = absolute
        .parent()
        .ok_or_else(|| anyhow!("release signing key path must include a parent directory"))?
        .to_path_buf();
    let mut missing = Vec::new();
    while !probe.exists() {
        let name = probe
            .file_name()
            .ok_or_else(|| anyhow!("could not resolve {}", absolute.display()))?
            .to_owned();
        missing.push(name);
        if !probe.pop() {
            anyhow::bail!("could not resolve {}", absolute.display());
        }
    }

    let mut resolved = normalize_existing_path(&probe)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    resolved.push(file_name);
    Ok(resolved)
}

fn path_starts_with(path: &Path, base: &Path) -> bool {
    path == base || path.starts_with(base)
}

fn prepare_secret_key_parent(path: &Path) -> Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    let existed = parent.exists();
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    secure_secret_dir(parent, existed)
}

#[cfg(unix)]
fn secure_secret_dir(path: &Path, existed: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if existed {
        let mode = fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            ui::warn(&format!(
                "{} is accessible by group/other; keep release keys outside shared directories",
                path.display()
            ));
        }
        return Ok(());
    }

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod 0700 {}", path.display()))
}

#[cfg(not(unix))]
fn secure_secret_dir(_path: &Path, _existed: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn create_new_secret_file(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("create {}", path.display()))
}

#[cfg(not(unix))]
fn create_new_secret_file(path: &Path) -> Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))
}

fn encode_hex_key(key: &[u8; 32], out: &mut [u8; 65]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (idx, byte) in key.iter().copied().enumerate() {
        out[idx * 2] = HEX[(byte >> 4) as usize];
        out[idx * 2 + 1] = HEX[(byte & 0x0f) as usize];
    }
    out[64] = b'\n';
}

fn parse_secret_key_hex(value: &str) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    match fill_secret_key_hex(value, &mut out) {
        Ok(()) => Ok(out),
        Err(err) => {
            out.zeroize();
            Err(err)
        }
    }
}

fn fill_secret_key_hex(value: &str, out: &mut [u8; 32]) -> Result<()> {
    let trimmed = value.trim();
    let input = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    let mut nibbles = 0usize;

    for ch in input.chars() {
        if ch.is_whitespace() || ch == '_' || ch == ':' {
            continue;
        }
        let digit = ch
            .to_digit(16)
            .ok_or_else(|| anyhow!("parse release signing key hex"))? as u8;
        if nibbles >= out.len() * 2 {
            return Err(anyhow!("release signing key must be exactly 32 bytes"));
        }
        let byte = nibbles / 2;
        if nibbles % 2 == 0 {
            out[byte] = digit << 4;
        } else {
            out[byte] |= digit;
        }
        nibbles += 1;
    }

    if nibbles != out.len() * 2 {
        return Err(anyhow!("release signing key must be exactly 32 bytes"));
    }

    Ok(())
}

fn parse_hex_32(value: &str, label: &str) -> Result<[u8; 32]> {
    let bytes = parse_hex_bytes(value, label)?;
    if bytes.len() != 32 {
        return Err(anyhow!("{label} must be exactly 32 bytes"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_hex_64(value: &str, label: &str) -> Result<[u8; 64]> {
    let bytes = parse_hex_bytes(value, label)?;
    if bytes.len() != 64 {
        return Err(anyhow!("{label} must be exactly 64 bytes"));
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_compressed_sec1_pubkey(value: &str, label: &str) -> Result<Vec<u8>> {
    let bytes = parse_hex_bytes(value, label)?;
    if bytes.len() != 33 {
        return Err(anyhow!("{label} must be exactly 33 bytes"));
    }
    match bytes[0] {
        0x02 | 0x03 => Ok(bytes),
        _ => Err(anyhow!("{label} must be a compressed SEC1 public key")),
    }
}

fn parse_hex_bytes(value: &str, label: &str) -> Result<Vec<u8>> {
    let trimmed = value.trim();
    let trimmed = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != ':')
        .collect();
    hex::decode(cleaned).with_context(|| format!("parse {label} hex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_json() -> UpdateManifestJson {
        UpdateManifestJson {
            manifest_version: UPDATE_MANIFEST_VERSION,
            release_version: 1,
            image_size: 3,
            image_sha256_hex: hex::encode([1u8; 32]),
            signing_pubkey_sha256_hex: hex::encode([2u8; 32]),
            hardware_target: "esp32s3-touch-lcd-1.47".to_string(),
            build_profile: "production".to_string(),
            protocol_v: nockster_core::PROTO_V1,
            git_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            tx_types_rev: "abcdef0123456789abcdef0123456789abcdef01".to_string(),
        }
    }

    #[test]
    fn manifest_from_json_rejects_unknown_manifest_version() {
        let mut json = sample_manifest_json();
        json.manifest_version = UPDATE_MANIFEST_VERSION.saturating_add(1);

        let err = manifest_from_json(&json).unwrap_err().to_string();
        assert!(err.contains("unsupported update manifest version"));
    }

    #[test]
    fn manifest_from_json_preserves_signed_fields() {
        let json = sample_manifest_json();
        let manifest = manifest_from_json(&json).unwrap();

        assert_eq!(manifest.manifest_version, UPDATE_MANIFEST_VERSION);
        assert_eq!(manifest.release_version, json.release_version);
        assert_eq!(manifest.image_size, json.image_size);
        assert_eq!(manifest.image_sha256, [1u8; 32]);
        assert_eq!(manifest.signing_pubkey_sha256, [2u8; 32]);
        assert_eq!(manifest.hardware_target.as_str(), json.hardware_target);
        assert_eq!(manifest.build_profile.as_str(), json.build_profile);
        assert_eq!(manifest.protocol_v, json.protocol_v);
        assert_eq!(manifest.git_commit.as_str(), json.git_commit);
        assert_eq!(manifest.tx_types_rev.as_str(), json.tx_types_rev);
    }

    #[test]
    fn release_index_exports_browser_update_urls_and_manifest_metadata() {
        let manifest = manifest_from_json(&sample_manifest_json()).unwrap();
        let index = release_index_from_manifest(
            &manifest,
            "nockster-fw.update.json".to_string(),
            "nockster-fw.bin".to_string(),
        );

        assert_eq!(index.format, "nockster-release-index-v1");
        assert_eq!(index.bundle_url, "nockster-fw.update.json");
        assert_eq!(index.firmware_url, "nockster-fw.bin");
        assert_eq!(index.release_version, manifest.release_version);
        assert_eq!(index.image_size, manifest.image_size);
        assert_eq!(index.image_sha256_hex, hex::encode(manifest.image_sha256));
        assert_eq!(index.hardware_target, manifest.hardware_target.as_str());
        assert_eq!(index.build_profile, manifest.build_profile.as_str());
        assert_eq!(index.protocol_v, manifest.protocol_v);
        assert_eq!(index.git_commit, manifest.git_commit.as_str());
        assert_eq!(index.tx_types_rev, manifest.tx_types_rev.as_str());
    }

    #[test]
    fn default_artifact_url_uses_file_name_only() {
        assert_eq!(
            default_artifact_url(Path::new("/tmp/releases/nockster-fw.bin"), "firmware").unwrap(),
            "nockster-fw.bin"
        );
    }

    #[test]
    fn explicit_artifact_url_must_not_be_empty() {
        let err =
            resolve_artifact_url(Some("  "), Path::new("nockster-fw.bin"), "firmware").unwrap_err();
        assert!(err.to_string().contains("must be non-empty"));

        assert_eq!(
            resolve_artifact_url(
                Some(" updates/nockster-fw.bin "),
                Path::new("ignored.bin"),
                "firmware"
            )
            .unwrap(),
            "updates/nockster-fw.bin"
        );
    }

    #[test]
    fn explicit_artifact_urls_follow_browser_publication_policy() {
        assert_eq!(
            resolve_artifact_url(
                Some("https://updates.example.test/releases/nockster-fw.bin"),
                Path::new("ignored.bin"),
                "firmware"
            )
            .unwrap(),
            "https://updates.example.test/releases/nockster-fw.bin"
        );
        assert_eq!(
            resolve_artifact_url(
                Some("http://localhost:3000/updates/nockster-fw.bin"),
                Path::new("ignored.bin"),
                "firmware"
            )
            .unwrap(),
            "http://localhost:3000/updates/nockster-fw.bin"
        );
        assert_eq!(
            resolve_artifact_url(
                Some("http://[::1]:3000/updates/nockster-fw.bin"),
                Path::new("ignored.bin"),
                "firmware"
            )
            .unwrap(),
            "http://[::1]:3000/updates/nockster-fw.bin"
        );

        let remote_http = resolve_artifact_url(
            Some("http://updates.example.test/nockster-fw.bin"),
            Path::new("ignored.bin"),
            "firmware",
        )
        .unwrap_err();
        assert!(remote_http.to_string().contains("must use HTTPS"));

        let non_http = resolve_artifact_url(
            Some("file:///tmp/nockster-fw.bin"),
            Path::new("ignored.bin"),
            "firmware",
        )
        .unwrap_err();
        assert!(non_http.to_string().contains("must use http or https"));

        let protocol_relative = resolve_artifact_url(
            Some("//updates.example.test/nockster-fw.bin"),
            Path::new("ignored.bin"),
            "firmware",
        )
        .unwrap_err();
        assert!(protocol_relative
            .to_string()
            .contains("must not be protocol-relative"));
    }

    #[test]
    fn bundled_signing_pubkey_must_be_compressed_sec1() {
        let valid_even =
            parse_compressed_sec1_pubkey(&format!("02{}", "00".repeat(32)), "signing pubkey")
                .unwrap();
        assert_eq!(valid_even.len(), 33);

        let valid_odd =
            parse_compressed_sec1_pubkey(&format!("03{}", "11".repeat(32)), "signing pubkey")
                .unwrap();
        assert_eq!(valid_odd.len(), 33);

        let short = parse_compressed_sec1_pubkey("02", "signing pubkey")
            .unwrap_err()
            .to_string();
        assert!(short.contains("exactly 33 bytes"));

        let uncompressed_prefix =
            parse_compressed_sec1_pubkey(&format!("04{}", "00".repeat(32)), "signing pubkey")
                .unwrap_err()
                .to_string();
        assert!(uncompressed_prefix.contains("compressed SEC1 public key"));
    }

    #[test]
    fn release_preflight_requires_bundle_to_advance() {
        validate_release_advances(8, 7).unwrap();

        let same = validate_release_advances(7, 7).unwrap_err().to_string();
        assert!(same.contains("rollback blocked"));

        let older = validate_release_advances(6, 7).unwrap_err().to_string();
        assert!(older.contains("rollback blocked"));
    }

    #[test]
    fn device_manifest_compatibility_uses_build_metadata() {
        let manifest = manifest_from_json(&sample_manifest_json()).unwrap();

        validate_update_manifest_compatibility(&manifest, "production", nockster_core::PROTO_V1)
            .unwrap();

        let wrong_protocol =
            validate_update_manifest_compatibility(&manifest, "production", 2).unwrap_err();
        assert_eq!(
            wrong_protocol,
            UpdateManifestPolicyError::UnsupportedManifest
        );

        let non_production_device =
            validate_update_manifest_compatibility(&manifest, "dev", nockster_core::PROTO_V1);
        assert!(non_production_device.is_ok());

        let mut dev_json = sample_manifest_json();
        dev_json.build_profile = "dev".to_string();
        let dev_manifest = manifest_from_json(&dev_json).unwrap();
        let production_device = validate_update_manifest_compatibility(
            &dev_manifest,
            "production",
            nockster_core::PROTO_V1,
        )
        .unwrap_err();
        assert_eq!(
            production_device,
            UpdateManifestPolicyError::UnsupportedManifest
        );
    }

    #[test]
    fn update_stream_status_validation_requires_exact_progress() {
        let manifest = manifest_from_json(&sample_manifest_json()).unwrap();
        let begin = UpdateStatus {
            active: true,
            manifest_verified: true,
            image_verified: false,
            release_version: manifest.release_version,
            bytes_received: 0,
            image_size: manifest.image_size,
        };
        validate_stream_begin_status(&begin, &manifest).unwrap();

        let mut stale_chunk = begin;
        stale_chunk.bytes_received = 1;
        let err = validate_stream_chunk_status(&stale_chunk, &manifest, 2)
            .unwrap_err()
            .to_string();
        assert!(err.contains("bytes_received"));

        let finish = UpdateStatus {
            active: false,
            manifest_verified: true,
            image_verified: true,
            release_version: manifest.release_version,
            bytes_received: manifest.image_size,
            image_size: manifest.image_size,
        };
        validate_stream_finish_status(&finish, &manifest).unwrap();

        let mut wrong_release = finish;
        wrong_release.release_version = manifest.release_version.saturating_add(1);
        let err = validate_stream_finish_status(&wrong_release, &manifest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("release_version"));
    }

    #[test]
    fn path_prefix_check_uses_whole_components() {
        assert!(path_starts_with(
            Path::new("/tmp/work/repo/key.hex"),
            Path::new("/tmp/work/repo")
        ));
        assert!(!path_starts_with(
            Path::new("/tmp/work/repo-other/key.hex"),
            Path::new("/tmp/work/repo")
        ));
    }
}
