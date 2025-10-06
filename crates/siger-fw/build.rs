fn main() {
    if let Err(e) = generate_boot_logo() {
        panic!("failed to prepare boot logo: {e}");
    }

    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn generate_boot_logo() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, io::Write, path::PathBuf, string::String};

    const SOURCE: &str = "../../nockster-flip.png";

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
