use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const ADJECTIVES: &[&str] = &[
    "sleepy", "hungry", "brave", "fuzzy", "clever", "rusty",
    "happy", "sad", "angry", "curious", "gentle", "bold",
    "shy", "loud", "quiet", "bright", "dark", "swift",
    "slow", "strong", "weak", "wise", "foolish", "graceful",
];
const NOUNS: &[&str] = &[
    "tiger", "otter", "eagle", "panda", "narwhal", "beetle",
    "lion", "dolphin", "falcon", "koala", "whale", "ant",
    "elephant", "wolf", "sparrow", "dragon", "shark", "butterfly",
    "giraffe", "bear", "penguin", "zebra", "fox", "rabbit",
];
const VERBS: &[&str] = &[
    "sleeping", "eating", "running", "jumping", "flying", "swimming",
    "reading", "writing", "singing", "dancing", "drawing", "painting",
    "cooking", "baking", "gardening", "cycling", "crouching", "climbing",
    "fishing", "skating", "skiing", "surfing", "kayaking", "rowing",
];

const NOUNS2: &[&str] = &[
    "cheetah", "platypus", "hawk", "lemur", "manatee", "cricket",
    "panther", "orca", "vulture", "sloth", "seal", "termite",
    "rhinoceros", "coyote", "finch", "griffin", "barracuda", "moth",
    "kangaroo", "bison", "flamingo", "gazelle", "badger", "hedgehog",
];

pub fn generate_tx_name(input: String) -> String {
    // 1. Convert the input data to u64
    let input_data: u64 = input.as_bytes().into_iter().fold(0u64, |h, b| h.wrapping_mul(131) ^ *b as u64) | 1;
    
    // 2. Pull current time in millis:
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs() as u64;
    
    // 3. Mix in some randomness:
    let extra: u64 = rand::random();

    // 4. Combine into one 64‑bit seed (e.g. XOR or wrapping add):
    let seed = input_data ^ extra;

    // 5. Seed a small, fast RNG:
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // 6. Construct the phrases:
    let adj = ADJECTIVES[rng.gen_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.gen_range(0..NOUNS.len())];
    let verb = VERBS[rng.gen_range(0..VERBS.len())];
    let noun2 = NOUNS2[rng.gen_range(0..NOUNS2.len())];

    // 6. Combine with an underscore:
    format!("{}-{}-{}-{}-{}", now, adj, noun, verb, noun2)
}