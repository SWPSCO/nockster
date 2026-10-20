//! Sign a public test fixture with the same entry point as the firmware.
use nockster_core::draft_sign::{sign_draft_v1_with_policy, FeePolicy, SignerConfig};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "sign_test_fixture INPUT.jam OUTPUT.jam HEIGHT"
    );
    let height: u64 = args[3].parse().expect("candidate height");
    let (sk_be, _) = nockster_core::cheetah::master_from_mnemonic(
        "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float",
        "",
    );
    let draft = std::fs::read(&args[1]).expect("read draft");
    let signed = sign_draft_v1_with_policy(
        &draft,
        &SignerConfig { sk_be },
        height,
        FeePolicy::default(),
    )
    .expect("sign public test fixture");
    std::fs::write(&args[2], signed).expect("write signed fixture");
}
