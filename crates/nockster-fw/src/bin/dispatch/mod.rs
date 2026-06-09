use core::fmt::Write as _;

#[cfg(test)]
use nockster_core::Msg;
use nockster_core::{
    BuildInfo, Caps, DeviceAddressBookEntry, Frame, ReleaseInfo, Request, Response,
    TouchCalibration, UpdateTrust, ERR_BAD_COBS_OR_POSTCARD, ERR_BUSY, ERR_CRYPTO,
    ERR_DEVICE_LOCKED, ERR_ENCODE_TOO_BIG, ERR_FLASH, ERR_NO_SEED, ERR_OVERFLOW,
    ERR_UNSUPPORTED_VERSION, MAX_ADDRESS_BOOK_LABEL_LEN, MAX_ADDRESS_BOOK_PKH_LEN,
    MAX_DEVICE_ADDRESS_BOOK_ENTRIES, PROTO_V1,
};

use crate::gui::{
    default_touch_calibration, touch_calibration_valid, Gui, TOUCH_I2C_FREQUENCY_KHZ,
};
use crate::update_auth;
use crate::{seed_store, session};
use esp_hal::time::Duration;
use nockster_fw::nvs_store::{NvsError, NvsStore};
use nockster_fw::security::read_security_status;
use zeroize::Zeroize;

const ADDRESS_BOOK_PAYLOAD_MAX: usize = 4 + MAX_DEVICE_ADDRESS_BOOK_ENTRIES
    * (MAX_ADDRESS_BOOK_LABEL_LEN + MAX_ADDRESS_BOOK_PKH_LEN + 2);

pub fn update_mode_allows_frame(frame: &Frame) -> bool {
    matches!(
        frame,
        Frame::One(Request::Hello)
            | Frame::One(Request::Ping)
            | Frame::One(Request::GetInfo)
            | Frame::One(Request::GetLockStatus)
            | Frame::One(Request::GetSecurityStatus)
            | Frame::One(Request::GetBuildInfo)
            | Frame::One(Request::GetReleaseInfo)
            | Frame::One(Request::GetUpdateBootStatus)
            | Frame::One(Request::GetUpdateTrust)
            | Frame::One(Request::GetUpdateStatus)
            | Frame::One(Request::UpdateChunk { .. })
            | Frame::One(Request::FinishUpdate)
            | Frame::One(Request::CancelUpdate)
    )
}

pub fn frame_confirmation_prompt(frame: &Frame) -> Option<&'static str> {
    match frame {
        Frame::One(Request::SignDigest { .. }) => Some("Sign digest?"),
        Frame::One(Request::SignSpendHash { .. }) => Some("Approve spend?"),
        Frame::One(Request::SignSpendHashFor { .. }) => Some("Approve spend?"),
        Frame::One(Request::DeleteSeed { .. }) => Some("Delete seed?"),
        Frame::One(Request::Reset) => Some("Factory reset?"),
        Frame::One(Request::VaultStore { .. }) => Some("Store secret?"),
        Frame::One(Request::VaultReveal { .. }) => Some("Reveal secret?"),
        Frame::One(Request::VaultDelete { .. }) => Some("Delete secret?"),
        Frame::One(Request::GetMasterPubkey { .. }) => Some("Export pubkey?"),
        _ => None,
    }
}

