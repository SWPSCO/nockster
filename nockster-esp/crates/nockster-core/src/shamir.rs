//! Shamir Secret Sharing for the 64-byte master coil (`sk ‖ cc`).
//!
//! A coil is split into `n` shares such that any `k` reconstruct it and any
//! `k-1` reveal nothing. Sharing is byte-wise over GF(2^8) (the AES field,
//! reduction polynomial `0x11b`): for each of the 64 secret bytes an
//! independent degree-`k-1` polynomial is evaluated at the share's x-coordinate;
//! reconstruction is Lagrange interpolation at x = 0.
//!
//! Share wire format — base58check (Bitcoin alphabet, 4-byte double-SHA256
//! checksum, same idiom as `extended_key`) over a 72-byte payload:
//!
//! ```text
//! [version(1)=0x01][k(1)][n(1)][x(1)][secret_digest4(4)][y(64)]
//! ```
//!
//! `x` is the GF(2^8) x-coordinate (1..=n), also the human "share number".
//! `secret_digest4` is the first four bytes of `sha256(coil)`; it is identical
//! across every share of one split, so `combine` uses it both to reject shares
//! from different splits and to confirm the reconstructed coil is the intended
//! one (a 256-bit secret loses negligible strength to a 32-bit tag). The
//! base58check checksum guards each share against transcription errors.
//!
//! Splitting needs randomness; to keep this crate deterministic and free of a
//! getrandom dependency, [`split_coil`] takes a fill-bytes closure. Callers on
//! device/host supply a CSPRNG; tests supply fixed bytes.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Coils are the only secret this module shares.
pub const SECRET_LEN: usize = 64;
const VERSION: u8 = 0x01;
const PAYLOAD_LEN: usize = 1 + 1 + 1 + 1 + 4 + SECRET_LEN; // 72
pub const MIN_THRESHOLD: u8 = 2;
pub const MAX_SHARES: u8 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShamirError {
    /// k/n outside `2 <= k <= n <= 16`.
    BadParams,
    /// The RNG closure failed to produce bytes.
    Rng,
    /// A share string was not valid base58 / failed its checksum.
    Share,
    /// Shares disagree on version/k/n/secret_digest (mixed splits).
    Mismatch,
    /// Two shares carried the same x-coordinate.
    DuplicateShare,
    /// Fewer distinct shares than the threshold requires.
    NotEnough,
    /// Interpolation produced a coil whose digest did not match the shares'
    /// `secret_digest4` — corrupt or insufficient shares.
    Reconstruction,
}

// --- GF(2^8), reduction polynomial x^8 + x^4 + x^3 + x + 1 (0x11b) ---

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p: u8 = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

/// Multiplicative inverse in GF(2^8): a^254 (since a^255 = 1 for a != 0).
fn gf_inv(a: u8) -> u8 {
    let mut result = 1u8;
    let mut base = a;
    // exponent 254 = 0b1111_1110
    for bit in 0..8 {
        if (254 >> bit) & 1 != 0 {
            result = gf_mul(result, base);
        }
        base = gf_mul(base, base);
    }
    result
}

fn sha256_4(data: &[u8]) -> [u8; 4] {
    let digest = Sha256::digest(data);
    let mut out = [0u8; 4];
    out.copy_from_slice(&digest[..4]);
    out
}

fn check_params(k: u8, n: u8) -> Result<(), ShamirError> {
    if k < MIN_THRESHOLD || n < k || n > MAX_SHARES {
        return Err(ShamirError::BadParams);
    }
    Ok(())
}

/// Split `coil` into `n` shares with threshold `k`. `fill` must fill the given
/// buffer with cryptographically secure random bytes (`Ok(())`) or signal
/// failure (`Err(())`); it is used for the secret polynomial coefficients.
pub fn split_coil(
    coil: &[u8; SECRET_LEN],
    k: u8,
    n: u8,
    fill: &mut dyn FnMut(&mut [u8]) -> Result<(), ()>,
) -> Result<Vec<String>, ShamirError> {
    check_params(k, n)?;
    let digest = sha256_4(coil);

    // coeffs[byte][0] = secret byte; coeffs[byte][1..k] = random.
    let mut coeffs = vec![[0u8; MAX_SHARES as usize]; SECRET_LEN];
    let mut rand_buf = vec![0u8; SECRET_LEN * (k as usize - 1)];
    fill(&mut rand_buf).map_err(|_| ShamirError::Rng)?;
    for (byte_idx, row) in coeffs.iter_mut().enumerate() {
        row[0] = coil[byte_idx];
        for j in 1..k as usize {
            row[j] = rand_buf[byte_idx * (k as usize - 1) + (j - 1)];
        }
    }
    rand_buf.zeroize();

    let mut shares = Vec::with_capacity(n as usize);
    for share_no in 1..=n {
        let x = share_no; // x-coordinate, never 0
        let mut payload = Vec::with_capacity(PAYLOAD_LEN);
        payload.push(VERSION);
        payload.push(k);
        payload.push(n);
        payload.push(x);
        payload.extend_from_slice(&digest);
        for row in coeffs.iter() {
            // Horner evaluation of the byte's polynomial at x.
            let mut acc = 0u8;
            for j in (0..k as usize).rev() {
                acc = gf_mul(acc, x) ^ row[j];
            }
            payload.push(acc);
        }
        shares.push(encode_share(&payload));
        payload.zeroize();
    }
    for row in coeffs.iter_mut() {
        row.zeroize();
    }
    Ok(shares)
}

