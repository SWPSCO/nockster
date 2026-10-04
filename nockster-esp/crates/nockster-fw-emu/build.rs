use std::{env, fs, io::Write, path::PathBuf};

fn main() {
    if let Err(err) = generate_boot_logo() {
        panic!("failed to prepare boot logo: {err}");
    }
}

fn generate_boot_logo() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let src_path = manifest_dir.join("../nockster-fw/nockster-flip.png");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    println!("cargo:rerun-if-changed={}", src_path.display());

    let image = image::open(&src_path)?.to_rgba8();
    let (width, height) = image.dimensions();

    let mut raw: Vec<u8> = Vec::with_capacity((width as usize) * (height as usize) * 2);
    for pixel in image.pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = u16::from(a);
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

    let raw_path_str = raw_path
        .to_str()
        .ok_or("non-utf8 boot logo path")?
        .replace('\\', r"\\");

    let mut meta = String::new();
    meta.push_str(&format!(
        "pub const BOOT_LOGO_WIDTH: u16 = {width} as u16;\n"
    ));
    meta.push_str(&format!(
        "pub const BOOT_LOGO_HEIGHT: u16 = {height} as u16;\n"
    ));
    meta.push_str(&format!(
        "pub const BOOT_LOGO: &[u8] = include_bytes!(r#\"{}\"#);\n",
        raw_path_str
    ));

    let mut meta_file = fs::File::create(out_dir.join("boot_logo.rs"))?;
    meta_file.write_all(meta.as_bytes())?;

    Ok(())
}
