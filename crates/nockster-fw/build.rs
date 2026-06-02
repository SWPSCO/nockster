use std::path::Path;
use std::process::Command;

fn main() {
    if let Err(e) = generate_boot_logo() {
        panic!("failed to prepare boot logo: {e}");
    }

    emit_build_info();
    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn emit_build_info() {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-env-changed=NOCKSTER_BUILD_PROFILE");
    println!("cargo:rerun-if-env-changed=NOCKSTER_RELEASE_VERSION");
    println!("cargo:rerun-if-env-changed=NOCKSTER_UPDATE_PUBKEY_SHA256_HEX");

    let git_commit = git_output(["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let git_dirty = git_dirty();
    let build_profile = std::env::var("NOCKSTER_BUILD_PROFILE").unwrap_or_else(|_| {
        if std::env::var_os("CARGO_FEATURE_CHIP_SECURITY").is_some() {
            "chip-security".to_string()
        } else {
            "dev".to_string()
        }
    });
    let tx_types_rev =
        tx_types_rev(Path::new("../../Cargo.toml")).unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=NOCKSTER_GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=NOCKSTER_GIT_DIRTY={}", u8::from(git_dirty));
    println!("cargo:rustc-env=NOCKSTER_BUILD_PROFILE={build_profile}");
    println!(
        "cargo:rustc-env=NOCKSTER_RELEASE_VERSION={}",
        release_version()
    );
    println!("cargo:rustc-env=NOCKSTER_TX_TYPES_REV={tx_types_rev}");

    if let Ok(anchor) = std::env::var("NOCKSTER_UPDATE_PUBKEY_SHA256_HEX") {
        let cleaned: String = anchor
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_' && *c != ':')
            .collect();
        let cleaned = cleaned.strip_prefix("0x").unwrap_or(&cleaned);
        if cleaned.is_empty() {
            return;
        }
        if cleaned.len() != 64 || !cleaned.bytes().all(|b| b.is_ascii_hexdigit()) {
            panic!("NOCKSTER_UPDATE_PUBKEY_SHA256_HEX must be 32 bytes of hex");
        }
        println!("cargo:rustc-env=NOCKSTER_UPDATE_PUBKEY_SHA256_HEX={cleaned}");
    }
}

fn release_version() -> String {
    let value = std::env::var("NOCKSTER_RELEASE_VERSION").unwrap_or_else(|_| "0".to_string());
    let parsed: u32 = value
        .parse()
        .unwrap_or_else(|_| panic!("NOCKSTER_RELEASE_VERSION must be a u32, got {value:?}"));
    parsed.to_string()
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_string())
}

fn git_dirty() -> bool {
    let tracked_dirty = Command::new("git")
        .args(["diff", "--quiet", "--ignore-submodules"])
        .status()
        .map(|status| !status.success())
        .unwrap_or(false);

    let staged_dirty = Command::new("git")
        .args(["diff", "--cached", "--quiet", "--ignore-submodules"])
        .status()
        .map(|status| !status.success())
        .unwrap_or(false);

    tracked_dirty || staged_dirty
}

fn tx_types_rev(path: &Path) -> Option<String> {
    let workspace = std::fs::read_to_string(path).ok()?;
    for line in workspace.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("tx-types =") {
            continue;
        }
        let rev_start = trimmed.find("rev = \"")? + "rev = \"".len();
        let rev_tail = &trimmed[rev_start..];
        let rev_end = rev_tail.find('"')?;
        return Some(rev_tail[..rev_end].to_string());
    }
    None
}

fn generate_boot_logo() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, io::Write, path::PathBuf, string::String};

    const SOURCE: &str = "nockster-flip.png";

    println!("cargo:rerun-if-changed={SOURCE}");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let src_path = manifest_dir.join(SOURCE);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    let image = image::open(&src_path)?.to_rgba8();
    let (width, height) = image.dimensions();

    let mut raw: Vec<u8> = Vec::with_capacity((width as usize) * (height as usize) * 2);
    for pixel in image.pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = u16::from(a);

        // Pre-multiply to avoid halos if the PNG has transparency
        let apply_alpha = |channel: u8| -> u16 {
            let scaled = u32::from(channel) * u32::from(alpha);
            ((scaled + 127) / 255) as u16
        };

        let r = apply_alpha(r);
        let g = apply_alpha(g);
        let b = apply_alpha(b);

        let r5 = ((u32::from(r) * 31 + 127) / 255) as u16;
        let g6 = ((u32::from(g) * 63 + 127) / 255) as u16;
        let b5 = ((u32::from(b) * 31 + 127) / 255) as u16;

        let value = (r5 << 11) | (g6 << 5) | b5;
        raw.push((value >> 8) as u8);
        raw.push(value as u8);
    }

    let raw_path = out_dir.join("nockster_boot_logo.rgb565");
    fs::write(&raw_path, &raw)?;

    let mut meta = String::new();
    meta.push_str(&format!(
        "pub const BOOT_LOGO_WIDTH: u16 = {width} as u16;\n"
    ));
    meta.push_str(&format!(
        "pub const BOOT_LOGO_HEIGHT: u16 = {height} as u16;\n"
    ));

    let raw_path_str = raw_path
        .to_str()
        .ok_or("non-utf8 boot logo path")?
        .replace('\\', r"\\");

    meta.push_str(&format!(
        "pub const BOOT_LOGO: &[u8] = include_bytes!(r#\"{}\"#);\n",
        raw_path_str
    ));

    let mut meta_file = fs::File::create(out_dir.join("boot_logo.rs"))?;
    meta_file.write_all(meta.as_bytes())?;

    Ok(())
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                "esp_wifi_preempt_enable"
                | "esp_wifi_preempt_yield_task"
                | "esp_wifi_preempt_task_create" => {
                    eprintln!();
                    eprintln!("💡 `esp-wifi` has no scheduler enabled. Make sure you have the `builtin-scheduler` feature enabled, or that you provide an external scheduler.");
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!("💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests");
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
