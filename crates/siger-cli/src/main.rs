mod keys;
extern crate alloc;

use std::path::{Path, PathBuf};
use std::collections::BTreeMap;
use alloc::fmt::Debug;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use clap::{Parser, Subcommand, Args};
use cobs;
use hex;
use postcard::{from_bytes_cobs, to_allocvec};
use serialport;
use std::{fs, io::{Read, Write}, time::Duration};
use siger_core::{Msg, Request, Response, PROTO_V1, Frame, FragKind, alloc_path as pathmod};

use nockapp::noun::slab::NounSlab;
use noun_serde::{NounEncode, NounDecode};
use nockvm::mem::NockStack;
use nockvm::noun::{Atom, IndirectAtom, Noun};
use nockapp::AtomExt;
use nockvm::serialization::cue;
use tx_types::collections::{ZMap, ZSet};
use tx_types::transaction_types::*;
use tx_types::RawTransaction;

pub struct InputSigningPlan {
  pub name: NName,
  pub m: u64,
  pub combos: alloc::vec::Vec<alloc::vec::Vec<SchnorrPubkey>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LockKey(Lock);

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

    #[command(subcommand)]
    Keys(KeysCmd),

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

    /// Inspect a .draft/.tx file: typed summary + optional raw noun dump
    Inspect {
      /// Path to the jammed noun (wallet tx, raw-tx, tx, or [name inputs])
      #[arg(long, required = true)]
      draft: String,
      /// Also dump the raw noun tree
      #[arg(long, default_value_t = false)]
      dump_noun: bool,
      /// Max recursive depth for noun dump
      #[arg(long, default_value_t = 6)]
      max_depth: usize,
      /// Max children shown per cell/list at each level
      #[arg(long, default_value_t = 16)]
      max_items: usize,
  },
}

#[derive(Subcommand)]
enum KeysCmd {
    /// Import a key and write it to disk for the ESP32
    Import(ImportArgs),
}

// in ImportArgs
#[derive(Args)]
struct ImportArgs {
    /// Output base path (writes <out>.json and <out>.bin)
    #[arg(long)]
    out: PathBuf,

    /// BIP-39 mnemonic phrase (conflicts with --sk-b58 and --sk-hex)
    #[arg(long, conflicts_with_all=&["sk_b58","sk_hex"])]
    mnemonic: Option<String>,

    /// Optional BIP-39 passphrase
    #[arg(long, default_value="")]
    passphrase: String,

    /// Derivation path (only used with --mnemonic). Default m/44'/0'/0'/0/0
    #[arg(long, default_value="m/44'/0'/0'/0/0")]
    path: String,

    /// Base58-encoded 32-byte private key (conflicts with --mnemonic and --sk-hex)
    #[arg(long, conflicts_with_all=&["mnemonic","sk_hex"])]
    sk_b58: Option<String>,

    /// Hex-encoded 32-byte private key (conflicts with --mnemonic and --sk-b58)
    #[arg(long, conflicts_with_all=&["mnemonic","sk_b58"])]
    sk_hex: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test { port, baud, seed_hex, path } => cmd_test(&port, baud, seed_hex.as_deref(), &path),
        Cmd::GetInfo { port, baud }              => cmd_getinfo(&port, baud),
        Cmd::Health { port, baud }               => cmd_health(&port, baud),
        Cmd::Seed { port, baud, seed_hex }       => cmd_seed(&port, baud, &seed_hex),
        Cmd::Keys(keys_cmd) => {
            match keys_cmd {
                KeysCmd::Import(args) => {
                    cmd_keys_import(args).map_err(|e| anyhow::anyhow!(e))
                }
            }
        }
        Cmd::Plan { port, baud, draft }          => cmd_plan(&port, baud, &draft),
        Cmd::Inspect { draft, dump_noun, max_depth, max_items } =>
        cmd_inspect(&draft, dump_noun, max_depth, max_items),
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
  let raw = load_draft_as_raw(Path::new(draft_path))?;

