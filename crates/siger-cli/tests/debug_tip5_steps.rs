/// Debug TIP5 step by step to understand the difference
use siger_core::cheetah::test_tip5_hash_words;

#[test]
fn test_tip5_simple_inputs() {
    println!("\n{}", "=".repeat(80));
    println!("TIP5 DEBUG: Simple inputs");
    println!("{}\n", "=".repeat(80));

    // Test 1: Single element [1]
    let input1 = [1u64];
    let hash1 = test_tip5_hash_words(&input1);
    println!("Input: [1]");
    println!("  Length: 1");
    println!("  Remainder: 1 % 10 = 1");
    println!("  Padding needed: 10 - 1 = 9");
    println!("  After padding: [1, 1, 0, 0, 0, 0, 0, 0, 0, 0] (10 elements)");
    println!("  Hash: {:016x?}", hash1);

    // Test 2: Two elements [1, 2]
    let input2 = [1u64, 2];
    let hash2 = test_tip5_hash_words(&input2);
    println!("\nInput: [1, 2]");
    println!("  Length: 2");
    println!("  Remainder: 2 % 10 = 2");
    println!("  Padding needed: 10 - 2 = 8");
    println!("  After padding: [1, 2, 1, 0, 0, 0, 0, 0, 0, 0] (10 elements)");
    println!("  Hash: {:016x?}", hash2);

    // Test 3: Ten elements [1..10]
    let input10 = [1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let hash10 = test_tip5_hash_words(&input10);
    println!("\nInput: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]");
    println!("  Length: 10");
    println!("  Remainder: 10 % 10 = 0");
    println!("  Padding needed: 10 - 0 = 10");
    println!("  After padding: [1..10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0] (20 elements, 2 blocks)");
    println!("  Hash: {:016x?}", hash10);

    // Test 4: Eleven elements
    let input11 = [1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    let hash11 = test_tip5_hash_words(&input11);
    println!("\nInput: [1..11]");
    println!("  Length: 11");
    println!("  Remainder: 11 % 10 = 1");
    println!("  Padding needed: 10 - 1 = 9");
    println!("  After padding: [1..11, 1, 0, 0, 0, 0, 0, 0, 0, 0] (20 elements, 2 blocks)");
    println!("  Hash: {:016x?}", hash11);

    println!("\n{}", "=".repeat(80));
    println!("Now let's check if the Hoon formula matches:");
    println!("Hoon code: (dvr (lent input) rate) gives quotient q and remainder r");
    println!("Hoon code: padding length = (dec (sub rate r))");
    println!("  For r=1: padding = (dec (sub 10 1)) = (dec 9) = 8");
    println!("  For r=2: padding = (dec (sub 10 2)) = (dec 8) = 7");
    println!("  For r=0: padding = (dec (sub 10 0)) = (dec 10) = 9");
    println!("\nWait! Hoon pads with [1, 0, ..., 0] where the total padding is rate-r");
    println!("But the NUMBER OF ZEROS is (dec (sub rate r)) = rate - r - 1");
    println!("So total padding length is: 1 + (rate - r - 1) = rate - r");
    println!("\nMy implementation always pads with: 1 + (rate - remainder - 1) = rate - remainder");
    println!("This should be correct!");
    println!("{}", "=".repeat(80));
}
