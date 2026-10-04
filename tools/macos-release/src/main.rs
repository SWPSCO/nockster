use anyhow::{bail, ensure, Context, Result};
use apple_codesign::{
    cryptography::{parse_pfx_data, PrivateKey},
    stapling::Stapler,
    Notarizer, SigningSettings, UnifiedSigner,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{env, ffi::OsStr, fs, path::Path, process::Command, time::Duration};
use zeroize::Zeroizing;

const TEAM_ID: &str = "484J85QW9N";

fn secret(name: &str) -> Result<Zeroizing<String>> {
    let value = env::var(name).with_context(|| format!("Set the {name} GitHub Actions secret"))?;
    ensure!(!value.is_empty(), "{name} must not be empty");
    Ok(Zeroizing::new(value))
}

fn decode_secret(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    decode_base64(&secret(name)?)
        .with_context(|| format!("{name} must contain base64-encoded data"))
}

fn decode_base64(value: &str) -> Result<Zeroizing<Vec<u8>>> {
    let compact: Zeroizing<String> =
        Zeroizing::new(value.chars().filter(|c| !c.is_whitespace()).collect());
    Ok(Zeroizing::new(STANDARD.decode(compact.as_bytes())?))
}

fn run(program: &str, args: &[&OsStr]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("Run {program}"))?;
    ensure!(status.success(), "{program} failed with {status}");
    Ok(())
}

fn verify_app(app: &Path) -> Result<()> {
    run(
        "codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            app.as_os_str(),
        ],
    )?;
    run(
        "xcrun",
        &["stapler".as_ref(), "validate".as_ref(), app.as_os_str()],
    )?;
    run(
        "spctl",
        &[
            "--assess".as_ref(),
            "--type".as_ref(),
            "execute".as_ref(),
            "--verbose=2".as_ref(),
            app.as_os_str(),
        ],
    )
}

fn notarize(notary: &Notarizer, path: &Path) -> Result<()> {
    println!("Notarizing {}", path.display());
    // The library rejects an unsuccessful submission or a timeout.
    notary.notarize_path(path, Some(Duration::from_secs(1800)))?;
    Stapler::new()?.staple_path(path)?;
    Ok(())
}

fn release(app: &Path, dmg: &Path) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "Packaging and Gatekeeper verification require macOS"
    );
    ensure!(
        app.is_dir() && app.extension().is_some_and(|ext| ext == "app"),
        "Supply a built .app bundle"
    );
    ensure!(
        dmg.extension().is_some_and(|ext| ext == "dmg"),
        "Output must be a .dmg"
    );
    ensure!(!dmg.exists(), "Output already exists: {}", dmg.display());
    let (certificate, key) =
        parse_pfx_data(&decode_secret("APPLE_P12")?, &secret("APPLE_P12_PASSWORD")?)?;
    ensure!(
        certificate.time_constraints_valid(None),
        "Developer ID certificate is expired or not yet valid"
    );
    let mut settings = SigningSettings::default();
    settings.set_signing_key(key.as_key_info_signer(), certificate);
    ensure!(
        settings.chain_apple_certificates().is_some(),
        "Signing certificate must chain to Apple"
    );
    ensure!(
        settings.set_team_id_from_signing_certificate() == Some(TEAM_ID),
        "Signing certificate must belong to team {TEAM_ID}"
    );
    settings.set_time_stamp_url("http://timestamp.apple.com/ts01")?;
    settings.set_for_notarization(true);
    settings.ensure_for_notarization_settings()?;
    let signer = UnifiedSigner::new(settings);

    // A private temporary directory contains only the notarization credential;
    // the Developer ID private key stays in memory and never enters a keychain.
    let credentials = tempfile::tempdir()?;
    let key_path = credentials.path().join("AuthKey.p8");
    fs::write(&key_path, &*decode_secret("AC_API_KEY_PATH")?)?;
    let api_key = app_store_connect::UnifiedApiKey::from_ecdsa_pem_path(
        &*secret("AC_API_ISSUER_ID")?,
        &*secret("AC_API_KEY_ID")?,
        &key_path,
    )?;
    let api_path = credentials.path().join("api-key.json");
    api_key.write_json_file(&api_path)?;
    let notary = Notarizer::from_api_key(&api_path)?;

    println!("Signing {}", app.display());
    signer.sign_path_in_place(app)?;
    notarize(&notary, app)?;
    verify_app(app)?;

    let staging = tempfile::tempdir()?;
    let staged_app = staging.path().join("Nockster.app");
    run("ditto", &[app.as_os_str(), staged_app.as_os_str()])?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("/Applications", staging.path().join("Applications"))?;
    if let Some(parent) = dmg.parent() {
        fs::create_dir_all(parent)?;
    }
    run(
        "hdiutil",
        &[
            "create".as_ref(),
            "-volname".as_ref(),
            "Nockster".as_ref(),
            "-srcfolder".as_ref(),
            staging.path().as_os_str(),
            "-format".as_ref(),
            "UDZO".as_ref(),
            dmg.as_os_str(),
        ],
    )?;
    signer.sign_path_in_place(dmg)?;
    notarize(&notary, dmg)?;
    run(
        "xcrun",
        &["stapler".as_ref(), "validate".as_ref(), dmg.as_os_str()],
    )?;
    run(
        "spctl",
        &[
            "--assess".as_ref(),
            "--type".as_ref(),
            "open".as_ref(),
            "--context".as_ref(),
            "context:primary-signature".as_ref(),
            "--verbose=2".as_ref(),
            dmg.as_os_str(),
        ],
    )?;
    println!(
        "Signed, notarized, stapled, and Gatekeeper-verified: {}",
        dmg.display()
    );
    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        bail!("Usage: nockster-macos-release PATH.app OUTPUT.dmg");
    }
    release(Path::new(&args[0]), Path::new(&args[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_wrapped_base64_secrets_and_rejects_invalid_data() {
        assert_eq!(
            &*decode_base64(" Y2Vy\ndGlm\raWNhdGU= \t").unwrap(),
            b"certificate"
        );
        assert!(decode_base64("not a certificate!").is_err());
    }
}