/// Vault and master-pubkey requests. All of these arrive here only after the
/// on-screen confirmation (except VaultList, which is metadata-only).
pub fn handle_vault_request(req: &Request, locked: bool) -> Option<Response> {
    match req {
        Request::VaultList => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            Some(match NvsStore::new().vault_entries() {
                Ok(entries) => Response::OkVaultEntries(entries),
                Err(NvsError::Flash) => Response::Err { code: ERR_FLASH },
                Err(_) => Response::OkVaultEntries(alloc::vec::Vec::new()),
            })
        }
        Request::VaultStore { label, preimage } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            if preimage.is_empty() || preimage.len() > nockster_core::MAX_VAULT_PREIMAGE_LEN {
                return Some(Response::Err { code: ERR_OVERFLOW });
            }
            // The commitment is computed here, from the bytes that will be
            // stored — never trusted from the host.
            let commitment =
                match nockster_core::draft_sign::noun_commitment_v1(preimage.as_slice()) {
                    Ok(digest) => digest,
                    Err(_) => {
                        return Some(Response::Err {
                            code: ERR_BAD_COBS_OR_POSTCARD,
                        });
                    }
                };
            let Some(mut master_key) = seed_store::master_key_copy() else {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            };
            let result =
                NvsStore::new().vault_store(&master_key, label.as_str(), commitment, preimage);
            master_key.zeroize();
            Some(match result {
                Ok(_slot) => match NvsStore::new().vault_entries() {
                    Ok(entries) => Response::OkVaultEntries(entries),
                    Err(_) => Response::OkVaultEntries(alloc::vec::Vec::new()),
                },
                Err(NvsError::Full) | Err(NvsError::AlreadyInitialized) => {
                    Response::Err { code: ERR_OVERFLOW }
                }
                Err(NvsError::InvalidLabel) => Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                },
                Err(NvsError::Crypto) => Response::Err { code: ERR_CRYPTO },
                Err(_) => Response::Err { code: ERR_FLASH },
            })
        }
        Request::VaultReveal { slot } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            let Some(mut master_key) = seed_store::master_key_copy() else {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            };
            let mut nvs = NvsStore::new();
            let result = nvs.vault_reveal(&master_key, *slot as usize);
            master_key.zeroize();
            Some(match result {
                Ok(preimage) => {
                    let commitment = nvs
                        .vault_entries()
                        .ok()
                        .and_then(|entries| {
                            entries
                                .into_iter()
                                .find(|entry| entry.slot == *slot)
                                .map(|entry| entry.commitment)
                        })
                        .unwrap_or([0u64; 5]);
                    Response::OkVaultPreimage {
                        commitment,
                        preimage,
                    }
                }
                Err(NvsError::InvalidSlot) | Err(NvsError::NotInitialized) => {
                    Response::Err { code: ERR_NO_SEED }
                }
                Err(NvsError::Crypto) => Response::Err { code: ERR_CRYPTO },
                Err(_) => Response::Err { code: ERR_FLASH },
            })
        }
        Request::VaultDelete { slot } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            Some(match NvsStore::new().vault_delete(*slot as usize) {
                Ok(()) => match NvsStore::new().vault_entries() {
                    Ok(entries) => Response::OkVaultEntries(entries),
                    Err(_) => Response::OkVaultEntries(alloc::vec::Vec::new()),
                },
                Err(NvsError::InvalidSlot) | Err(NvsError::NotInitialized) => {
                    Response::Err { code: ERR_NO_SEED }
                }
                Err(_) => Response::Err { code: ERR_FLASH },
            })
        }
        Request::GetMasterPubkey { slot } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            let Ok(mut seed) = session::get_seed_for_slot(*slot as usize) else {
                return Some(Response::Err { code: ERR_NO_SEED });
            };
            let (mut sk, chain_code) = nockster_core::cheetah::master_from_seed(&seed);
            seed.zeroize();
            let pk = nockster_core::cheetah::cheetah_pub_from_sk(sk);
            sk.zeroize();
            Some(Response::OkMasterPubkey {
                x: pk.0,
                y: pk.1,
                chain_code,
            })
        }
        _ => None,
    }
}

pub fn handle_metadata_request(
    req: &Request,
    fw_major: u16,
    fw_minor: u16,
    features: u32,
) -> Option<Response> {
    match req {
        Request::Hello => Some(Response::Hello(Caps {
            proto_v: PROTO_V1,
            compressed_pk: true,
        })),
        Request::GetInfo => {
            let mut nvs = NvsStore::new();
            let has_seed_persisted = nvs.is_initialized();
            let has_seed_ram = session::has_seed();
            let cheetah_pubs = if has_seed_ram {
                seed_store::collect_info_pubs_from_ram()
            } else {
                alloc::vec::Vec::new()
            };
            Some(Response::Info {
                proto_v: PROTO_V1,
                fw_major,
                fw_minor,
                features,
                has_seed: has_seed_persisted || has_seed_ram,
                cheetah_pubs,
            })
        }
        Request::Ping => Some(Response::Pong),
        Request::GetLockStatus => {
            let mut nvs = NvsStore::new();
            let has_seed_in_ram = session::has_seed();
            let persisted_seed = nvs.is_initialized();
            let locked = if has_seed_in_ram || persisted_seed {
                session::is_locked()
            } else {
                false
            };
            Some(Response::OkLockStatus {
                locked,
                attempts_remaining: nvs.get_attempts_remaining(),
            })
        }
        Request::GetSecurityStatus => {
            let mut nvs = NvsStore::new();
            Some(Response::OkSecurityStatus(read_security_status(&mut nvs)))
        }
        Request::GetBuildInfo => Some(Response::OkBuildInfo(firmware_build_info())),
        _ => None,
    }
}

