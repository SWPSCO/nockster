use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tx_types::crypto::utils_nostd::{be32_lt, is_zero32, CHEETAH_N};
use vanity::{encode_pkh, key_json, Match, MatchMode, Mnemonic, MnemonicSearch, Prefix, Search};
use zeroize::Zeroizing;

use crate::{cli::VanityArgs, ui};
use rand::RngCore;

struct Args {
    prefix: Prefix,
    output: PathBuf,
    threads: usize,
    max_attempts: u64,
    raw_key: bool,
}

pub fn run(input: VanityArgs) -> anyhow::Result<()> {
    let mode = if input.insensitive {
        MatchMode::Insensitive
    } else {
        MatchMode::Exact
    };
    let args = Args {
        prefix: Prefix::with_mode(input.prefix.trim(), mode)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        output: input.out,
        threads: input
            .threads
            .map(usize::from)
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from)),
        max_attempts: if input.max_attempts == 0 {
            u64::MAX
        } else {
            input.max_attempts
        },
        raw_key: input.raw_key,
    };
    ui::header("vanity");
    ui::kv("prefix", ui::accent(input.prefix.trim()));
    ui::kv(
        "matching",
        if input.insensitive {
            "case and letter / digit equivalents"
        } else {
            "exact"
        },
    );
    ui::kv(
        "recovery",
        if args.raw_key {
            "secret key (no phrase)"
        } else {
            "24-word seed phrase"
        },
    );
    ui::kv("workers", args.threads.to_string());
    ui::note("Search runs locally. Recovery material is saved only to the output file.");
    if mine(args).map_err(anyhow::Error::msg)? {
        Ok(())
    } else {
        anyhow::bail!("no match within the attempt limit")
    }
}

fn random_search() -> Result<Search, String> {
    let mut secret = Zeroizing::new([0; 32]);
    loop {
        rand::rngs::OsRng
            .try_fill_bytes(secret.as_mut())
            .map_err(|e| format!("OS entropy failed: {e}"))?;
        // Rejection sampling is uniform over all nonzero group scalars.
        if !is_zero32(&secret) && be32_lt(&secret, &CHEETAH_N) {
            return Search::new(secret).map_err(|e| e.to_string());
        }
    }
}

fn create_output(path: &PathBuf) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}

struct Found {
    key: Match,
    mnemonic: Option<Mnemonic>,
}

fn write_match(file: &mut File, found: &Found) -> Result<(), String> {
    let json = key_json(&found.key, found.mnemonic.as_ref());
    file.write_all(json.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("cannot save winning key: {e}"))
}

enum WorkerSearch {
    Mnemonic(MnemonicSearch),
    Raw(Search),
}

impl WorkerSearch {
    fn random(raw_key: bool) -> Result<Self, String> {
        if raw_key {
            return random_search().map(Self::Raw);
        }
        let mut entropy = Zeroizing::new([0; 32]);
        rand::rngs::OsRng
            .try_fill_bytes(entropy.as_mut())
            .map_err(|e| format!("OS entropy failed: {e}"))?;
        Ok(Self::Mnemonic(MnemonicSearch::new(entropy)))
    }

    fn batch(&mut self, prefix: &Prefix, limit: u64) -> (u64, Option<Found>, bool) {
        match self {
            Self::Mnemonic(search) => {
                let batch = search.search_batch(prefix, limit);
                (
                    batch.attempts,
                    batch.matched.map(|found| Found {
                        key: found.key,
                        mnemonic: Some(found.mnemonic),
                    }),
                    batch.exhausted,
                )
            }
            Self::Raw(search) => {
                let batch = search.search_batch(prefix, limit);
                (
                    batch.attempts,
                    batch.matched.map(|key| Found {
                        key,
                        mnemonic: None,
                    }),
                    batch.exhausted,
                )
            }
        }
    }
}