  let id_str = fmt_u64x5(&raw.id.values);
  let inputs_count = raw.inputs.p.wyt();
  let tl_min = raw.timelock_range.min.as_ref().map(|p| p.value);
  let tl_max = raw.timelock_range.max.as_ref().map(|p| p.value);
  let fee = raw.total_fees.value;

  println!(
      "draft: id={}, inputs={}, timelock=[{:?}, {:?}], total_fees={}",
      id_str, inputs_count, tl_min, tl_max, fee
  );

  let plans = enumerate_signing_plans(&raw.inputs);
  for p in plans {
      let total_keys = raw.inputs.p.tap()
          .iter()
          .find(|(n, _)| *n == p.name)
          .map(|(_, i)| i.note.lock.pubkeys.wyt())
          .unwrap_or(0);
      println!(
          "input {:?}: m-of-n = {} of {}, combos={}",
          p.name.to_hash().to_base58(), p.m, total_keys, p.combos.len()
      );
      for (i, combo) in p.combos.iter().enumerate() {
          println!("  combo#{i}: {} keys", combo.len());
      }
  }

  Ok(())
}

fn cmd_keys_import(args: ImportArgs) -> Result<(), String> {
  let (key, blob) = if let Some(m) = args.mnemonic.as_deref() {
      keys::import_from_mnemonic(m, &args.passphrase, &args.path)?
  } else if let Some(b58) = args.sk_b58.as_deref() {
      keys::import_from_b58_priv(b58)?
  } else if let Some(hex) = args.sk_hex.as_deref() {
      keys::import_from_hex_priv(hex)?
  } else {
      return Err("provide one of --mnemonic, --sk-b58, or --sk-hex".into());
  };

  let (json_path, bin_path) = keys::write_key_files(&args.out, &key, &blob)?;
  println!("✔ wrote key JSON to {}", json_path.display());
  println!("✔ wrote device blob to {}", bin_path.display());
  println!("pubkey (b58): {}", key.pk_b58);
  if let Some(p) = &key.path { println!("path: {}", p); }
  Ok(())
}

