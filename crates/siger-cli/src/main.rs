// NO bitcoin modules
// mod address;

extern crate alloc;

use alloc::fmt::Debug;
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use cobs;
use hex;
use postcard::{from_bytes_cobs, to_allocvec};
use serialport;
use std::{io::{Read, Write}, time::Duration};
use siger_core::{Msg, Request, Response, PROTO_V1, Frame, FragKind, alloc_path as pathmod};

use nockvm::mem::NockStack;
use nockvm::noun::{Atom, IndirectAtom, Noun};
use nockvm::serialization::cue;
use noun_serde::NounEncode;
use tx_types::collections::{ZMap, ZSet};
use tx_types::transaction_types::*;
use tx_types::RawTransaction;

// ---------- CLI --------------------------------------------------------------

#[derive(Parser)]
#[command(name="siger-cli")]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// End-to-end self test (seed -> child key -> self-check signatures)
    Test {
        #[arg(long, default_value = "/dev/ttyACM0")]
        port: String,
        #[arg(long, default_value_t = 115200)]
        baud: u32,
        /// Optional: 64-byte seed in hex (overrides default)
        #[arg(long)]
        seed_hex: Option<String>,
        /// Derivation path (comma separated uint32, MSB=hard)
        #[arg(long, default_value = "2147483692,2147483648,2147483648,0,0")] // 44'/0'/0'/0/0
        path: String,
    },

    /// Get basic device info / capabilities
    GetInfo {
        #[arg(long, default_value = "/dev/ttyACM0")] port: String,
        #[arg(long, default_value_t = 115200)] baud: u32,
    },

    /// Device health (the firmware’s well-known test)
    Health {
        #[arg(long, default_value = "/dev/ttyACM0")] port: String,
        #[arg(long, default_value_t = 115200)] baud: u32,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test { port, baud, seed_hex, path } => cmd_test(&port, baud, seed_hex.as_deref(), &path),
        Cmd::GetInfo { port, baud } => cmd_getinfo(&port, baud),
        Cmd::Health { port, baud } => cmd_health(&port, baud),
    }
}

// ---------- Serial helpers ---------------------------------------------------

pub trait RW: Read + Write {}
impl<T: Read + Write> RW for T {}

fn open(port: &str, baud: u32) -> anyhow::Result<Box<dyn serialport::SerialPort>> {
    Ok(serialport::new(port, baud)
        .timeout(Duration::from_millis(200))
        .open()?)
}

fn send_call(sp: &mut dyn serialport::SerialPort, id: u32, req: Request) -> anyhow::Result<Response> {
    send_recv(sp, id, Frame::One(req))
}

fn send_blob(sp: &mut dyn serialport::SerialPort, xid: u16, kind: FragKind, bytes: &[u8]) -> anyhow::Result<()> {
    let total = bytes.len() as u32;
    let _: Response = send_recv(sp, 0xF000_0000 | xid as u32, Frame::FragBegin { id: xid, total_len: total, kind })?;
    const CHUNK: usize = 200; // 256b budget
    let mut off = 0u32;
    while (off as usize) < bytes.len() {
        let end = core::cmp::min(bytes.len(), off as usize + CHUNK);
        let last = end == bytes.len();
        let chunk = bytes[off as usize..end].to_vec();
        let _: Response = send_recv(sp, 0xF100_0000 | (xid as u32), Frame::FragPart {
            id: xid, offset: off, chunk, last,
        })?;
        off = end as u32;
    }
    Ok(())
}

fn send_recv<T: serde::Serialize, R: for<'de> serde::Deserialize<'de>>(
    sp: &mut dyn serialport::SerialPort,
    id: u32,
    msg: T,
) -> anyhow::Result<R> {
    let m = Msg { v: PROTO_V1, id, msg };
    let buf = to_allocvec(&m)?;
    let mut framed = vec![0u8; cobs::max_encoding_length(buf.len()) + 1];
    let n = cobs::encode(&buf, &mut framed);
    framed.truncate(n);
    framed.push(0);
    sp.write_all(&framed)?;

    let mut rx: Vec<u8> = Vec::with_capacity(256);
    loop {
        let mut b = [0u8; 1];
        if sp.read_exact(&mut b).is_err() { continue; }
        if b[0] == 0 { break; }
        rx.push(b[0]);
    }
    let resp: Msg<R> = from_bytes_cobs(&mut rx)?;
    anyhow::ensure!(resp.v == PROTO_V1, "bad protocol version");
    Ok(resp.msg)
}

// ---------- Subcommands ------------------------------------------------------

fn cmd_getinfo(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    // Assuming firmware maps Hello => Caps
    let resp: Response = send_recv(&mut *sp, 1, Frame::One(Request::Hello))?;
    println!("{resp:?}");
    Ok(())
}

fn cmd_health(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let resp: Response = send_recv(&mut *sp, 2, Frame::One(Request::Health))?;
    println!("{resp:?}");
    Ok(())
}