fn mine(args: Args) -> Result<bool, String> {
    // Reserve the private file before spending time mining or generating keys.
    let mut output = create_output(&args.output)?;
    let stop = AtomicBool::new(false);
    let claimed = AtomicU64::new(0);
    let completed = AtomicU64::new(0);
    let start = Instant::now();
    let winner = std::thread::scope(|scope| -> Result<Option<Found>, String> {
        let (sender, receiver) = mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..args.threads {
            let sender = sender.clone();
            let (stop, claimed, completed) = (&stop, &claimed, &completed);
            let args = &args;
            let worker = std::thread::Builder::new().spawn_scoped(scope, move || {
                let result = (|| -> Result<(), String> {
                    let mut search = WorkerSearch::random(args.raw_key)?;
                    let batch_size = if args.raw_key { 256 } else { 1 };
                    while !stop.load(Ordering::Relaxed) {
                        let claim =
                            claimed.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                                (n < args.max_attempts)
                                    .then(|| n.saturating_add(batch_size).min(args.max_attempts))
                            });
                        let Ok(first) = claim else {
                            break;
                        };
                        let limit = batch_size.min(args.max_attempts - first);
                        let (attempts, matched, exhausted) = search.batch(&args.prefix, limit);
                        completed.fetch_add(attempts, Ordering::Relaxed);
                        if let Some(found) = matched {
                            if !stop.swap(true, Ordering::Relaxed) {
                                sender
                                    .send(found)
                                    .map_err(|_| "result receiver disconnected")?;
                            }
                            break;
                        }
                        if exhausted {
                            search = WorkerSearch::random(args.raw_key)?;
                        }
                    }
                    Ok(())
                })();
                if result.is_err() {
                    stop.store(true, Ordering::Relaxed);
                }
                result
            });
            match worker {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    stop.store(true, Ordering::Relaxed);
                    return Err(format!("cannot start worker: {error}"));
                }
            }
        }
        drop(sender);
        let winner = loop {
            match receiver.recv_timeout(Duration::from_secs(1)) {
                Ok(found) => break Some(found),
                Err(mpsc::RecvTimeoutError::Disconnected) => break None,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let count = completed.load(Ordering::Relaxed);
                    ui::info(&format!(
                        "{count} candidates · {:.0}/s · {:.1}s elapsed",
                        count as f64 / start.elapsed().as_secs_f64(),
                        start.elapsed().as_secs_f64()
                    ));
                }
            }
        };
        // Save a winning key before waiting for the remaining workers.
        if let Some(found) = &winner {
            write_match(&mut output, found)?;
        }
        for worker in workers {
            let result = worker.join().map_err(|_| "mining worker panicked")?;
            if winner.is_none() {
                result?;
            }
        }
        Ok(winner)
    })?;
    let count = completed.load(Ordering::Relaxed);
    ui::note(&format!(
        "Tested {count} candidates in {:.2}s ({:.0}/s)",
        start.elapsed().as_secs_f64(),
        count as f64 / start.elapsed().as_secs_f64()
    ));
    if let Some(found) = winner {
        ui::kv("address", ui::accent(encode_pkh(found.key.pkh).as_str()));
        ui::ok(&format!("Recovery JSON saved to {}", args.output.display()));
        Ok(true)
    } else {
        ui::note(&format!("{} is empty.", args.output.display()));
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tx_types::crypto::cheetah_nostd::{cheetah_pub_from_sk, ser_a_pt};
    use vanity::pkh_from_public_key;

    #[test]
    fn output_contains_a_recoverable_key_and_matching_address() {
        let mut secret_key = Zeroizing::new([0; 32]);
        secret_key[31] = 1;
        let public_key = cheetah_pub_from_sk(*secret_key);
        let pkh = pkh_from_public_key(&public_key);
        let found = Match {
            secret_key_be: secret_key,
            public_key,
            pkh,
        };
        let path = std::env::temp_dir().join(format!(
            "vanity-pkh-output-test-{}.json",
            std::process::id()
        ));
        let mut file = create_output(&path).unwrap();
        assert!(create_output(&path).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        }
        write_match(
            &mut file,
            &Found {
                key: Match {
                    secret_key_be: Zeroizing::new(*found.secret_key_be),
                    public_key,
                    pkh,
                },
                mnemonic: None,
            },
        )
        .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let secret_hex = saved["secret_key_hex_be"].as_str().unwrap();
        assert_eq!(secret_hex, format!("{:064x}", 1));
        let decoded = bs58::decode(saved["secret_key_base58"].as_str().unwrap())
            .into_vec()
            .unwrap();
        assert_eq!(decoded, *found.secret_key_be);
        assert_eq!(cheetah_pub_from_sk(decoded.try_into().unwrap()), public_key);
        assert_eq!(saved["pkh"].as_str().unwrap(), encode_pkh(pkh).as_str());
        assert_eq!(
            bs58::decode(saved["public_key_base58"].as_str().unwrap())
                .into_vec()
                .unwrap(),
            ser_a_pt(&public_key)
        );
        drop(file);
        std::fs::remove_file(path).unwrap();
    }
}