pub fn handle_update_request(req: &Request) -> Option<Response> {
    match req {
        Request::GetReleaseInfo => Some(Response::OkReleaseInfo(ReleaseInfo {
            release_version: update_auth::firmware_release_version(),
        })),
        Request::GetUpdateBootStatus => Some(Response::OkUpdateBootStatus(
            update_auth::read_update_boot_status(),
        )),
        Request::GetUpdateTrust => {
            let anchor = update_auth::trusted_pubkey_sha256();
            Some(Response::OkUpdateTrust(UpdateTrust {
                configured: anchor.is_some(),
                pubkey_sha256: anchor.unwrap_or([0u8; 32]),
            }))
        }
        Request::VerifyUpdateManifest {
            manifest,
            signature64,
            signing_pubkey_sec1,
        } => Some(
            match update_auth::verify_manifest(
                manifest,
                signature64,
                signing_pubkey_sec1.as_slice(),
            ) {
                Ok(()) => Response::Ok,
                Err(update_auth::UpdateAuthError::NoTrustAnchor)
                | Err(update_auth::UpdateAuthError::UnsupportedManifest) => Response::Err {
                    code: ERR_UNSUPPORTED_VERSION,
                },
                Err(update_auth::UpdateAuthError::Crypto(
                    nockster_core::update::UpdateSignatureError::RollbackVersion,
                )) => Response::Err {
                    code: ERR_UNSUPPORTED_VERSION,
                },
                Err(update_auth::UpdateAuthError::Crypto(_)) => Response::Err { code: ERR_CRYPTO },
            },
        ),
        Request::BeginUpdate {
            manifest,
            signature64,
            signing_pubkey_sec1,
            write_flash,
        } => Some(
            match update_auth::begin_stream(
                manifest,
                signature64,
                signing_pubkey_sec1.as_slice(),
                *write_flash,
            ) {
                Ok(status) => Response::OkUpdateStatus(status),
                Err(err) => Response::Err {
                    code: update_stream_error_code(err),
                },
            },
        ),
        Request::UpdateChunk { offset, chunk } => {
            Some(match update_auth::append_chunk(*offset, chunk.as_slice()) {
                Ok(status) => Response::OkUpdateStatus(status),
                Err(err) => Response::Err {
                    code: update_stream_error_code(err),
                },
            })
        }
        Request::FinishUpdate => Some(match update_auth::finish_stream() {
            Ok(status) => Response::OkUpdateStatus(status),
            Err(err) => Response::Err {
                code: update_stream_error_code(err),
            },
        }),
        Request::CancelUpdate => {
            update_auth::cancel_stream();
            Some(Response::Ok)
        }
        Request::GetUpdateStatus => Some(Response::OkUpdateStatus(update_auth::stream_status())),
        _ => None,
    }
}

pub fn handle_seed_label_request(req: &Request, locked: bool) -> Option<Response> {
    match req {
        Request::GetSeedLabels => {
            let mut nvs = NvsStore::new();
            Some(match nvs.read_seed_labels() {
                Ok(labels) => Response::OkSeedLabels(labels),
                Err(NvsError::Flash) => Response::Err { code: ERR_FLASH },
                Err(_) => Response::OkSeedLabels(alloc::vec::Vec::new()),
            })
        }
        Request::SetSeedLabel { slot, label } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }

            let mut nvs = NvsStore::new();
            Some(match nvs.write_seed_label(*slot as usize, label.as_str()) {
                Ok(()) => Response::Ok,
                Err(NvsError::InvalidSlot) | Err(NvsError::NotInitialized) => {
                    Response::Err { code: ERR_NO_SEED }
                }
                Err(NvsError::InvalidLabel) => Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                },
                Err(NvsError::Flash) => Response::Err { code: ERR_FLASH },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            })
        }
        _ => None,
    }
}

