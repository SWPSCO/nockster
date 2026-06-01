use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature};
use nockster_core::alloc_path as pathmod;
use nockster_core::{
    cheetah, draft_sign::SignerConfig, Request, Response, ERR_DEVICE_LOCKED, ERR_NO_SEED,
    ERR_WRONG_PUBKEY,
};
use zeroize::Zeroize;

use crate::seed_store::{
    active_slot_index, derive_child_sk_for_slot, derive_signing_key_active, get_xpub,
    master_fingerprint_for_active,
};

pub fn active_root_signer_config() -> Result<SignerConfig, u16> {
    let slot = active_slot_index().map_err(|_| ERR_NO_SEED)?;
    let path = pathmod::Path::new();
    let sk = derive_child_sk_for_slot(&path, slot).map_err(|_| ERR_NO_SEED)?;
    Ok(SignerConfig { sk_be: sk })
}

pub fn preflight_spend_pubkey(
    slot: u8,
    path: &pathmod::Path,
    pubkey: &([u64; 6], [u64; 6]),
    locked: bool,
) -> Result<(), u16> {
    if locked {
        return Err(ERR_DEVICE_LOCKED);
    }
    let mut sk = derive_child_sk_for_slot(path, slot as usize).map_err(|_| ERR_NO_SEED)?;
    let pk_dev = cheetah::cheetah_pub_from_sk(sk);
    sk.zeroize();
    if &pk_dev != pubkey {
        return Err(ERR_WRONG_PUBKEY);
    }
    Ok(())
}

pub fn handle_request(req: &Request, locked: bool) -> Option<Response> {
    match req {
        Request::GetFingerprint => Some(match master_fingerprint_for_active() {
            Ok(fp4) => Response::OkFingerprint { fp4 },
            Err(_) => Response::Err { code: ERR_NO_SEED },
        }),
        Request::GetPubkey { path, compressed } => Some(match derive_signing_key_active(path) {
            Ok(sk) => {
                let vk = sk.verifying_key();
                if *compressed {
                    let mut out = [0u8; 33];
                    out.copy_from_slice(vk.to_encoded_point(true).as_bytes());
                    Response::OkPubkeyCompressed { compressed: out }
                } else {
                    let mut out = [0u8; 65];
                    out.copy_from_slice(vk.to_encoded_point(false).as_bytes());
                    Response::OkPubkey { uncompressed: out }
                }
            }
            Err(_) => Response::Err { code: ERR_NO_SEED },
        }),
        Request::SignDigest { path, digest32 } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            Some(match derive_signing_key_active(path) {
                Ok(sk) => {
                    let mut sig: Signature = PrehashSigner::sign_prehash(&sk, digest32).unwrap();
                    if let Some(norm) = sig.normalize_s() {
                        sig = norm;
                    }
                    let mut out = [0u8; 64];
                    out.copy_from_slice(&sig.to_bytes());
                    Response::OkSig { sig64: out }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            })
        }
        Request::GetXpub { path } => Some(match get_xpub(path) {
            Ok(x) => Response::OkXpub(x),
            Err(_) => Response::Err { code: ERR_NO_SEED },
        }),
        Request::GetCheetahPub { slot, path } => {
            Some(match derive_child_sk_for_slot(path, *slot as usize) {
                Ok(sk) => {
                    let pk = cheetah::cheetah_pub_from_sk(sk);
                    Response::OkCheetahPub { x: pk.0, y: pk.1 }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            })
        }
        Request::SignSpendHash {
            slot, path, msg5, ..
        } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            Some(match derive_child_sk_for_slot(path, *slot as usize) {
                Ok(sk) => {
                    let pk = cheetah::cheetah_pub_from_sk(sk);
                    let hash = cheetah::Hash { values: *msg5 };
                    let (e, s) = cheetah::schnorr_sign_tx(sk, pk, hash.values);
                    Response::OkCheetahSig {
                        chal: e.values,
                        sig: s.values,
                    }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            })
        }
        Request::SignSpendHashFor {
            slot,
            path,
            msg5,
            pubkey,
            ..
        } => {
            if locked {
                return Some(Response::Err {
                    code: ERR_DEVICE_LOCKED,
                });
            }
            Some(match derive_child_sk_for_slot(path, *slot as usize) {
                Ok(sk) => {
                    let pk_dev = cheetah::cheetah_pub_from_sk(sk);
                    if &pk_dev != pubkey {
                        Response::Err {
                            code: ERR_WRONG_PUBKEY,
                        }
                    } else {
                        let hash = cheetah::Hash { values: *msg5 };
                        let (e, s) = cheetah::schnorr_sign_tx(sk, *pubkey, hash.values);
                        Response::OkCheetahSig {
                            chal: e.values,
                            sig: s.values,
                        }
                    }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            })
        }
        Request::Health => Some(handle_health()),
        _ => None,
    }
}

fn handle_health() -> Response {
    let slot = match active_slot_index() {
        Ok(idx) => idx,
        Err(_) => return Response::Err { code: ERR_NO_SEED },
    };
    let path = pathmod::Path::from_iter([0x8000_002c, 0x8000_0000, 0x8000_0000, 0, 0].into_iter());
    match derive_child_sk_for_slot(&path, slot) {
        Ok(sk) => {
            let pk = cheetah::cheetah_pub_from_sk(sk);
            let hash = cheetah::Hash {
                values: [0, 0, 0, 0, 0],
            };
            let (e, s) = cheetah::schnorr_sign_tx(sk, pk, hash.values);
            Response::OkCheetahSig {
                chal: e.values,
                sig: s.values,
            }
        }
        Err(_) => Response::Err { code: ERR_NO_SEED },
    }
}