fn encode_share(payload: &[u8]) -> String {
    debug_assert_eq!(payload.len(), PAYLOAD_LEN);
    let checksum = {
        let first = Sha256::digest(payload);
        Sha256::digest(first)
    };
    let mut buf = Vec::with_capacity(PAYLOAD_LEN + 4);
    buf.extend_from_slice(payload);
    buf.extend_from_slice(&checksum[..4]);
    let s = bs58::encode(&buf).into_string();
    buf.zeroize();
    s
}

struct ParsedShare {
    k: u8,
    n: u8,
    x: u8,
    digest: [u8; 4],
    y: [u8; SECRET_LEN],
}

fn decode_share(s: &str) -> Result<ParsedShare, ShamirError> {
    let mut raw = bs58::decode(s.trim().as_bytes())
        .into_vec()
        .map_err(|_| ShamirError::Share)?;
    if raw.len() != PAYLOAD_LEN + 4 {
        raw.zeroize();
        return Err(ShamirError::Share);
    }
    let (payload, checksum) = raw.split_at(PAYLOAD_LEN);
    let expect = {
        let first = Sha256::digest(payload);
        Sha256::digest(first)
    };
    if expect[..4] != *checksum || payload[0] != VERSION {
        raw.zeroize();
        return Err(ShamirError::Share);
    }
    let mut parsed = ParsedShare {
        k: payload[1],
        n: payload[2],
        x: payload[3],
        digest: [payload[4], payload[5], payload[6], payload[7]],
        y: [0u8; SECRET_LEN],
    };
    parsed.y.copy_from_slice(&payload[8..8 + SECRET_LEN]);
    raw.zeroize();
    if parsed.x == 0 || parsed.k < MIN_THRESHOLD || parsed.n < parsed.k {
        return Err(ShamirError::Share);
    }
    Ok(parsed)
}

/// Inspect a share without combining: `(k, n, x, digest_hex_first4)`.
pub struct ShareInfo {
    pub threshold: u8,
    pub total: u8,
    pub index: u8,
    pub secret_digest: [u8; 4],
}

pub fn share_info(s: &str) -> Result<ShareInfo, ShamirError> {
    let p = decode_share(s)?;
    Ok(ShareInfo {
        threshold: p.k,
        total: p.n,
        index: p.x,
        secret_digest: p.digest,
    })
}

/// Reconstruct the coil from `k` (or more) shares of one split.
pub fn combine_shares(shares: &[&str]) -> Result<[u8; SECRET_LEN], ShamirError> {
    if shares.is_empty() {
        return Err(ShamirError::NotEnough);
    }
    let mut parsed: Vec<ParsedShare> = Vec::with_capacity(shares.len());
    for s in shares {
        parsed.push(decode_share(s)?);
    }

    // All shares must come from the same split.
    let first = &parsed[0];
    let (k, n, digest) = (first.k, first.n, first.digest);
    for p in parsed.iter() {
        if p.k != k || p.n != n || p.digest != digest {
            zeroize_shares(&mut parsed);
            return Err(ShamirError::Mismatch);
        }
    }
    // Distinct x-coordinates only.
    for i in 0..parsed.len() {
        for j in (i + 1)..parsed.len() {
            if parsed[i].x == parsed[j].x {
                zeroize_shares(&mut parsed);
                return Err(ShamirError::DuplicateShare);
            }
        }
    }
    if (parsed.len() as u8) < k {
        zeroize_shares(&mut parsed);
        return Err(ShamirError::NotEnough);
    }

    // Lagrange interpolation at x = 0, byte-wise. Using more than k consistent
    // shares is harmless (over-determined but agreeing).
    let xs: Vec<u8> = parsed.iter().map(|p| p.x).collect();
    let mut secret = [0u8; SECRET_LEN];
    for (byte_idx, out) in secret.iter_mut().enumerate() {
        let mut acc = 0u8;
        for (i, p) in parsed.iter().enumerate() {
            // Lagrange basis L_i(0) = prod_{m!=i} x_m / (x_m - x_i); in GF(2)
            // subtraction is XOR.
            let mut num = 1u8;
            let mut den = 1u8;
            for (m, &xm) in xs.iter().enumerate() {
                if m == i {
                    continue;
                }
                num = gf_mul(num, xm);
                den = gf_mul(den, xm ^ xs[i]);
            }
            let basis = gf_mul(num, gf_inv(den));
            acc ^= gf_mul(p.y[byte_idx], basis);
        }
        *out = acc;
    }

    zeroize_shares(&mut parsed);

    if sha256_4(&secret) != digest {
        secret.zeroize();
        return Err(ShamirError::Reconstruction);
    }
    Ok(secret)
}

