//! Proof-of-work anti-spam check (spec 006 §6).
//!
//! Work function: `BLAKE3-256(id || nonce_le64)`, difficulty is the
//! count of leading zero bits in the digest. The nonce travels in the
//! `PUB` frame beside the record, never inside the signed envelope, so
//! work is additive and re-spendable by third parties (spec's
//! "Decisions" section).

/// Count leading zero bits of a digest (most-significant byte first).
pub fn leading_zero_bits(digest: &[u8]) -> u32 {
    let mut bits = 0u32;
    for byte in digest {
        if *byte == 0 {
            bits += 8;
        } else {
            bits += byte.leading_zeros();
            break;
        }
    }
    bits
}

/// `BLAKE3-256(id || nonce_le64)`.
pub fn work_digest(id: &[u8; 32], nonce: u64) -> [u8; 32] {
    let mut input = [0u8; 40];
    input[..32].copy_from_slice(id);
    input[32..].copy_from_slice(&nonce.to_le_bytes());
    *blake3::hash(&input).as_bytes()
}

/// The proof-of-work difficulty (leading zero bits) a given `(id, nonce)`
/// pair achieves.
pub fn difficulty(id: &[u8; 32], nonce: u64) -> u32 {
    leading_zero_bits(&work_digest(id, nonce))
}

/// Whether a publication satisfies the relay's minimum PoW requirement.
///
/// `min_bits == 0` means PoW is disabled (spec §6): always accepted,
/// even with no nonce. A relay requiring PoW rejects a publication
/// with no nonce outright.
pub fn check(id: &[u8; 32], nonce: Option<u64>, min_bits: u32) -> bool {
    if min_bits == 0 {
        return true;
    }
    match nonce {
        Some(n) => difficulty(id, n) >= min_bits,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_zero_bits_known_values() {
        assert_eq!(leading_zero_bits(&[0x00, 0x0F]), 12);
        assert_eq!(leading_zero_bits(&[0xFF]), 0);
        assert_eq!(leading_zero_bits(&[0x00, 0x00, 0x01]), 23);
        assert_eq!(leading_zero_bits(&[0x00, 0x00]), 16);
    }

    #[test]
    fn pow_disabled_always_passes() {
        let id = [1u8; 32];
        assert!(check(&id, None, 0));
        assert!(check(&id, Some(0), 0));
    }

    #[test]
    fn missing_nonce_fails_when_pow_required() {
        let id = [1u8; 32];
        assert!(!check(&id, None, 1));
    }

    /// Brute-forces a small nonce that clears a low difficulty bar for a
    /// fixed id, then checks `check()` agrees at that bar and rejects a
    /// bar one bit higher than what was actually achieved.
    ///
    /// This is a self-consistency test (it does not hardcode a
    /// pre-computed BLAKE3 digest, since one was not available without
    /// executing the code). A byte-exact known-answer vector against
    /// the conformance suite's `relay/pow-*` fixtures should replace or
    /// augment this once the crate builds.
    #[test]
    fn brute_force_low_difficulty_nonce() {
        let id = [0x42u8; 32];
        let target_bits = 4;
        let mut nonce = 0u64;
        let achieved = loop {
            let d = difficulty(&id, nonce);
            if d >= target_bits {
                break d;
            }
            nonce += 1;
            assert!(nonce < 1_000_000, "did not find a nonce within budget");
        };
        assert!(check(&id, Some(nonce), target_bits));
        assert!(!check(&id, Some(nonce), achieved + 1));
    }
}
