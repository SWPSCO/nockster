use anyhow::{bail, ensure, Context, Result};
use app_store_connect::notary_api::SubmissionResponseStatus;
use apple_codesign::{
    cryptography::{parse_pfx_data, PrivateKey},
    stapling::Stapler,
    NotarizationUpload, Notarizer, SigningSettings, UnifiedSigner,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    env,
    ffi::OsStr,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const TEAM_ID: &str = "484J85QW9N";
const NOTARIZATION_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
const NOTARIZATION_POLL_INTERVAL: Duration = Duration::from_secs(60);

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

fn wait_for_notarization(
    mut poll: impl FnMut() -> Result<SubmissionResponseStatus>,
    elapsed: impl Fn() -> Duration,
    mut sleep: impl FnMut(Duration),
) -> Result<()> {
    loop {
        match poll()? {
            SubmissionResponseStatus::Accepted => return Ok(()),
            SubmissionResponseStatus::InProgress => {}
            status => bail!("Notarization {status}"),
        }
        let remaining = NOTARIZATION_TIMEOUT.saturating_sub(elapsed());
        ensure!(
            !remaining.is_zero(),
            "Notarization timed out after six hours"
        );
        sleep(NOTARIZATION_POLL_INTERVAL.min(remaining));
    }
}

fn notarize(notary: &Notarizer, path: &Path) -> Result<()> {
    println!(
        "Notarizing {} (up to six hours, polling once a minute)",
        path.display()
    );
    // Submit once and poll explicitly to control the interval.
    let submission_id = match notary.notarize_path(path, None)? {
        NotarizationUpload::UploadId(id) => id,
        NotarizationUpload::NotaryResponse(response) => response.data.id,
    };
    println!("Notarization submission: {submission_id}");
    let started = Instant::now();
    wait_for_notarization(
        || {
            let status = notary
                .get_submission(&submission_id)?
                .data
                .attributes
                .status;
            println!(
                "Notarization {submission_id}: {status} after {}s",
                started.elapsed().as_secs()
            );
            if status != SubmissionResponseStatus::InProgress {
                println!(
                    "Notarization log: {}",
                    notary.fetch_notarization_log(&submission_id)?
                );
            }
            Ok(status)
        },
        || started.elapsed(),
        std::thread::sleep,
    )
    .with_context(|| format!("Notarizing {} (submission {submission_id})", path.display()))?;
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
    fn accepts_a_submission_after_hours_of_minute_interval_polling() {
        let elapsed = std::cell::Cell::new(Duration::ZERO);
        let mut polls = 0;
        wait_for_notarization(
            || {
                polls += 1;
                Ok(if elapsed.get() >= Duration::from_secs(3 * 60 * 60) {
                    SubmissionResponseStatus::Accepted
                } else {
                    SubmissionResponseStatus::InProgress
                })
            },
            || elapsed.get(),
            |delay| {
                assert_eq!(delay, Duration::from_secs(60));
                elapsed.set(elapsed.get() + delay);
            },
        )
        .unwrap();
        assert_eq!(polls, 181);
    }

    #[test]
    fn pending_submission_times_out_at_six_hours() {
        let elapsed = std::cell::Cell::new(Duration::ZERO);
        let mut polls = 0;
        let error = wait_for_notarization(
            || {
                polls += 1;
                Ok(SubmissionResponseStatus::InProgress)
            },
            || elapsed.get(),
            |delay| {
                assert_eq!(delay, Duration::from_secs(60));
                elapsed.set(elapsed.get() + delay);
            },
        )
        .unwrap_err();
        assert_eq!(elapsed.get(), Duration::from_secs(6 * 60 * 60));
        assert_eq!(polls, 361);
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn rejected_submissions_and_api_errors_stop_without_sleeping() {
        for status in [
            SubmissionResponseStatus::Invalid,
            SubmissionResponseStatus::Rejected,
            SubmissionResponseStatus::Unknown,
        ] {
            assert!(wait_for_notarization(
                || Ok(status),
                || Duration::ZERO,
                |_| panic!("must not sleep after rejection")
            )
            .is_err());
        }
        let error = wait_for_notarization(
            || bail!("API unavailable"),
            || Duration::ZERO,
            |_| panic!("must not sleep after API failure"),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "API unavailable");
    }

    #[test]
    fn accepts_wrapped_base64_secrets_and_rejects_invalid_data() {
        assert_eq!(
            &*decode_base64(" Y2Vy\ndGlm\raWNhdGU= \t").unwrap(),
            b"certificate"
        );
        assert!(decode_base64("not a certificate!").is_err());
    }
}