fn zeroize_shares(parsed: &mut [ParsedShare]) {
    for p in parsed.iter_mut() {
        p.y.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic "RNG": fills with a fixed byte pattern. Fine for tests —
    // the security property under test is reconstruction, not unpredictability.
    fn fill_pattern(buf: &mut [u8]) -> Result<(), ()> {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(31).wrapping_add(7);
        }
        Ok(())
    }

    fn sample_coil() -> [u8; SECRET_LEN] {
        let mut coil = [0u8; SECRET_LEN];
        for (i, b) in coil.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(0x42);
        }
        coil
    }

    #[test]
    fn gf_inverse_is_consistent() {
        for a in 1u8..=255 {
            assert_eq!(gf_mul(a, gf_inv(a)), 1, "inv of {a}");
        }
    }

    #[test]
    fn any_k_of_n_reconstructs() {
        let coil = sample_coil();
        let shares = split_coil(&coil, 3, 5, &mut fill_pattern).unwrap();
        assert_eq!(shares.len(), 5);

        // Every 3-subset of the 5 shares reconstructs.
        for a in 0..5 {
            for b in (a + 1)..5 {
                for c in (b + 1)..5 {
                    let subset = [shares[a].as_str(), shares[b].as_str(), shares[c].as_str()];
                    assert_eq!(combine_shares(&subset).unwrap(), coil, "{a}{b}{c}");
                }
            }
        }
        // All five also agree.
        let all: Vec<&str> = shares.iter().map(|s| s.as_str()).collect();
        assert_eq!(combine_shares(&all).unwrap(), coil);
    }

    #[test]
    fn fewer_than_threshold_is_rejected() {
        let coil = sample_coil();
        let shares = split_coil(&coil, 3, 5, &mut fill_pattern).unwrap();
        // Two shares of a 3-of-5 split: not enough by count.
        let two = [shares[0].as_str(), shares[1].as_str()];
        assert_eq!(combine_shares(&two), Err(ShamirError::NotEnough));
    }

    #[test]
    fn two_of_two_and_two_of_three() {
        let coil = sample_coil();
        for n in 2..=3u8 {
            let shares = split_coil(&coil, 2, n, &mut fill_pattern).unwrap();
            let subset = [shares[0].as_str(), shares[n as usize - 1].as_str()];
            assert_eq!(combine_shares(&subset).unwrap(), coil);
        }
    }

    #[test]
    fn tampered_share_fails_checksum() {
        let coil = sample_coil();
        let shares = split_coil(&coil, 2, 3, &mut fill_pattern).unwrap();
        let mut bytes: Vec<u8> = shares[0].bytes().collect();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'z' { b'y' } else { b'z' };
        let tampered = core::str::from_utf8(&bytes).unwrap();
        let combo = [tampered, shares[1].as_str()];
        assert!(matches!(combine_shares(&combo), Err(ShamirError::Share)));
    }

    #[test]
    fn mixed_splits_are_rejected() {
        let coil_a = sample_coil();
        let mut coil_b = sample_coil();
        coil_b[0] ^= 0xff;
        let a = split_coil(&coil_a, 2, 3, &mut fill_pattern).unwrap();
        let b = split_coil(&coil_b, 2, 3, &mut fill_pattern).unwrap();
        // One share from each split: different secret_digest -> Mismatch.
        let combo = [a[0].as_str(), b[1].as_str()];
        assert_eq!(combine_shares(&combo), Err(ShamirError::Mismatch));
    }

    #[test]
    fn duplicate_share_is_rejected() {
        let coil = sample_coil();
        let shares = split_coil(&coil, 2, 3, &mut fill_pattern).unwrap();
        let combo = [shares[0].as_str(), shares[0].as_str()];
        assert_eq!(combine_shares(&combo), Err(ShamirError::DuplicateShare));
    }

    #[test]
    fn share_info_reports_params() {
        let coil = sample_coil();
        let shares = split_coil(&coil, 3, 5, &mut fill_pattern).unwrap();
        let info = share_info(&shares[2]).unwrap();
        assert_eq!(info.threshold, 3);
        assert_eq!(info.total, 5);
        assert_eq!(info.index, 3); // 1-based share number
    }

    #[test]
    fn bad_params_rejected() {
        let coil = sample_coil();
        assert_eq!(
            split_coil(&coil, 1, 3, &mut fill_pattern),
            Err(ShamirError::BadParams)
        );
        assert_eq!(
            split_coil(&coil, 4, 3, &mut fill_pattern),
            Err(ShamirError::BadParams)
        );
        assert_eq!(
            split_coil(&coil, 2, 17, &mut fill_pattern),
            Err(ShamirError::BadParams)
        );
    }
}
