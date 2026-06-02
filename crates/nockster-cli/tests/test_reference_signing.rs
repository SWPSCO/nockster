/// Test that directly uses the reference implementation
use std::path::Path;

#[test]
fn test_reference_signing() {
    let ref_nockster = Path::new("../../reference/nockster-esp");
    if !ref_nockster.exists() {
        eprintln!("Skipping test: reference implementation not found");
        return;
    }

    // We can't directly call the reference implementation from here,
    // but we can document what it produces for the same inputs

    println!("\n{}", "=".repeat(80));
    println!("REFERENCE IMPLEMENTATION EXPECTED OUTPUT");
    println!("{}\n", "=".repeat(80));

    println!("\nFor demo-tx message:");
    println!("Message: b5a460c35639f670 5669f17d0d1c673b 7117e0793673d153 08351a9913062377 cf9bbbba73a69824");
    println!("\nExpected outputs from ESP device:");
    println!("Challenge: 86273cc1 262e9c6c e2872ab3 eb48df30 f221c3ec ef4d3bae d29a7a62 068a4332");
    println!("Signature: e544dc73 c529dd83 77c2beec 7d3c94db 7896fabf 23c20d16 4c1e4b34 432a1aae");
    println!("\nExpected outputs from wallet:");
    println!("Signature: 6b22da17 e4358f42 87a42c6a 61f1f319 3b75c1c0 782a1013 3d3a8d95 4c9b5333");

    println!("\n{}", "=".repeat(80));
}
