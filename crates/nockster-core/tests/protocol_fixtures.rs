//! Golden postcard fixtures shared with nockster-js.
//!
//! The wire format is kept in sync between the Rust enums here and the
//! hand-written serializers in `nockster-js/src/protocol.ts`. This test pins
//! one fixture per protocol variant to a checked-in golden file that the
//! nockster-js test suite (`test/protocol-fixtures.test.mjs`) replays from the
//! TypeScript side. A wire-format change on either side fails one of the two
//! suites instead of corrupting traffic at runtime.
//!
//! To update after an intentional protocol change:
//!   NOCKSTER_REGEN_PROTOCOL_FIXTURES=1 cargo test -p nockster-core --test protocol_fixtures
//! then update nockster-js/src/protocol.ts and its fixture test to match.

use nockster_core::update::UpdateManifest;
use nockster_core::*;

const FIXTURES_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../nockster-js/test/fixtures/protocol-fixtures.txt"
);

// When a variant is added to the protocol, these matches stop compiling.
// That is intentional: add a fixture for the new variant below, regenerate
// the golden file, and update nockster-js/src/protocol.ts plus its fixture
// test. Do not add a wildcard arm.
#[allow(dead_code)]
fn request_variant_reminder(req: Request) {
    match req {
        Request::Hello => {}
        Request::GetInfo => {}
        Request::Ping => {}
        Request::Wipe => {}
        Request::SetSeed { .. } => {}
        Request::GetFingerprint => {}
        Request::GetPubkey { .. } => {}
        Request::GetXpub { .. } => {}
        Request::SignDigest { .. } => {}
        Request::GetCheetahPub { .. } => {}
        Request::SignSpendHash { .. } => {}
        Request::SignSpendHashFor { .. } => {}
        Request::Health => {}
        Request::InitializePIN { .. } => {}
        Request::AddSeed { .. } => {}
        Request::DeleteSeed { .. } => {}
        Request::Unlock { .. } => {}
        Request::Lock => {}
        Request::ResetPIN { .. } => {}
        Request::GetLockStatus => {}
        Request::SelectSeed { .. } => {}
        Request::Reset => {}
        Request::GetSecurityStatus => {}
        Request::GetBuildInfo => {}
        Request::GetTouchCalibration => {}
        Request::SetTouchCalibration { .. } => {}
        Request::ShowTouchDiagnostics { .. } => {}
        Request::GetSeedLabels => {}
        Request::SetSeedLabel { .. } => {}
        Request::ChangePinOnDevice { .. } => {}
        Request::StartTouchCalibration => {}
        Request::GetUpdateTrust => {}
        Request::VerifyUpdateManifest { .. } => {}
        Request::BeginUpdate { .. } => {}
        Request::UpdateChunk { .. } => {}
        Request::FinishUpdate => {}
        Request::CancelUpdate => {}
        Request::GetUpdateStatus => {}
        Request::GetReleaseInfo => {}
        Request::GetUpdateBootStatus => {}
        Request::Reboot => {}
        Request::GetAddressBook => {}
    }
}

#[allow(dead_code)]
fn response_variant_reminder(resp: Response) {
    match resp {
        Response::Hello(_) => {}
        Response::FragBegin { .. } => {}
        Response::FragPart { .. } => {}
        Response::Info { .. } => {}
        Response::Pong => {}
        Response::Ok => {}
        Response::OkSig { .. } => {}
        Response::OkFingerprint { .. } => {}
        Response::OkPubkey { .. } => {}
        Response::OkPubkeyCompressed { .. } => {}
        Response::OkXpub(_) => {}
        Response::OkCheetahPub { .. } => {}
        Response::OkCheetahSig { .. } => {}
        Response::OkLockStatus { .. } => {}
        Response::Err { .. } => {}
        Response::OkSecurityStatus(_) => {}
        Response::OkBuildInfo(_) => {}
        Response::OkTouchCalibration(_) => {}
        Response::OkSeedLabels(_) => {}
        Response::OkUpdateTrust(_) => {}
        Response::OkUpdateStatus(_) => {}
        Response::OkReleaseInfo(_) => {}
        Response::OkUpdateBootStatus(_) => {}
        Response::OkAddressBook(_) => {}
    }
}

