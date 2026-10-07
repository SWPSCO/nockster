use std::process::Command;
fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nockster-cli"));
    cmd.arg("vanity");
    cmd
}
struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap().keep())
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn requires_exactly_one_match_pattern() {
    for args in [
        vec!["--out", "unused.json"],
        vec!["--prefix", "a", "--suffix", "b", "--out", "unused.json"],
        vec!["--suffix", "a", "--contains", "b", "--out", "unused.json"],
    ] {
        assert_eq!(
            command().args(args).output().unwrap().status.code(),
            Some(2)
        );
    }
    let help = command().arg("--help").output().unwrap();
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    for flag in ["--suffix", "--contains", "--continuous"] {
        assert!(text.contains(flag));
    }
}

#[test]
fn pattern_searches_save_one_or_multiple_distinct_recoverable_keys() {
    let dir = TempDir::new();
    for flag in ["--suffix", "--contains"] {
        for continuous in [false, true] {
            let path = dir.0.join(format!("{flag}-{continuous}.json"));
            let mut cmd = command();
            cmd.args([flag, "1", "--raw-key", "--max-attempts", "2000", "--out"])
                .arg(&path);
            cmd.args(["--threads", "3"]);
            if continuous {
                cmd.arg("--continuous");
            }
            let result = cmd.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let items = if continuous {
                saved.as_array().unwrap().clone()
            } else {
                assert!(saved.is_object());
                vec![saved]
            };
            if continuous {
                assert!(items.len() > 1);
                assert!(String::from_utf8_lossy(&result.stdout).contains("Tested 2000 candidates"));
            }
            let mut addresses = std::collections::HashSet::new();
            for item in &items {
                let scalar: [u8; 32] = bs58::decode(item["secret_key_base58"].as_str().unwrap())
                    .into_vec()
                    .unwrap()
                    .try_into()
                    .unwrap();
                let restored = vanity::encode_pkh(vanity::pkh_from_public_key(
                    &tx_types::crypto::cheetah_nostd::cheetah_pub_from_sk(scalar),
                ));
                let address = restored.as_str();
                assert_eq!(item["pkh"], address);
                assert!(if flag == "--suffix" {
                    address.ends_with('1')
                } else {
                    address.contains('1')
                });
                assert!(addresses.insert(address.to_owned()));
                assert!(!String::from_utf8_lossy(&result.stdout)
                    .contains(item["secret_key_hex_be"].as_str().unwrap()));
            }
        }
    }
}