pub fn read_address_book_payload(locked: bool) -> Result<alloc::vec::Vec<u8>, Response> {
    if locked {
        return Err(Response::Err {
            code: ERR_DEVICE_LOCKED,
        });
    }

    let mut nvs = NvsStore::new();
    let entries = match nvs.read_device_address_book() {
        Ok(entries) => entries,
        Err(NvsError::Flash) => return Err(Response::Err { code: ERR_FLASH }),
        Err(_) => alloc::vec::Vec::new(),
    };
    let mut buf = alloc::vec![0u8; ADDRESS_BOOK_PAYLOAD_MAX];
    let used = postcard::to_slice(&entries, buf.as_mut_slice()).map_err(|_| Response::Err {
        code: ERR_ENCODE_TOO_BIG,
    })?;
    Ok(used.to_vec())
}

pub fn write_address_book_payload(payload: &[u8], locked: bool) -> Response {
    if locked {
        return Response::Err {
            code: ERR_DEVICE_LOCKED,
        };
    }

    let entries = match postcard::from_bytes::<alloc::vec::Vec<DeviceAddressBookEntry>>(payload) {
        Ok(entries) => entries,
        Err(_) => {
            return Response::Err {
                code: ERR_BAD_COBS_OR_POSTCARD,
            };
        }
    };

    let mut nvs = NvsStore::new();
    match nvs.write_device_address_book(entries.as_slice()) {
        Ok(()) => Response::Ok,
        Err(NvsError::Full) | Err(NvsError::InvalidLabel) => Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        },
        Err(NvsError::Flash) => Response::Err { code: ERR_FLASH },
        Err(_) => Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        },
    }
}

pub fn handle_touch_request(
    req: &Request,
    ui: Option<&mut Gui<'_>>,
    locked: bool,
) -> Option<Response> {
    match req {
        Request::GetTouchCalibration => {
            let calibration = NvsStore::new()
                .read_touch_calibration()
                .ok()
                .flatten()
                .unwrap_or_else(default_touch_calibration);
            Some(Response::OkTouchCalibration(calibration))
        }
        Request::SetTouchCalibration { calibration } => {
            if !touch_calibration_valid(calibration) {
                return Some(Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                });
            }

            let mut nvs = NvsStore::new();
            Some(match nvs.write_touch_calibration(calibration) {
                Ok(()) => {
                    if let Some(ui) = ui {
                        ui.set_touch_calibration(*calibration);
                    }
                    Response::Ok
                }
                Err(NvsError::Flash) => Response::Err { code: ERR_FLASH },
                Err(_) => Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                },
            })
        }
        Request::ShowTouchDiagnostics { enabled } => {
            if let Some(ui) = ui {
                if *enabled {
                    let build = diagnostics_build_label();
                    ui.show_touch_diagnostics(build.as_str());
                } else {
                    ui.hide_touch_diagnostics(locked);
                }
            }
            Some(Response::Ok)
        }
        _ => None,
    }
}

pub fn firmware_build_info() -> BuildInfo {
    BuildInfo {
        git_commit: build_string(option_env!("NOCKSTER_GIT_COMMIT").unwrap_or("unknown")),
        git_dirty: option_env!("NOCKSTER_GIT_DIRTY").unwrap_or("0") == "1",
        build_profile: build_string(option_env!("NOCKSTER_BUILD_PROFILE").unwrap_or("dev")),
        protocol_v: PROTO_V1,
        tx_types_rev: build_string(option_env!("NOCKSTER_TX_TYPES_REV").unwrap_or("unknown")),
    }
}

pub fn diagnostics_build_label() -> heapless::String<64> {
    let info = firmware_build_info();
    let mut out = heapless::String::<64>::new();
    let _ = write!(
        out,
        "fw {} {}",
        info.build_profile.as_str(),
        info.git_commit.as_str()
    );
    if info.git_dirty {
        let _ = out.push_str(" dirty");
    }
    let _ = write!(out, " i2c{}k", TOUCH_I2C_FREQUENCY_KHZ);
    out
}

fn build_string<const N: usize>(value: &str) -> heapless::String<N> {
    let mut out = heapless::String::<N>::new();
    let take = value.len().min(N);
    let _ = out.push_str(&value[..take]);
    out
}