fn cmd_test(port: &str, baud: u32, seed_hex: Option<&str>, path_str: &str) -> anyhow::Result<()> {
    use siger_core::cheetah;

    let mut sp = open(port, baud)?;

    // hello
    let caps: Response = send_call(&mut *sp, 1, Request::Hello)?;
    println!("caps: {caps:?}");

    // set seed from hex seed
    let seed = seed_hex.map(parse_64).transpose()?.unwrap_or([0x11;64]);
    send_blob(&mut *sp, 42, FragKind::SetSeed, &seed)?;

    // set deriv path
    let path = pathmod::Path::from_iter(
        path_str.split(',').filter(|s| !s.is_empty()).map(|s| s.parse::<u32>().unwrap())
    );

    // cheetah pub
    let dev_pk_resp: Response = send_call(&mut *sp, 5, Request::GetCheetahPub { path: path.clone() })?;
    let dev_pk = match dev_pk_resp { Response::OkCheetahPub { x, y } => (x,y), r => anyhow::bail!("unexpected: {r:?}") };

    // sign-for self-test
    let txid = siger_core::cheetah::Hash { values: [1,2,3,4,5] };
    let host_sig = {
    let (sk, cc) = siger_core::cheetah::master_from_seed(&seed);
    let mut xk = siger_core::cheetah::XKey::from_master(sk, cc);
    for i in path.iter() { xk = siger_core::cheetah::xprv_derive_child(&xk, *i); }
    let pk = xk.pk.unwrap();
    anyhow::ensure!(pk == dev_pk, "device pk != host pk");
    siger_core::cheetah::schnorr_sign_txid(xk.sk.unwrap(), pk, txid)
    };
    let dev_sig = match send_call(
    &mut *sp, 6,
    Request::SignTxIdFor { path, txid5: txid.values, pubkey: dev_pk }
    )? {
    Response::OkCheetahSig { chal, sig } => (chal, sig),
    r => anyhow::bail!("unexpected: {r:?}"),
    };
    anyhow::ensure!(dev_sig.0 == host_sig.0.values && dev_sig.1 == host_sig.1.values, "sig mismatch");
    println!("self-test: OK");

    Ok(())
}

// ---------- Draft helpers (jam -> RawTransaction, planning, signing) --------

fn raw_tx_from_jam_bytes(bytes: &[u8]) -> anyhow::Result<RawTransaction> {
    // Avoid NounSlab type parameter issues; use NockStack to materialize Atom
    let mut stack = NockStack::new(8 << 20, 0);
    let (mut atom, mut buf) = unsafe { IndirectAtom::new_raw_mut_bytes(&mut stack, bytes.len()) };
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len()); }
    let atom: Atom = unsafe { atom.normalize_as_atom() };

    let noun: Noun = cue(&mut stack, atom)
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    let tx = RawTransaction::from_noun(&mut stack, &noun)
        .map_err(|e| anyhow!("RawTransaction::from_noun failed: {e:?}"))?;
    Ok(tx)
}

pub fn raw_tx_from_draft_file(path: &str) -> anyhow::Result<RawTransaction> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path}"))?;
    raw_tx_from_jam_bytes(&bytes)
}

pub struct InputSigningPlan {
    pub name: NName,
    pub m: u64,
    pub combos: alloc::vec::Vec<alloc::vec::Vec<SchnorrPubkey>>,
}

pub fn enumerate_signing_plans(inputs: &Inputs) -> alloc::vec::Vec<InputSigningPlan> {
    let pairs: alloc::vec::Vec<(NName, Input)> = inputs.p.tap();
    pairs.into_iter().map(|(name, input)| {
        let m = input.note.lock.m as usize;
        let mut keys: alloc::vec::Vec<SchnorrPubkey> = input.note.lock.pubkeys.tap();
        keys.sort(); // needs Ord on SchnorrPubkey

        let mut combos = alloc::vec::Vec::new();
        if m > 0 && m <= keys.len() {
            let mut cur = alloc::vec::Vec::with_capacity(m);
            fn choose(
                out: &mut alloc::vec::Vec<alloc::vec::Vec<SchnorrPubkey>>,
                keys: &[SchnorrPubkey],
                m: usize, start: usize,
                cur: &mut alloc::vec::Vec<SchnorrPubkey>,
            ) {
                if cur.len() == m { out.push(cur.clone()); return; }
                for i in start..keys.len() {
                    cur.push(keys[i].clone());
                    choose(out, keys, m, i + 1, cur);
                    cur.pop();
                }
            }
            choose(&mut combos, &keys, m, 0, &mut cur);
        }

        InputSigningPlan { name, m: input.note.lock.m, combos }
    }).collect()
}