#[allow(dead_code)]
fn frame_variant_reminder(frame: Frame) {
    match frame {
        Frame::One(_) => {}
        Frame::FragBegin { .. } => {}
        Frame::FragPart { .. } => {}
    }
}

fn hstr<const N: usize>(s: &str) -> heapless::String<N> {
    let mut out = heapless::String::new();
    out.push_str(s).expect("fixture string fits");
    out
}

fn path(vals: &[u32]) -> alloc_path::Path {
    vals.iter().copied().collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn manifest() -> UpdateManifest {
    UpdateManifest {
        manifest_version: 1,
        release_version: 42,
        image_size: 1024,
        image_sha256: [0xAA; 32],
        signing_pubkey_sha256: [0xBB; 32],
        hardware_target: hstr("esp32s3"),
        build_profile: hstr("dev"),
        protocol_v: 1,
        git_commit: hstr("abc123"),
        tx_types_rev: hstr("def456"),
    }
}

fn spend_meta() -> SpendMeta {
    SpendMeta {
        outputs: vec![SpendOutputMeta {
            gift: 1_000,
            recipient_pkh_b58: "addr1".into(),
            is_refund: false,
        }],
    }
}

fn cheetah_pub() -> CheetahPub {
    CheetahPub {
        slot: 0,
        path: path(&[44, 0]),
        x: [1, 2, 3, 4, 5, 6],
        y: [7, 8, 9, 10, 11, 12],
    }
}

fn request_fixtures() -> Vec<(&'static str, Request)> {
    vec![
        ("Hello", Request::Hello),
        ("GetInfo", Request::GetInfo),
        ("Ping", Request::Ping),
        ("Wipe", Request::Wipe),
        ("SetSeed", Request::SetSeed { seed64: [0x11; 64] }),
        ("GetFingerprint", Request::GetFingerprint),
        (
            "GetPubkey",
            Request::GetPubkey {
                path: path(&[44, 0, 7]),
                compressed: true,
            },
        ),
        (
            "GetXpub",
            Request::GetXpub {
                path: path(&[44, 1]),
            },
        ),
        (
            "SignDigest",
            Request::SignDigest {
                path: path(&[2_147_483_692]),
                digest32: core::array::from_fn(|i| i as u8),
            },
        ),
        (
            "GetCheetahPub",
            Request::GetCheetahPub {
                slot: 2,
                path: path(&[44, 0]),
            },
        ),
        (
            "SignSpendHash#meta",
            Request::SignSpendHash {
                slot: 1,
                path: path(&[44]),
                msg5: [1, 2, u64::MAX, 4, 5],
                meta: Some(spend_meta()),
            },
        ),
        (
            "SignSpendHash#nometa",
            Request::SignSpendHash {
                slot: 1,
                path: path(&[44]),
                msg5: [1, 2, 3, 4, 5],
                meta: None,
            },
        ),
        (
            "SignSpendHashFor",
            Request::SignSpendHashFor {
                slot: 0,
                path: path(&[44, 9]),
                msg5: [9, 8, 7, 6, 5],
                pubkey: ([1, 2, 3, 4, 5, 6], [7, 8, 9, 10, 11, 12]),
                meta: None,
            },
        ),
        ("Health", Request::Health),
        (
            "InitializePIN",
            Request::InitializePIN {
                pin: "1234".into(),
                seed64: [0x22; 64],
            },
        ),
        ("AddSeed", Request::AddSeed { seed64: [0x33; 64] }),
        ("DeleteSeed", Request::DeleteSeed { slot: 3 }),
        ("Unlock", Request::Unlock { pin: "1234".into() }),
        ("Lock", Request::Lock),
        (
            "ResetPIN",
            Request::ResetPIN {
                current_pin: "1234".into(),
                new_pin: "5678".into(),
            },
        ),
        ("GetLockStatus", Request::GetLockStatus),
        ("SelectSeed", Request::SelectSeed { slot: 1 }),
        ("Reset", Request::Reset),
        ("GetSecurityStatus", Request::GetSecurityStatus),
        ("GetBuildInfo", Request::GetBuildInfo),
        ("GetTouchCalibration", Request::GetTouchCalibration),
        (
            "SetTouchCalibration",
            Request::SetTouchCalibration {
                calibration: TouchCalibration {
                    raw_x_min: 1,
                    raw_x_max: 4000,
                    raw_y_min: 3,
                    raw_y_max: 4001,
                    mirror_x: true,
                    mirror_y: false,
                },
            },
        ),
        (
            "ShowTouchDiagnostics",
            Request::ShowTouchDiagnostics { enabled: true },
        ),
        ("GetSeedLabels", Request::GetSeedLabels),
        (
            "SetSeedLabel",
            Request::SetSeedLabel {
                slot: 0,
                label: hstr("main"),
            },
        ),
        (
            "ChangePinOnDevice",
            Request::ChangePinOnDevice {
                current_pin: "1234".into(),
            },
        ),
        ("StartTouchCalibration", Request::StartTouchCalibration),
        ("GetUpdateTrust", Request::GetUpdateTrust),
        (
            "VerifyUpdateManifest",
            Request::VerifyUpdateManifest {
                manifest: manifest(),
                signature64: [0x44; 64],
                signing_pubkey_sec1: vec![0x02; 33],
            },
        ),
        (
            "BeginUpdate",
            Request::BeginUpdate {
                manifest: manifest(),
                signature64: [0x44; 64],
                signing_pubkey_sec1: vec![0x02; 33],
                write_flash: true,
            },
        ),
        (
            "UpdateChunk",
            Request::UpdateChunk {
                offset: 512,
                chunk: vec![0xAB; 8],
            },
        ),
        ("FinishUpdate", Request::FinishUpdate),
        ("CancelUpdate", Request::CancelUpdate),
        ("GetUpdateStatus", Request::GetUpdateStatus),
        ("GetReleaseInfo", Request::GetReleaseInfo),
        ("GetUpdateBootStatus", Request::GetUpdateBootStatus),
        ("Reboot", Request::Reboot),
        ("GetAddressBook", Request::GetAddressBook),
    ]
}

fn frame_fixtures() -> Vec<(&'static str, Frame)> {
    vec![
        (
            "FragBegin",
            Frame::FragBegin {
                id: 5,
                total_len: 1024,
                kind: FragKind::SignDraft,
            },
        ),
        (
            "FragPart",
            Frame::FragPart {
                id: 5,
                offset: 512,
                chunk: vec![0xCD; 4],
                last: true,
            },
        ),
    ]
}

fn response_fixtures() -> Vec<(&'static str, Response)> {
    vec![
        (
            "Hello",
            Response::Hello(Caps {
                proto_v: 1,
                compressed_pk: false,
            }),
        ),
        (
            "FragBegin",
            Response::FragBegin {
                id: 9,
                total_len: 2048,
                kind: FragKind::SignDraft,
            },
        ),
        (
            "FragPart",
            Response::FragPart {
                id: 9,
                offset: 0,
                chunk: vec![0xEF; 4],
                last: false,
            },
        ),
        (
            "Info",
            Response::Info {
                proto_v: 1,
                fw_major: 0,
                fw_minor: 1,
                features: 0x3FFF,
                has_seed: true,
                cheetah_pubs: vec![cheetah_pub()],
            },
        ),
        ("Pong", Response::Pong),
        ("Ok", Response::Ok),
        ("OkSig", Response::OkSig { sig64: [0x55; 64] }),
        (
            "OkFingerprint",
            Response::OkFingerprint { fp4: [1, 2, 3, 4] },
        ),
        (
            "OkPubkey",
            Response::OkPubkey {
                uncompressed: [0x04; 65],
            },
        ),
        (
            "OkPubkeyCompressed",
            Response::OkPubkeyCompressed {
                compressed: [0x03; 33],
            },
        ),
        (
            "OkXpub",
            Response::OkXpub(Xpub {
                depth: 1,
                fp4: [1, 2, 3, 4],
                child: 5,
                chain_code: [0x10; 32],
                pubkey33: [0x03; 33],
            }),
        ),
        (
            "OkCheetahPub",
            Response::OkCheetahPub {
                x: [1, 2, 3, 4, 5, u64::MAX],
                y: [7, 8, 9, 10, 11, 12],
            },
        ),
        (
            "OkCheetahSig",
            Response::OkCheetahSig {
                chal: [1, 2, 3, 4, 5, 6, 7, 8],
                sig: [9, 10, 11, 12, 13, 14, 15, 16],
            },
        ),
        (
            "OkLockStatus",
            Response::OkLockStatus {
                locked: true,
                attempts_remaining: 7,
            },
        ),
        ("Err", Response::Err { code: 0x0103 }),
        (
            "OkSecurityStatus",
            Response::OkSecurityStatus(SecurityStatus {
                chip_security_available: true,
                mac: [1, 2, 3, 4, 5, 6],
                flash_encryption: false,
                flash_crypt_cnt: 0,
                secure_boot: false,
                secure_version: 0,
                key_purposes: [0; 6],
                hmac_key_slots: 1,
                hmac_user_key_slots: 1,
                read_protected_key_slots: 0,
                pad_jtag_disabled: false,
                usb_jtag_disabled: false,
                soft_jtag_disabled: false,
                soft_jtag_disable_bits: 0,
                usb_serial_jtag_disabled: false,
                download_mode_disabled: false,
                usb_serial_jtag_download_disabled: false,
                usb_otg_download_disabled: false,
                secure_download_enabled: false,
                direct_boot_disabled: false,
                usb_rom_print_disabled: false,
                power_glitch_enabled: false,
                nvs_initialized: true,
                nvs_schema_version: 2,
                nvs_slot_count: 1,
            }),
        ),
        (
            "OkBuildInfo",
            Response::OkBuildInfo(BuildInfo {
                git_commit: hstr("abc123"),
                git_dirty: false,
                build_profile: hstr("dev"),
                protocol_v: 1,
                tx_types_rev: hstr("def456"),
            }),
        ),
        (
            "OkTouchCalibration",
            Response::OkTouchCalibration(TouchCalibration {
                raw_x_min: 1,
                raw_x_max: 4000,
                raw_y_min: 3,
                raw_y_max: 4001,
                mirror_x: false,
                mirror_y: true,
            }),
        ),
        (
            "OkSeedLabels",
            Response::OkSeedLabels(vec![SeedSlotLabel {
                slot: 0,
                label: hstr("main"),
            }]),
        ),
        (
            "OkUpdateTrust",
            Response::OkUpdateTrust(UpdateTrust {
                configured: true,
                pubkey_sha256: [0xBB; 32],
            }),
        ),
        (
            "OkUpdateStatus",
            Response::OkUpdateStatus(UpdateStatus {
                active: true,
                manifest_verified: true,
                image_verified: false,
                release_version: 42,
                bytes_received: 4096,
                image_size: 1024,
            }),
        ),
        (
            "OkReleaseInfo",
            Response::OkReleaseInfo(ReleaseInfo {
                release_version: 42,
            }),
        ),
        (
            "OkUpdateBootStatus",
            Response::OkUpdateBootStatus(UpdateBootStatus {
                partition_table_ok: true,
                ota_data_present: true,
                ota0_present: true,
                ota1_present: true,
                current_slot: UPDATE_SLOT_NONE,
                next_slot: UPDATE_SLOT_OTA0,
                ota_state: UPDATE_OTA_STATE_VALID,
                ota0_offset: 0x320000,
                ota0_size: 0x300000,
                ota1_offset: 0x620000,
                ota1_size: 0x300000,
            }),
        ),
        (
            "OkAddressBook",
            Response::OkAddressBook(vec![DeviceAddressBookEntry {
                label: hstr("alice"),
                pkh: hstr("addr1"),
            }]),
        ),
    ]
}

fn serialize<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    postcard::to_slice(value, &mut buf)
        .expect("fixture serializes")
        .to_vec()
}