pub fn begin_touch_calibration(ui: Option<&mut Gui<'_>>) -> Result<(), u16> {
    let Some(ui) = ui else {
        return Err(ERR_UNSUPPORTED_VERSION);
    };
    ui.begin_touch_calibration();
    Ok(())
}

pub fn finish_touch_calibration(
    calibration: TouchCalibration,
    ui: &mut Gui<'_>,
    locked: bool,
) -> Response {
    if !touch_calibration_valid(&calibration) {
        return Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        };
    }

    match NvsStore::new().write_touch_calibration(&calibration) {
        Ok(()) => {
            ui.set_touch_calibration(calibration);
            if locked {
                ui.begin_unlock(None);
            } else {
                ui.show_unlock_success();
                ui.show_idle_message_timed("Calibration saved", Duration::from_millis(3_000));
            }
            Response::OkTouchCalibration(calibration)
        }
        Err(NvsError::Flash) => Response::Err { code: ERR_FLASH },
        Err(_) => Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        },
    }
}

pub fn update_stream_error_code(err: update_auth::UpdateStreamError) -> u16 {
    match err {
        update_auth::UpdateStreamError::Auth(update_auth::UpdateAuthError::NoTrustAnchor)
        | update_auth::UpdateStreamError::Auth(update_auth::UpdateAuthError::UnsupportedManifest) => {
            ERR_UNSUPPORTED_VERSION
        }
        update_auth::UpdateStreamError::Auth(update_auth::UpdateAuthError::Crypto(
            nockster_core::update::UpdateSignatureError::RollbackVersion,
        )) => ERR_UNSUPPORTED_VERSION,
        update_auth::UpdateStreamError::Auth(update_auth::UpdateAuthError::Crypto(_))
        | update_auth::UpdateStreamError::Flash(update_auth::UpdateFlashError::VerifyMismatch)
        | update_auth::UpdateStreamError::Image(
            nockster_core::update::UpdateImageStreamError::ImageHashMismatch,
        ) => ERR_CRYPTO,
        update_auth::UpdateStreamError::Busy => ERR_BUSY,
        update_auth::UpdateStreamError::Flash(_) => ERR_FLASH,
        update_auth::UpdateStreamError::Image(
            nockster_core::update::UpdateImageStreamError::ChunkTooLarge
            | nockster_core::update::UpdateImageStreamError::Overflow,
        ) => ERR_OVERFLOW,
        update_auth::UpdateStreamError::NoActiveSession
        | update_auth::UpdateStreamError::Image(
            nockster_core::update::UpdateImageStreamError::OffsetMismatch
            | nockster_core::update::UpdateImageStreamError::IncompleteImage,
        ) => ERR_BAD_COBS_OR_POSTCARD,
    }
}

#[cfg(test)]
pub fn handle_one_frame_cobs_with<F>(frame: &[u8], mut handle_frame: F) -> alloc::vec::Vec<u8>
where
    F: FnMut(u32, &Frame) -> Response,
{
    let mut out = alloc::vec::Vec::new();
    match postcard::from_bytes_cobs::<Msg<Frame>>(frame) {
        Ok(m) if m.v == PROTO_V1 => {
            let body = handle_frame(m.id, &m.msg);
            encode_response(&mut out, m.id, body);
        }
        Ok(_) => encode_response(
            &mut out,
            0,
            Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            },
        ),
        Err(_) => encode_response(
            &mut out,
            0,
            Response::Err {
                code: ERR_BAD_COBS_OR_POSTCARD,
            },
        ),
    }
    out
}

#[cfg(test)]
fn encode_response(out: &mut alloc::vec::Vec<u8>, id: u32, resp: Response) {
    let msg = Msg {
        v: PROTO_V1,
        id,
        msg: resp,
    };
    let mut tmp = [0u8; 4096];
    let tmp = postcard::to_slice(&msg, &mut tmp).unwrap();
    let mut enc = alloc::vec::Vec::with_capacity(cobs::max_encoding_length(tmp.len()));
    enc.resize(cobs::max_encoding_length(tmp.len()), 0);
    let used = cobs::encode(tmp, &mut enc[..]);
    enc.truncate(used);
    out.extend_from_slice(&enc);
    out.push(0);
}
