// NO bitcoin modules
// mod address;

extern crate alloc;

use std::path::Path;
use alloc::fmt::Debug;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use cobs;
use hex;
use postcard::{from_bytes_cobs, to_allocvec};
use serialport;
use std::{fs, io::{Read, Write}, time::Duration};
use siger_core::{Msg, Request, Response, PROTO_V1, Frame, FragKind, alloc_path as pathmod};

use nockapp::noun::slab::NounSlab;
use noun_serde::{NounEncode, NounDecode, NounDecodeError};
use nockvm::mem::NockStack;
use nockvm::noun::{Atom, IndirectAtom, Noun, D};
use nockvm::serialization::cue;
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
        #[arg(long, required = true)]
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
        #[arg(long, required = true)] port: String,
        #[arg(long, default_value_t = 115200)] baud: u32,
    },

    /// Device health (the firmware’s well-known test)
    Health {
        #[arg(long, required = true)] port: String,
        #[arg(long, default_value_t = 115200)] baud: u32,
    },

    /// Set the seed (64-byte hex) using inbound fragments
    Seed {
        #[arg(long, required = true)] port: String,
        #[arg(long, default_value_t = 115200)] baud: u32,
        #[arg(long, required = true)] seed_hex: String,
    },

    /// Parse a .draft (jam) and print inputs + signing plans
    Plan {
        #[arg(long, required = true)] port: String,     // required by design
        #[arg(long, default_value_t = 115200)] baud: u32,
        #[arg(long, required = true)] draft: String,
    },

    /// Send a .draft (jam) to device as FragKind::SignDraft and receive a blob back
    SignDraft {
        #[arg(long, required = true)] port: String,
        #[arg(long, default_value_t = 115200)] baud: u32,
        #[arg(long, required = true)] draft: String,
        /// Where to write the returned blob (defaults to stdout hex if omitted)
        #[arg(long)] out: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test { port, baud, seed_hex, path } => cmd_test(&port, baud, seed_hex.as_deref(), &path),
        Cmd::GetInfo { port, baud }              => cmd_getinfo(&port, baud),
        Cmd::Health { port, baud }               => cmd_health(&port, baud),
        Cmd::Seed { port, baud, seed_hex }       => cmd_seed(&port, baud, &seed_hex),
        Cmd::Plan { port, baud, draft }          => cmd_plan(&port, baud, &draft),
        Cmd::SignDraft { port, baud, draft, out }=> cmd_sign_draft(&port, baud, &draft, out.as_deref()),
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
    const CHUNK: usize = 200; // fits in 256B postcard frame
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

fn send_blob_and_recv_outbound(
    sp: &mut dyn serialport::SerialPort,
    xid: u16,
    kind: FragKind,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    // Inbound frag (host -> device)
    send_blob(sp, xid, kind, bytes)?;

    // Outbound (device -> host) begins with Response::FragBegin
    let rb: Msg<Response> = recv_msg(sp, 8 * 1024)?;
    let (msg_id, total, kind2, frag_id) = match rb.msg {
        Response::FragBegin { id, total_len, kind } => (rb.id, total_len, kind, id),
        other => return Err(anyhow!("expected outbound FragBegin, got {:?}", other)),
    };
    if kind2 != kind { return Err(anyhow!("outbound kind mismatch")); }

    // Collect parts
    let mut out = Vec::with_capacity(total as usize);
    let mut expect_off = 0u32;
    loop {
        let rp: Msg<Response> = recv_msg(sp, 8 * 1024)?;
        if rp.id != msg_id { return Err(anyhow!("msg id changed mid-stream")); }
        match rp.msg {
            Response::FragPart { id, offset, chunk, last } => {
                if id != frag_id { return Err(anyhow!("frag id mismatch")); }
                if offset != expect_off { return Err(anyhow!("offset mismatch")); }
                out.extend_from_slice(&chunk);
                expect_off += chunk.len() as u32;
                if last { break; }
            }
            other => return Err(anyhow!("expected FragPart, got {:?}", other)),
        }
    }
    if out.len() as u32 != total { return Err(anyhow!("truncated outbound frag")); }
    Ok(out)
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

    let mut rx: Vec<u8> = Vec::with_capacity(512);
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

// Read a single Msg<Response> from the wire (COBS-delimited)
fn recv_msg(sp: &mut dyn serialport::SerialPort, max_len: usize) -> Result<Msg<Response>> {
    let mut rx: Vec<u8> = Vec::with_capacity(256);
    loop {
        let mut b = [0u8; 1];
        match sp.read_exact(&mut b) {
            Ok(()) => {
                rx.push(b[0]);
                if rx.len() > max_len {
                    return Err(anyhow!("frame too large (> {} bytes)", max_len));
                }
                if b[0] == 0 { break; }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let resp: Msg<Response> = from_bytes_cobs(&mut rx)?;
    anyhow::ensure!(resp.v == PROTO_V1, "bad protocol version");
    Ok(resp)
}

// ---------- Subcommands ------------------------------------------------------

fn cmd_getinfo(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let resp: Response = send_recv(&mut *sp, 1, Frame::One(Request::GetInfo))?;
    match resp {
        Response::Info { proto_v, fw_major, fw_minor, features, has_seed } => {
            println!("info: proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}");
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    }
    Ok(())
}

fn cmd_health(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let resp: Response = send_recv(&mut *sp, 2, Frame::One(Request::Health))?;
    println!("{resp:?}");
    Ok(())
}

fn cmd_seed(port: &str, baud: u32, seed_hex: &str) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let seed = parse_64(seed_hex)?;
    let _ = send_blob_and_recv_outbound(&mut *sp, 0x42, FragKind::SetSeed, &seed)?; // device may echo via outbound frag; ignored
    println!("seed: set ({} bytes via frag)", seed.len());
    Ok(())
}

fn cmd_plan(_port: &str, _baud: u32, draft_path: &str) -> anyhow::Result<()> {
    let raw: Transaction = load_jam_as(&draft_path)?;

    // Summary based on your RawTransaction fields
    let id_str = fmt_u64x5(&raw.id.values);
    let inputs_count = raw.inputs.p.wyt();
    let tl_min = raw.timelock_range.min.as_ref().map(|p| p.value);
    let tl_max = raw.timelock_range.max.as_ref().map(|p| p.value);
    let fee = raw.total_fees.value;

    println!("draft: id={}, inputs={}, timelock=[{:?}, {:?}], total_fees={}",
        id_str, inputs_count, tl_min, tl_max, fee);

    // Per-input m-of-n + combos
    let plans = enumerate_signing_plans(&raw.inputs);
    for p in plans {
        let total_keys = raw.inputs.p.tap()
            .iter()
            .find(|(n, _)| *n == p.name)
            .map(|(_, i)| i.note.lock.pubkeys.wyt())
            .unwrap_or(0);
        println!("input {:?}: m-of-n = {} of {}, combos={}", p.name, p.m, total_keys, p.combos.len());
        for (i, combo) in p.combos.iter().enumerate() {
            println!("  combo#{i}: {} keys", combo.len());
        }
    }
    Ok(())
}

fn cmd_sign_draft(port: &str, baud: u32, draft_path: &str, out_path: Option<&str>) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let ret = send_blob_and_recv_outbound(&mut *sp, 0x99, FragKind::SignDraft, &bytes)?;
    if let Some(p) = out_path {
        fs::write(p, &ret).with_context(|| format!("write {p}"))?;
        println!("wrote {} bytes to {}", ret.len(), p);
    } else {
        println!("received {} bytes:", ret.len());
        println!("{}", hex::encode(&ret));
    }
    Ok(())
}

fn cmd_test(port: &str, baud: u32, seed_hex: Option<&str>, path_str: &str) -> anyhow::Result<()> {
    use siger_core::cheetah;

    let mut sp = open(port, baud)?;

    // 1) hello
    let caps: Response = send_call(&mut *sp, 1, Request::Hello)?;
    println!("caps: {caps:?}");

    // 2) info BEFORE seed
    if let Response::Info { proto_v, fw_major, fw_minor, features, has_seed } =
        send_call(&mut *sp, 2, Request::GetInfo)?
    {
        println!("info(before): proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}");
    }

    // 3) set seed (frag)
    let seed = seed_hex.map(parse_64).transpose()?.unwrap_or([0x11; 64]);
    let _ = send_blob_and_recv_outbound(&mut *sp, 42, FragKind::SetSeed, &seed)?;
    println!("seed: set ({} bytes via frag)", seed.len());

    // 4) info AFTER seed
    if let Response::Info { proto_v, fw_major, fw_minor, features, has_seed } =
        send_call(&mut *sp, 3, Request::GetInfo)?
    {
        println!("info(after):  proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}");
    }

    // 5) fingerprint
    match send_call(&mut *sp, 4, Request::GetFingerprint)? {
        Response::OkFingerprint { fp4 } => println!("fingerprint: {}", hex::encode(fp4)),
        other => anyhow::bail!("unexpected: {other:?}"),
    }

    // 6) parse derivation path
    let path = pathmod::Path::from_iter(
        path_str.split(',').filter(|s| !s.is_empty()).map(|s| s.parse::<u32>().unwrap())
    );

    // 7) device cheetah pub
    let (dev_x, dev_y) = match send_call(&mut *sp, 5, Request::GetCheetahPub { path: path.clone() })? {
        Response::OkCheetahPub { x, y } => {
            println!("cheetah pub.X = {}", fmt_u64x6(&x));
            println!("cheetah pub.Y = {}", fmt_u64x6(&y));
            (x, y)
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    };

    // 8) host-derive same path and compare pk
    let (sk, cc) = cheetah::master_from_seed(&seed);
    let mut xk = cheetah::XKey::from_master(sk, cc);
    for i in path.iter() { xk = cheetah::xprv_derive_child(&xk, *i); }
    let host_pk = xk.pk.unwrap();
    anyhow::ensure!(host_pk.0 == dev_x && host_pk.1 == dev_y, "device pk != host pk");
    println!("pk match: OK");

    // 9) device sign known txid
    let txid = cheetah::Hash { values: [1, 2, 3, 4, 5] };
    match send_call(
        &mut *sp, 6,
        Request::SignTxId { path: path.clone(), txid5: txid.values }
    )? {
        Response::OkCheetahSig { chal, sig } => {
            println!("sign: txid   = {}", fmt_u64x5(&txid.values));
            println!("sign: chal e = {}", fmt_u64x8(&chal));
            println!("sign: sig  s = {}", fmt_u64x8(&sig));
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    }

    // 10) sign-for self-test: device vs host
    let (e_host, s_host) = cheetah::schnorr_sign_txid(xk.sk.unwrap(), host_pk, txid);
    let (e_dev, s_dev) = match send_call(
        &mut *sp, 7,
        Request::SignTxIdFor { path: path.clone(), txid5: txid.values, pubkey: host_pk }
    )? {
        Response::OkCheetahSig { chal, sig } => (chal, sig),
        other => anyhow::bail!("unexpected: {other:?}"),
    };
    anyhow::ensure!(e_dev == e_host.values && s_dev == s_host.values, "sign-for mismatch");
    println!("self-test: OK");

    // 11) health
    match send_call(&mut *sp, 8, Request::Health)? {
        Response::OkCheetahSig { chal, sig } => {
            println!("health: chal e = {}", fmt_u64x8(&chal));
            println!("health: sig  s = {}", fmt_u64x8(&sig));
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    }

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

pub fn load_jam_noun(path: impl AsRef<Path>) -> Result<Noun> {
    let data = std::fs::read(&path)
        .with_context(|| format!("failed to read jam file {}", path.as_ref().display()))?;
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab
        .cue_into(Bytes::from(data))
        .map_err(|e| anyhow::anyhow!("cue failed: {e}"))?;
    Ok(noun))
}

/// Load a jam/draft file and decode it into a typed value.
pub fn load_jam_as<T: NounDecode>(path: impl AsRef<Path>) -> Result<T> {
    let noun = load_jam_noun(path)?;
    T::from_noun(&noun).map_err(|e: NounDecodeError| anyhow::anyhow!(e))
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

// minimal "round_trip" for Msg<Frame> (used above)
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

// generic COBS helpers for RW (used by round_trip_frame)
fn write_cobs_frame(sp: &mut dyn RW, payload: &[u8]) -> Result<()> {
    let mut enc = vec![0u8; payload.len() + payload.len() / 254 + 2];
    let n = cobs::encode(payload, &mut enc);
    sp.write_all(&enc[..n])?;
    sp.write_all(&[0])?;
    Ok(())
}

fn read_cobs_frame(sp: &mut dyn RW, max_len: usize) -> Result<Vec<u8>> {
    let mut rx = Vec::with_capacity(256);
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

// safer ZSet contains using tap()
fn zset_contains<T>(zs: &ZSet<T>, x: &T) -> bool
where
    T: NounEncode + Clone + Debug + PartialEq + Ord,
{
    zs.iter().any(|t| t == x)
}

// ---------- tiny utils -------------------------------------------------------

fn parse_64(s: &str) -> anyhow::Result<[u8; 64]> {
    let mut h = s.trim();
    if let Some(stripped) = h.strip_prefix("0x") { h = stripped; }
    let cleaned: String = h.chars().filter(|c| !c.is_whitespace() && *c != '_').collect();

    let bytes = hex::decode(&cleaned)
        .map_err(|e| anyhow::anyhow!("invalid hex for 64-byte seed: {e}"))?;
    if bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "seed must be exactly 64 bytes (got {} bytes)",
            bytes.len()
        ));
    }

    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn fmt_u64x5(v: &[u64; 5]) -> String {
    v.iter().map(|w| format!("{w:016x}")).collect::<Vec<_>>().join("_")
}
fn fmt_u64x6(v: &[u64; 6]) -> String {
    v.iter().map(|w| format!("{w:016x}")).collect::<Vec<_>>().join("_")
}
fn fmt_u64x8(v: &[u64; 8]) -> String {
    v.iter().map(|w| format!("{w:016x}")).collect::<Vec<_>>().join("_")
}
