/// Test modular arithmetic functions
use nockster_core::cheetah::utils::{add_mod_n, mul_mod_n};

#[test]
fn test_mod_arithmetic_with_known_values() {
    println!("\n{}", "=".repeat(80));
    println!("TESTING MODULAR ARITHMETIC");
    println!("{}\n", "=".repeat(80));

    // Test with small known values
    let a = [0u8; 32];
    let mut b = [0u8; 32];
    b[31] = 5; // b = 5

    let sum = add_mod_n(&a, &b);
    println!("0 + 5 mod n = {:02x?}", &sum[28..]);
    assert_eq!(sum[31], 5);

    // Test multiplication: 2 * 3 = 6
    let mut two = [0u8; 32];
    two[31] = 2;
    let mut three = [0u8; 32];
    three[31] = 3;

    let prod = mul_mod_n(&two, &three);
    println!("2 * 3 mod n = {:02x?}", &prod[28..]);
    assert_eq!(prod[31], 6);

    // Exercise modular multiplication with a fixed numeric multiplier.
    let chal_be = [
        0x06, 0x8a, 0x43, 0x32, 0xd2, 0x9a, 0x7a, 0x62, 0xef, 0x4d, 0x3b, 0xae, 0xf2, 0x21, 0xc3,
        0xec, 0x31, 0xe7, 0xa7, 0x91, 0xe3, 0xf8, 0x0c, 0x39, 0xfc, 0x3f, 0xe8, 0xcf, 0x0f, 0xe6,
        0x1e, 0x9f,
    ];

    let multiplier_be = [7u8; 32];

    println!("\nTesting with numeric multiplier:");
    println!("  Challenge: {:02x?}", &chal_be[..16]);

    let chal_times_multiplier = mul_mod_n(&chal_be, &multiplier_be);
    println!(
        "  chal * multiplier mod n: {:02x?}",
        &chal_times_multiplier[..16]
    );

    let nonce_be = [
        0x37, 0x2c, 0x54, 0x03, 0x5f, 0x09, 0xfd, 0xc6, 0x42, 0x15, 0xcc, 0x53, 0x9f, 0xcd, 0x85,
        0x1e, 0x97, 0x67, 0x3d, 0x6f, 0x32, 0x9e, 0x7d, 0xdc, 0x23, 0x74, 0x5b, 0x03, 0xd9, 0xd9,
        0xe3, 0xd0,
    ];

    println!("  Nonce: {:02x?}", &nonce_be[..16]);

    let s_be = add_mod_n(&nonce_be, &chal_times_multiplier);
    println!("  s = nonce + (chal * multiplier) mod n:");
    println!("    {:02x?}", &s_be[..16]);
    println!("    {:02x?}", &s_be[16..]);

    // Convert to T8 to compare
    let s_t8 = nockster_core::cheetah::utils::be32_atom_to_t8_le(&s_be);
    println!("  s as T8: {:08x?}", s_t8.values);

    println!("\n  Expected T8: [c86abd93, db388fef, f61859ac, 9f618b3a, d6c03b14, a8e6b7a9, 2edbb490, 25049f76]");

    println!("\n{}", "=".repeat(80));
}
