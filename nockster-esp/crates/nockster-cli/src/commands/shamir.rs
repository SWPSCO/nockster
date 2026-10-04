//! Offline Shamir backup of a master coil (`sk ‖ cc`). No device is touched:
//! `split` turns a coil (from hex, a zprv, or a seed) into k-of-n shares to
//! write down; `combine` reconstructs the coil from shares and prints it plus
//! its cheetah address so the operator can confirm the right key came back.
//! Restore onto a device with `seed --coil-hex <hex> --pin <pin>`.

use crate::cli::{ShamirAction, ShamirArgs};
use crate::keys;
use crate::ui;
use nockster_core::cheetah::{cheetah_pub_from_sk, master_from_seed};
use nockster_core::extended_key::parse_zprv;
use nockster_core::shamir;
use zeroize::Zeroize;

pub fn run(args: ShamirArgs) -> anyhow::Result<()> {
    match args.action {
        ShamirAction::Split {
            coil_hex,
            zprv,
            seedphrase,
            seed_hex,
            passphrase,
            threshold,
            shares,
        } => {
            let mut coil = resolve_coil(
                coil_hex.as_deref(),
                zprv.as_deref(),
                seedphrase.as_deref(),
                seed_hex.as_deref(),
                &passphrase,
            )?;
            let result = split(&coil, threshold, shares);
            coil.zeroize();
            result
        }
        ShamirAction::Combine { share } => combine(&share),
    }
}

fn resolve_coil(
    coil_hex: Option<&str>,
    zprv: Option<&str>,
    seedphrase: Option<&str>,
    seed_hex: Option<&str>,
    passphrase: &str,
) -> anyhow::Result<[u8; 64]> {
    match (coil_hex, zprv, seedphrase, seed_hex) {
        (Some(hex), None, None, None) => parse_coil_hex(hex),
        (None, Some(z), None, None) => {
            let key = parse_zprv(z).map_err(|e| anyhow::anyhow!("invalid zprv: {e:?}"))?;
            Ok(key.coil64())
        }
        (None, None, Some(phrase), None) => {
            let mut seed = keys::bip39_seed_from_mnemonic(phrase, passphrase);
            let coil = coil_from_seed(&seed);
            seed.zeroize();
            Ok(coil)
        }
        (None, None, None, Some(hex)) => {
            let bytes = hex::decode(hex.trim())
                .map_err(|_| anyhow::anyhow!("--seed-hex must be hex"))?;
            let mut seed: [u8; 64] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("seed must be 64 bytes"))?;
            let coil = coil_from_seed(&seed);
            seed.zeroize();
            Ok(coil)
        }
        _ => anyhow::bail!(
            "provide exactly one coil source: --coil-hex, --zprv, --seedphrase, or --seed-hex"
        ),
    }
}

fn parse_coil_hex(hex: &str) -> anyhow::Result<[u8; 64]> {
    let bytes = hex::decode(hex.trim()).map_err(|_| anyhow::anyhow!("--coil-hex must be hex"))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("coil must be 64 bytes (128 hex chars)"))
}

fn coil_from_seed(seed64: &[u8; 64]) -> [u8; 64] {
    let (mut sk, mut cc) = master_from_seed(seed64);
    let mut coil = [0u8; 64];
    coil[..32].copy_from_slice(&sk);
    coil[32..].copy_from_slice(&cc);
    sk.zeroize();
    cc.zeroize();
    coil
}

fn split(coil: &[u8; 64], k: u8, n: u8) -> anyhow::Result<()> {
    ui::header("shamir split");
    use rand::{rngs::OsRng, RngCore};
    let mut fill = |buf: &mut [u8]| -> Result<(), ()> {
        OsRng.fill_bytes(buf);
        Ok(())
    };
    let shares = shamir::split_coil(coil, k, n, &mut fill)
        .map_err(|e| anyhow::anyhow!("split failed: {e:?}"))?;

    ui::kv("threshold", &ui::strong(&format!("{k}-of-{n}")));
    ui::note(&format!(
        "write down all {n} shares; any {k} restore the wallet, any {} reveal nothing",
        k - 1
    ));
    ui::note("anyone with the threshold controls the funds — store shares separately");
    for (i, share) in shares.iter().enumerate() {
        ui::kv(&format!("share {}", i + 1), &ui::accent(share));
    }
    ui::note("restore with: nockster-cli shamir combine --share <s1> --share <s2> ...");
    Ok(())
}

fn combine(shares: &[String]) -> anyhow::Result<()> {
    ui::header("shamir combine");
    if shares.len() < 2 {
        anyhow::bail!("provide at least the threshold number of --share values");
    }
    let refs: Vec<&str> = shares.iter().map(|s| s.as_str()).collect();
    let mut coil = shamir::combine_shares(&refs)
        .map_err(|e| anyhow::anyhow!("combine failed: {e:?}"))?;

    // Derive the cheetah address so the operator can confirm the recovered key.
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&coil[..32]);
    let pk = cheetah_pub_from_sk(sk);
    sk.zeroize();
    let address = nockster_core::draft_sign::cheetah_pubkey_pkh_v1((pk[0], pk[1]))
        .map_err(|e| anyhow::anyhow!("address derive failed: {e:?}"))?;

    ui::kv("coil-hex", &ui::accent(&hex::encode(coil)));
    ui::kv("address", &ui::strong(&address));
    coil.zeroize();
    ui::note("restore onto a device with: nockster-cli seed --coil-hex <coil-hex> --pin <pin>");
    Ok(())
}