fn cmd_inspect(draft_path: &str, dump_noun: bool, max_depth: usize, max_items: usize) -> anyhow::Result<()> {
  use std::fs;

  let data = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;

  // Keep allocator alive while noun is in scope
  let mut slab: NounSlab = NounSlab::new();
  let noun = slab
      .cue_into(Bytes::from(data.clone()))
      .map_err(|e| anyhow!("cue failed: {e:?}"))?;

  // Basic shape
  println!("file: {draft_path}");
  println!("shape: {}", debug_shape(&noun));

  // Try all known shapes → RawTransaction
  let raw = match RawTransaction::from_noun(&mut slab, &noun) {
      Ok(r) => {
          println!("detected: raw-tx:transact");
          r
      }
      Err(_) => {
          // tx:transact — head is raw
          if let Ok(cell) = noun.as_cell() {
              if let Ok(r) = RawTransaction::from_noun(&mut slab, &cell.head()) {
                  println!("detected: tx:transact (head is raw-tx)");
                  r
              } else {
                  // wallet transaction:wt (has .p inputs and .name)
                  if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
                      println!("detected: transaction:wt (wallet wrapper)");
                      transaction_to_raw(&tx_wallet)
                  } else {
                      // bare [name inputs]
                      if let Ok(cell2) = noun.as_cell() {
                          if let Ok(inputs) = Inputs::from_noun(&cell2.tail()) {
                              println!("detected: [name inputs] pair");
                              raw_from_inputs(inputs)
                          } else {
                              return Err(anyhow!("unrecognized noun shape; cannot decode as any transaction form"));
                          }
                      } else {
                          return Err(anyhow!("unrecognized noun shape; not a cell and not raw-tx"));
                      }
                  }
              }
          } else {
              // not a cell; try wallet or give up
              if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
                  println!("detected: transaction:wt (wallet wrapper)");
                  transaction_to_raw(&tx_wallet)
              } else {
                  return Err(anyhow!("unrecognized noun shape; cannot decode as any transaction form"));
              }
          }
      }
  };

  // Typed summary (good for eyeballing vs known-good)
  print_raw_details(&raw);

  // Optional full noun dump (raw tree)
  if dump_noun {
      println!("\n-- raw noun dump --");
      let s = pretty_noun(&noun, max_depth, max_items);
      println!("{s}");
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

fn debug_shape(n: &Noun) -> String {
  if let Ok(cell) = n.as_cell() {
      format!("[{:?} ..]", cell.head())
  } else if let Ok(atom) = n.as_atom() {
      match atom.to_bytes_until_nul() {
          Ok(b) => format!("atom(cord:{:?})", String::from_utf8_lossy(&b)),
          _ => format!("atom({} bits)", nockvm::serialization::met0_usize(atom)),
      }
  } else {
      "direct".into()
  }
}

fn print_raw_details(raw: &RawTransaction) {
  let id_str = fmt_u64x5(&raw.id.values);
  let inputs_count = raw.inputs.p.wyt();
  let tl_min = raw.timelock_range.min.as_ref().map(|p| p.value);
  let tl_max = raw.timelock_range.max.as_ref().map(|p| p.value);
  let fee = raw.total_fees.value;

  println!("raw-tx:");
  println!("  id           = {}", raw.id.to_base58());
  println!("  inputs       = {}", inputs_count);
  println!("  timelock     = [{:?}, {:?}]", tl_min, tl_max);
  println!("  total_fees   = {}", fee);

  for (idx, (name, input)) in raw.inputs.p.tap().into_iter().enumerate() {
      println!("  - input[{}]:", idx);
      let name = &input.note.name;
      if name.p.len() >= 2 {
          println!(
              "      name        = [{:?} {:?}]",
              name.p[0].to_base58(),
              name.p[1].to_base58()
          );
      } else {
          println!("      name        = <unexpected arity {}>", name.p.len());
      }
      println!("      origin_page = {}", input.note.meta.origin_page.value);
      println!("      assets      = {}", input.note.assets.value);

      // Source
      let src_hash = fmt_u64x5(&input.note.source.p.values);
      println!("      source      = {{ hash={}, coinbase={} }}", src_hash, input.note.source.is_coinbase);

      // Timelock (range this note can spend, absolute)
      let (i_min, i_max) = input.calculate_timelock_range();
      println!("      timelock    = [{:?}, {:?}]", i_min, i_max);

      // Show up to 8 pubkeys (x only) for brevity
      let (m, pks_b58) = input.note.lock.to_b58();
      println!("      lock        = {}-of-{} signers", m, pks_b58.len());
      for (i, pk) in pks_b58.iter().enumerate() {
          println!("        pk[{}] = {}", i, pk);
      }

      // Spend / fee / sigs
      println!("      fee         = {}", input.spend.fee.value);
      let sig_count = input.spend.signature.as_ref().map(|m| m.map.wyt()).unwrap_or(0);
      println!("      signatures  = {}", sig_count);

      if let Some(sigmap) = &input.spend.signature {
          for (sidx, (pk, sig)) in sigmap.map.tap().into_iter().take(8).enumerate() {
              let x = fmt_u64x6(&pk.x.values);
              let chal = fmt_u64x8(&sig.chal.values.values);
              let s    = fmt_u64x8(&sig.sig.values.values);
              println!("        sig[{sidx}] pk.X={x} chal={chal} s={s}{}", if sidx==7 && sig_count>8 {" …"} else {""});
          }
      }
      print_input_seeds(&input);
  }
  summarize_outputs(raw);
}

fn summarize_outputs(tx: &RawTransaction) -> BTreeMap<LockKey, u128> {
  let mut by_lock: BTreeMap<LockKey, u128> = BTreeMap::new();

  for (_name, input) in tx.inputs.p.iter_kv() {
      for seed in input.spend.seeds.set.iter() {
          *by_lock.entry(LockKey(seed.recipient.clone()))
              .or_insert(0) += seed.gift.value as u128;
      }
  }
  by_lock
}

fn print_input_seeds(input: &Input) {
  for (k, seed) in input.spend.seeds.set.iter().enumerate() {
      let (m, pks_b58) = seed.recipient.to_b58();
      println!("      seed[{k}]: gift = {}, to {m}-of-{}", seed.gift.value, pks_b58.len());
      for (j, pk) in pks_b58.iter().enumerate() {
          println!("        pk[{j}] = {pk}");
      }
      println!("        parent = {}", seed.parent_hash.to_base58());
  }
}

fn print_outputs(tx: &RawTransaction) {
  let outs = summarize_outputs(tx);
  println!("outputs (derived from seeds): {}", outs.len());
  for (i, (LockKey(lock), amt)) in outs.iter().enumerate() {
      let (m, pks_b58) = lock.to_b58();
      println!("  out[{i}]: gift = {amt}, to {m}-of-{}", pks_b58.len());
      for (j, pk) in pks_b58.iter().enumerate() {
          println!("    pk[{j}] = {pk}");
      }
  }
}

fn pretty_noun(n: &Noun, max_depth: usize, max_items: usize) -> String {
  fn is_printable_ascii(bytes: &[u8]) -> bool {
      bytes.iter().all(|&b| (b == 0x09) || (b == 0x0A) || (b == 0x0D) || (0x20..=0x7E).contains(&b))
  }

  fn fmt_atom(atom: &Atom) -> String {
      // cord if it has a terminating NUL and is printable
      if let Ok(bytes) = atom.to_bytes_until_nul() {
          let b: Vec<u8> = bytes.to_vec();
          if is_printable_ascii(&b) {
              return format!("atom(cord:\"{}\")", String::from_utf8_lossy(&b));
          }
      }
      // otherwise: show small atoms as hex, big ones summarized
      let nbits = nockvm::serialization::met0_usize(atom.clone());
      let nbytes = (nbits + 7) / 8;
      if nbytes <= 64 {
          let mut v = vec![0u8; nbytes];
          let _ = atom.as_bitslice();
          format!("atom({} bytes, 0x{})", nbytes, hex::encode(v))
      } else {
          format!("atom({} bytes)", nbytes)
      }
  }

  fn try_collect_list(mut n: Noun, max_items: usize) -> Option<(Vec<Noun>, bool)> {
      let mut out = Vec::new();
      for _ in 0..max_items {
          if let Ok(cell) = n.as_cell() {
              out.push(cell.head());
              n = cell.tail();
              if let Ok(a) = n.as_atom() {
                  if a.as_u64() == Ok(0) { return Some((out, false)); }
                }
          } else {
              return None;
          }
      }
      Some((out, true)) // truncated
  }

  fn go(n: Noun, depth: usize, max_depth: usize, max_items: usize, indent: usize) -> String {
      if depth >= max_depth {
          return "...".into();
      }
      if let Ok(a) = n.as_atom() {
          return fmt_atom(&a);
      }
      if let Ok(c) = n.as_cell() {
          // try render as list if shape matches
          if let Some((els, truncated)) = try_collect_list(n, max_items) {
              let mut s = String::new();
              s.push_str("[\n");
              for (i, el) in els.into_iter().enumerate() {
                  s.push_str(&" ".repeat(indent + 2));
                  s.push_str(&go(el, depth + 1, max_depth, max_items, indent + 2));
                  if i + 1 < max_items { s.push('\n'); }
              }
              if truncated {
                  s.push_str(&" ".repeat(indent + 2));
                  s.push_str("…\n");
              }
              s.push_str(&" ".repeat(indent));
              s.push(']');
              return s;
          }
          // generic cell (head .. tail)
          let mut s = String::new();
          s.push_str("[\n");
          s.push_str(&" ".repeat(indent + 2));
          s.push_str(&go(c.head(), depth + 1, max_depth, max_items, indent + 2));
          s.push_str(",\n");
          s.push_str(&" ".repeat(indent + 2));
          s.push_str(&go(c.tail(), depth + 1, max_depth, max_items, indent + 2));
          s.push_str("\n");
          s.push_str(&" ".repeat(indent));
          s.push(']');
          return s;
      }
      "<?>".into()
  }

  go(n.clone(), 0, max_depth, max_items, 0)
}

fn load_draft_as_raw(path: &Path) -> anyhow::Result<RawTransaction> {
  let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

  // Keep allocator alive during decode
  let mut slab: NounSlab = NounSlab::new();
  let noun = slab
      .cue_into(Bytes::from(data))
      .map_err(|e| anyhow!("cue failed: {e:?}"))?;

  // raw-tx
  if let Ok(raw) = RawTransaction::from_noun(&mut slab, &noun) {
      return Ok(raw);
  }

  // tx:transact — head is raw-tx
  if let Ok(cell) = noun.as_cell() {
      if let Ok(raw) = RawTransaction::from_noun(&mut slab, &cell.head()) {
          return Ok(raw);
      }
  }

  // wallet transaction:wt (`p` (inputs) and `name`)
  if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
      return Ok(transaction_to_raw(&tx_wallet));
  }

  // 4) Naked pair: [name inputs]
  if let Ok(cell) = noun.as_cell() {
      if let Ok(inputs) = Inputs::from_noun(&cell.tail()) {
          let raw = raw_from_inputs(inputs);
          return Ok(raw);
      }
  }

  Err(anyhow!(
      "decode failed (shape {}): not RawTransaction / tx:transact / transaction:wt / [name inputs]",
      debug_shape(&noun)
  ))
}


