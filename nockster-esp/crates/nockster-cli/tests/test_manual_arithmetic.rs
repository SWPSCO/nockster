/// Manual test of modular arithmetic with exact values

#[test]
fn test_manual_sig_arithmetic() {
    use num_bigint::BigUint;

    let nonce_be = [
        0x70u8, 0xdd, 0xf3, 0x8c, 0xf1, 0xe7, 0x63, 0xd7, 0x91, 0xa0, 0x36, 0xab, 0xb9, 0xbe, 0x29,
        0x6c, 0x28, 0x39, 0x45, 0x89, 0xac, 0x97, 0x6a, 0x1e, 0x9f, 0xbc, 0xb4, 0xbc, 0x33, 0x40,
        0x71, 0x35,
    ];
    let chal_be = [
        0x1bu8, 0x6e, 0x2d, 0xba, 0xd3, 0xcb, 0xd8, 0x77, 0x9c, 0xfa, 0xb1, 0x83, 0xf0, 0xa7, 0xea,
        0xd2, 0x76, 0xe3, 0x87, 0x8e, 0xa6, 0xce, 0x47, 0xae, 0x5f, 0xba, 0xa2, 0x7d, 0xeb, 0x8c,
        0x34, 0xbb,
    ];
    let multiplier_be = [7u8; 32];
    let n_be = [
        0x7au8, 0xf2, 0x59, 0x9b, 0x3b, 0x3f, 0x22, 0xd0, 0x56, 0x3f, 0xbf, 0x0f, 0x99, 0x0a, 0x37,
        0xb5, 0x32, 0x7a, 0xa7, 0x23, 0x30, 0x15, 0x77, 0x22, 0xd4, 0x43, 0x62, 0x3e, 0xae, 0xd4,
        0xac, 0xcf,
    ];

    let nonce = BigUint::from_bytes_be(&nonce_be);
    let chal = BigUint::from_bytes_be(&chal_be);
    let multiplier = BigUint::from_bytes_be(&multiplier_be);
    let n = BigUint::from_bytes_be(&n_be);

    // Formula: s = (nonce + chal * multiplier) mod n
    let s = (nonce.clone() + (chal.clone() * multiplier.clone())) % n.clone();
    let s_bytes = s.to_bytes_be();
    let mut s_be = [0u8; 32];
    let offset = 32 - s_bytes.len();
    s_be[offset..].copy_from_slice(&s_bytes);

    println!("\nManual computation:");
    println!("  s = (nonce + chal*multiplier) mod n");
    println!("  Result: {:02x?}", s_be);

    // Try subtraction formula: s = (chal*multiplier - nonce) mod n
    let chal_times_multiplier = (chal * multiplier) % n.clone();
    let s2 = if chal_times_multiplier >= nonce {
        (chal_times_multiplier - nonce) % n.clone()
    } else {
        (n.clone() + chal_times_multiplier - nonce) % n.clone()
    };
    let s2_bytes = s2.to_bytes_be();
    let mut s2_be = [0u8; 32];
    let offset2 = 32 - s2_bytes.len();
    s2_be[offset2..].copy_from_slice(&s2_bytes);

    println!("\n  Try s = (chal*multiplier - nonce) mod n:");
    println!("  Result: {:02x?}", s2_be);

    // Now test our actual functions
    use nockster_core::cheetah::utils::{add_mod_n, mul_mod_n};
    let our_chal_times_multiplier = mul_mod_n(&chal_be, &multiplier_be);
    println!(
        "\n  Our mul_mod_n(chal, multiplier): {:02x?}",
        our_chal_times_multiplier
    );

    let our_s_be = add_mod_n(&nonce_be, &our_chal_times_multiplier);
    println!("  Our add_mod_n(nonce, chal*multiplier): {:02x?}", our_s_be);
    assert_eq!(our_s_be, s_be);
}
