use crate::keys::pubkey_to_b58;
use crate::serial::{open, send_blob, send_call};
use crate::util::{fmt_u64x5, fmt_u64x6, fmt_u64x8};
use nockster_core::alloc_path as pathmod;
use nockster_core::FragKind;
use nockster_core::{Request, Response};
use sha2::{Digest, Sha512};
use std::fmt::Write as _;

fn synthetic_smoke_seed64() -> [u8; 64] {
    let digest = Sha512::digest(b"nockster-cli destructive hardware smoke seed v1");
    let mut out = [0u8; 64];
    out.copy_from_slice(&digest);
    out
}

pub fn run(
    port: &str,
    baud: u32,
    _seed_hex: Option<&str>,
    _path_str: &str,
    version: u8,
) -> anyhow::Result<()> {
    use nockster_core::cheetah;

    let mut sp = open(port, baud)?;
    let _ = send_call(&mut *sp, 0x4001, Request::SelectSeed { slot: 0 })?;

    // 0) wipe first
    let _ = send_call(&mut *sp, 0x0001, Request::Wipe)?;

    // 1) hello
    let caps: Response = send_call(&mut *sp, 1, Request::Hello)?;
    println!("caps: {caps:?}");

    // 2) info BEFORE seed
    if let Response::Info {
        proto_v,
        fw_major,
        fw_minor,
        features,
        has_seed,
        cheetah_pubs,
    } = send_call(&mut *sp, 2, Request::GetInfo)?
    {
        println!(
            "info(before): proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}, keys={}",
            cheetah_pubs.len()
        );
    }

    // 3) use a deterministic non-secret seed for hardware smoke testing.
    let seed64 = synthetic_smoke_seed64();
    println!("seed: deterministic test seed (len=64)");

    // 4) set seed via inbound frag
    send_blob(&mut *sp, 42, FragKind::SetSeed, &seed64)?;
    println!("seed: set ({} bytes via frag)", seed64.len());

    // 5) info after seed
    if let Response::Info {
        proto_v,
        fw_major,
        fw_minor,
        features,
        has_seed,
        cheetah_pubs,
    } = send_call(&mut *sp, 3, Request::GetInfo)?
    {
        println!(
            "info(after):  proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}, keys={}",
            cheetah_pubs.len()
        );
        if let Some(first) = cheetah_pubs.get(0) {
            println!(
                "info(after):  key[00] path={} X={} Y={}",
                format_path(first.path.as_slice()),
                fmt_u64x6(&first.x),
                fmt_u64x6(&first.y)
            );
        }
    }

    // 6) fingerprint (for visibility)
    match send_call(&mut *sp, 4, Request::GetFingerprint)? {
        Response::OkFingerprint { fp4 } => println!("fingerprint: {}", hex::encode(fp4)),
        other => anyhow::bail!("unexpected: {other:?}"),
    }

    // 7) test path = m (master)
    let path = pathmod::Path::from_iter(core::iter::empty());

    // 8) device cheetah pub
    let (dev_x, dev_y) = match send_call(
        &mut *sp,
        5,
        Request::GetCheetahPub {
            slot: 0,
            path: path.clone(),
        },
    )? {
        Response::OkCheetahPub { x, y } => {
            println!("cheetah pub.X = {}", fmt_u64x6(&x));
            println!("cheetah pub.Y = {}", fmt_u64x6(&y));
            (x, y)
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    };

    // 8b) encode to base58 for visibility.
    let dev_pk_b58 = pubkey_to_b58(&(dev_x, dev_y), version);
    println!("cheetah pub (base58 v{}): {dev_pk_b58}", version);

    // 9) host-derive same path and compare affine limbs
    let (sk, cc) = cheetah::master_from_seed(&seed64);
    let xk = cheetah::XKey::from_master(sk, cc);
    // path is empty: stays at master
    let host_pk = xk.pk.unwrap();
    anyhow::ensure!(
        host_pk.0 == dev_x && host_pk.1 == dev_y,
        "device pk != host pk"
    );
    println!("pk match: OK");

    // 10) device sign dummy hash
    let dummy = cheetah::Hash {
        values: [1, 2, 3, 4, 5],
    };
    match send_call(
        &mut *sp,
        6,
        Request::SignSpendHash {
            slot: 0,
            path: path.clone(),
            msg5: dummy.values,
            meta: None,
        },
    )? {
        Response::OkCheetahSig { chal, sig } => {
            println!("sign: spend  = {}", fmt_u64x5(&dummy.values));
            println!("sign: chal e = {}", fmt_u64x8(&chal));
            println!("sign: sig  s = {}", fmt_u64x8(&sig));
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    }

    // 11) sign-for self-test: device vs host
    let (e_host, s_host) = cheetah::schnorr_sign_tx(xk.sk.unwrap(), host_pk, dummy.values);
    let (e_dev, s_dev) = match send_call(
        &mut *sp,
        7,
        Request::SignSpendHash {
            slot: 0,
            path: path.clone(),
            msg5: dummy.values,
            meta: None,
        },
    )? {
        Response::OkCheetahSig { chal, sig } => (chal, sig),
        other => anyhow::bail!("unexpected: {other:?}"),
    };
    anyhow::ensure!(
        e_dev == e_host.values && s_dev == s_host.values,
        "sign-for mismatch"
    );
    println!("self-test: OK");

    // 12) health
    match send_call(&mut *sp, 8, Request::Health)? {
        Response::OkCheetahSig { chal, sig } => {
            println!("health: chal e = {}", fmt_u64x8(&chal));
            println!("health: sig  s = {}", fmt_u64x8(&sig));
        }
        other => anyhow::bail!("unexpected: {other:?}"),
    }

    println!(
        "transaction signing smoke moved to `nockster-cli smoke --sign-draft <current-bythos.draft>`"
    );
    Ok(())
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