fn raw_from_inputs(inputs: Inputs) -> RawTransaction {
  let total_fees = sum_inputs_fees(&inputs);
  let tl = TimelockRange { min: None, max: None };

  // No Default for Hash → seed zeros, then compute real id.
  let mut raw = RawTransaction {
      id: Hash { values: [0u64; 5] },
      inputs,
      timelock_range: tl,
      total_fees: Coins { value: total_fees },
  };
  raw.id = raw.compute_id();
  raw
}

fn decode_cord_like(n: Noun) -> Option<String> {
  // Try cord (bytes up to NUL) → UTF-8 string
  n.as_atom().ok()
      .and_then(|a| a.to_bytes_until_nul().ok())
      .map(|b| String::from_utf8_lossy(&b).to_string())
}


pub fn transaction_to_raw(tx: &Transaction) -> RawTransaction {
  let inputs = tx.p.clone();
  let total_fees = sum_inputs_fees(&inputs);
  let tl = union_inputs_timelock_range(&inputs);
  RawTransaction::new(inputs, tl, Coins { value: total_fees })
}

fn sum_inputs_fees(inputs: &Inputs) -> u64 {
  inputs.p.tap().into_iter().fold(0u64, |acc, (_n, i)| {
      acc.saturating_add(i.spend.fee.value)
  })
}

fn union_inputs_timelock_range(inputs: &Inputs) -> TimelockRange {
  let mut min_page: Option<u64> = None;
  let mut max_page: Option<u64> = None;
  for (_name, input) in inputs.p.tap().into_iter() {
      let (i_min, i_max) = input.calculate_timelock_range();
      if let Some(v) = i_min {
          min_page = Some(min_page.map_or(v, |m| m.min(v)));
      }
      if let Some(v) = i_max {
          max_page = Some(max_page.map_or(v, |m| m.max(v)));
      }
  }
  TimelockRange {
      min: min_page.map(|v| PageNumber { value: v }),
      max: max_page.map(|v| PageNumber { value: v }),
  }
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
    signer_paths: Vec<Vec<u32>>,
) -> Result<RawTransaction> {
    let mut raw = load_draft_as_raw(Path::new(draft_path))?;
    let txid = raw.compute_id();
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
        let _req = Msg { v: PROTO_V1, id: 0x4100, msg: Frame::One(Request::GetCheetahPub { path: path.clone() }) };
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