/// Fixture msg id shared with the JS test; ids are u32 varints on the wire,
/// so use one that exercises a multi-byte varint.
const FIXTURE_MSG_ID: u32 = 777;

fn render_fixtures() -> String {
    let mut out = String::new();
    out.push_str(
        "# Golden postcard fixtures shared between nockster-core and nockster-js.\n\
         # Generated by crates/nockster-core/tests/protocol_fixtures.rs — do not edit by hand.\n\
         # Regenerate: NOCKSTER_REGEN_PROTOCOL_FIXTURES=1 cargo test -p nockster-core --test protocol_fixtures\n",
    );
    for (name, req) in request_fixtures() {
        let msg = Msg {
            v: PROTO_V1,
            id: FIXTURE_MSG_ID,
            msg: Frame::One(req),
        };
        out.push_str(&format!("req:{name}={}\n", hex(&serialize(&msg))));
    }
    for (name, frame) in frame_fixtures() {
        let msg = Msg {
            v: PROTO_V1,
            id: FIXTURE_MSG_ID,
            msg: frame,
        };
        out.push_str(&format!("frame:{name}={}\n", hex(&serialize(&msg))));
    }
    for (name, resp) in response_fixtures() {
        let msg = Msg {
            v: PROTO_V1,
            id: FIXTURE_MSG_ID,
            msg: resp,
        };
        out.push_str(&format!("resp:{name}={}\n", hex(&serialize(&msg))));
    }
    out
}

