use crate::commands::sign_tx;
use crate::keys::{bip39_seed_from_mnemonic, pubkey_to_b58};
use crate::serial::{open, send_blob, send_call};
use crate::util::{
    fmt_u64x5, fmt_u64x6, fmt_u64x8, load_draft_as_raw, transaction_name_from_bytes,
};
use std::fs;
use std::path::Path;
use siger_core::alloc_path as pathmod;
use siger_core::FragKind;
use siger_core::{Request, Response};

const TEST_MNEMONIC: &str =
    "around squeeze nerve chronic trophy kiwi enroll identify depth bicycle radio \
    gate critic child claim outer detect plug market visual stuff finish crime abuse";
const TEST_EXPECT_B58: &str =
    "32bePYRuJ3heGVEbznc6xSCaTymgz9bGFREaZ2dtJdnepjc6RX7cMSP8ATeT8bHTfxFmS7StDTmFHfvt9GP1PUq99pN7DcEFat9SDBpQwJbnwmhn5JHcGpLsRKp4fxfHSRy5";
const EXPECTED_TX_ID: &str =
    "8VKtRMuQRJNjCLgyGi2c6XtjDfinFKChfVENEZQiRPRfp5cVHPhVSSg";

pub fn run(port: &str, baud: u32, _seed_hex: Option<&str>, _path_str: &str) -> anyhow::Result<()> {
    use siger_core::cheetah;

    let mut sp = open(port, baud)?;

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
        ..
    } = send_call(&mut *sp, 2, Request::GetInfo)?
    {
        println!("info(before): proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}");
    }

    // 3) derive 64B seed from the hardcoded mnemonic (empty passphrase)
    let seed64 = bip39_seed_from_mnemonic(TEST_MNEMONIC, "");
    println!("seed: from hardcoded mnemonic (len=64)");

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
        ..
    } = send_call(&mut *sp, 3, Request::GetInfo)?
    {
        println!("info(after):  proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}");
    }

    // 6) fingerprint (for visibility)
    match send_call(&mut *sp, 4, Request::GetFingerprint)? {
        Response::OkFingerprint { fp4 } => println!("fingerprint: {}", hex::encode(fp4)),
        other => anyhow::bail!("unexpected: {other:?}"),
    }

    // 7) test path = m (master)
    let path = pathmod::Path::from_iter(core::iter::empty());

    // 8) device cheetah pub
    let (dev_x, dev_y) =
        match send_call(&mut *sp, 5, Request::GetCheetahPub { path: path.clone() })? {
            Response::OkCheetahPub { x, y } => {
                println!("cheetah pub.X = {}", fmt_u64x6(&x));
                println!("cheetah pub.Y = {}", fmt_u64x6(&y));
                (x, y)
            }
            other => anyhow::bail!("unexpected: {other:?}"),
        };

    // 8b) encode to base58 (0x01||Y||X a-pt -> base58) and compare to hardcoded expected
    let dev_pk_b58 = pubkey_to_b58(&(dev_x, dev_y));
    println!("cheetah pub (base58 a-pt): {dev_pk_b58}");
    let got = &dev_pk_b58;
    let want = TEST_EXPECT_B58;
    anyhow::ensure!(
        got == want,
        "pubkey base58 mismatch\n  expected: {want}\n       got: {got}"
    );
    println!("pubkey base58 match: OK");

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
            path: path.clone(),
            msg5: dummy.values,
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
            path: path.clone(),
            msg5: dummy.values,
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

    drop(sp);

    // 13) End-to-end sign known-good draft and verify transaction ID matches reference.
    let draft_path = "known-good.draft";
    let signed_path = "kg.tx";
    sign_tx::run(port, baud, draft_path, Some(signed_path), None)?;

    let signed_bytes = fs::read(signed_path)?;
    let tx_name = transaction_name_from_bytes(&signed_bytes)?;
    anyhow::ensure!(
        tx_name == EXPECTED_TX_ID,
        "signed tx id mismatch\n  expected: {}\n       got: {}",
        EXPECTED_TX_ID,
        tx_name
    );
    println!("sign-draft: tx id {tx_name}");

    let raw = load_draft_as_raw(Path::new(signed_path))?;
    let mut signed_inputs = 0usize;
    for (_, input) in raw.inputs.p.tap() {
        if let Some(sigmap) = &input.spend.signature {
            signed_inputs += sigmap.map.wyt();
        }
    }
    anyhow::ensure!(signed_inputs > 0, "device produced zero signatures");
    println!("sign-draft: signatures attached = {signed_inputs}");

    Ok(())
}