pub fn sign_draft_with_paths(
    sp: &mut dyn RW,
    draft_path: &str,
    signer_paths: Vec<Vec<u32>>, // derivation paths
) -> Result<RawTransaction> {
    let bytes = std::fs::read(draft_path).with_context(|| format!("read {}", draft_path))?;
    let mut stack = NockStack::new(8 << 20, 0);

    let (mut atom, mut buf) = unsafe { IndirectAtom::new_raw_mut_bytes(&mut stack, bytes.len()) };
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len()); }
    let atom = unsafe { atom.normalize_as_atom() };

    let noun: Noun = cue(&mut stack, atom)
        .map_err(|e| anyhow::anyhow!("cue failed: {e:?}"))?;
    let mut raw: RawTransaction = RawTransaction::from_noun(&mut stack, &noun)
        .map_err(|e| anyhow::anyhow!("RawTransaction::from_noun failed: {e:?}"))?;

    let txid = raw.compute_id();

    // cache pk per path
    let mut path_pks: Vec<(Vec<u32>, SchnorrPubkey)> = Vec::new();
    for path in signer_paths.iter() {
        let req = Msg { v: PROTO_V1, id: 0x4100, msg: Frame::One(Request::GetCheetahPub { path: path.clone() }) };
        let resp: Msg<Response> = {
            // send/recv via COBS
            let m = Msg { v: PROTO_V1, id: 0x4100, msg: Frame::One(Request::GetCheetahPub { path: path.clone() }) };
            round_trip_frame(sp, &m)?
        };
        let pk = match resp.msg {
            Response::OkCheetahPub { x, y } => SchnorrPubkey {
                x: F6LT { values: x }, y: F6LT { values: y }, inf: false
            },
            Response::Err { code } => return Err(anyhow!("GetCheetahPub failed: code {}", code)),
            _ => return Err(anyhow!("unexpected response to GetCheetahPub")),
        };
        path_pks.push((path.clone(), pk));
    }

    // sign inputs
    let mut new_inputs = ZMap::new();
    for (name, mut input) in raw.inputs.p.tap() {
        let lock_pks = &input.note.lock.pubkeys; // ZSet<SchnorrPubkey>

        // reuse or create signature map
        let mut sig_map: ZMap<SchnorrPubkey, SchnorrSignature> =
            input.spend.signature.as_ref().map(|s| s.map.clone()).unwrap_or_else(ZMap::new);

        for (path, pk_dev) in path_pks.iter() {
            if !zset_contains(lock_pks, pk_dev) {
                continue;
            }

            let req = Msg {
                v: PROTO_V1, id: 0x4200,
                msg: Frame::One(Request::SignTxId { path: path.clone(), txid5: txid.values }),
            };
            let resp: Msg<Response> = round_trip_frame(sp, &req)?;
            let (chal, sig) = match resp.msg {
                Response::OkCheetahSig { chal, sig } => (chal, sig),
                Response::Err { code } => return Err(anyhow!("SignTxId failed: code {}", code)),
                _ => return Err(anyhow!("unexpected response to SignTxId")),
            };

            let schnorr_sig = SchnorrSignature {
                chal: Chal { values: T8 { values: chal } },
                sig:  Sig  { values: T8 { values: sig  } },
            };
            sig_map.put(pk_dev.clone(), schnorr_sig);
        }

        if sig_map.wyt() > 0 {
            input.spend.signature = Some(Signature { map: sig_map });
        }
        new_inputs.put(name, input);
    }

    raw.inputs = Inputs { p: new_inputs };
    Ok(raw)
}

// safer ZSet contains using tap() (avoids Debug/NounEncode bounds on iter())
fn zset_contains<T>(zs: &ZSet<T>, x: &T) -> bool
where
    T: NounEncode + Clone + Debug + PartialEq + Ord,
{
    zs.iter().any(|t| t == x)
}

// A minimal "round_trip" variant that speaks Msg<Frame> over COBS
fn round_trip_frame(sp: &mut dyn RW, req: &Msg<Frame>) -> Result<Msg<Response>> {
    let buf = to_allocvec(req)?;
    write_cobs_frame(sp, &buf)?;
    let mut frame = read_cobs_frame(sp, 4 * 1024)?;
    let resp: Msg<Response> = from_bytes_cobs(&mut frame)?;
    if resp.v != PROTO_V1 {
        return Err(anyhow!("unsupported proto version {}", resp.v));
    }
    if resp.id != req.id {
        return Err(anyhow!("mismatched id: got {}, expected {}", resp.id, req.id));
    }
    Ok(resp)
}

// COBS I/O
fn write_cobs_frame(sp: &mut dyn RW, payload: &[u8]) -> Result<()> {
    let mut enc = vec![0u8; payload.len() + payload.len() / 254 + 2];
    let n = cobs::encode(payload, &mut enc);
    sp.write_all(&enc[..n])?;
    sp.write_all(&[0])?;
    Ok(())
}

fn read_cobs_frame(sp: &mut dyn RW, max_len: usize) -> Result<Vec<u8>> {
    let mut rx = Vec::with_capacity(128);
    let mut b = [0u8; 1];
    loop {
        match sp.read(&mut b) {
            Ok(1) => {
                rx.push(b[0]);
                if rx.len() > max_len {
                    return Err(anyhow!("frame too large (> {} bytes)", max_len));
                }
                if b[0] == 0 { return Ok(rx); }
            }
            Ok(_) => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