#[test]
fn protocol_fixtures_match_golden_file() {
    let rendered = render_fixtures();

    if std::env::var_os("NOCKSTER_REGEN_PROTOCOL_FIXTURES").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FIXTURES_PATH).parent().unwrap()).unwrap();
        std::fs::write(FIXTURES_PATH, &rendered).unwrap();
        return;
    }

    let golden = std::fs::read_to_string(FIXTURES_PATH).unwrap_or_else(|err| {
        panic!(
            "missing golden fixture file {FIXTURES_PATH} ({err}); run with \
             NOCKSTER_REGEN_PROTOCOL_FIXTURES=1 to create it"
        )
    });

    assert_eq!(
        rendered, golden,
        "protocol wire format changed; if intentional, regenerate with \
         NOCKSTER_REGEN_PROTOCOL_FIXTURES=1 and update nockster-js/src/protocol.ts \
         plus nockster-js/test/protocol-fixtures.test.mjs"
    );
}

#[test]
fn protocol_fixtures_roundtrip_in_rust() {
    for (name, req) in request_fixtures() {
        let msg = Msg {
            v: PROTO_V1,
            id: FIXTURE_MSG_ID,
            msg: Frame::One(req),
        };
        let bytes = serialize(&msg);
        let decoded: Msg<Frame> = postcard::from_bytes(&bytes)
            .unwrap_or_else(|err| panic!("req:{name} does not roundtrip: {err}"));
        assert_eq!(serialize(&decoded), bytes, "req:{name} re-encodes differently");
    }
    for (name, resp) in response_fixtures() {
        let msg = Msg {
            v: PROTO_V1,
            id: FIXTURE_MSG_ID,
            msg: resp,
        };
        let bytes = serialize(&msg);
        let decoded: Msg<Response> = postcard::from_bytes(&bytes)
            .unwrap_or_else(|err| panic!("resp:{name} does not roundtrip: {err}"));
        assert_eq!(serialize(&decoded), bytes, "resp:{name} re-encodes differently");
    }
}
