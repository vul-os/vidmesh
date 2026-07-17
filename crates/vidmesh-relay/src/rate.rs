//! Per-identity token-bucket rate limiting (spec 006 §6).
//!
//! Each `author.identity_id` gets its own bucket, refilled continuously
//! at `records_per_minute_per_key / 60` tokens per second, capped at
//! `records_per_minute_per_key` (one minute of burst). A configured
//! limit of `0` disables rate limiting entirely (mirrors `pow_min_bits`
//! `0` meaning "disabled").
//!
//! Buckets live in an in-memory `HashMap` behind a `std::sync::Mutex`;
//! `check` is a fast, non-blocking, non-`await`-ing critical section,
//! so holding the lock across it is safe from an async handler. There
//! is no persistence or cross-restart memory: a relay restart resets
//! everyone's bucket to full, which is a conservative (permissive)
//! failure mode.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// A per-identity token-bucket limiter.
pub struct RateLimiter {
    /// `0` means disabled: [`RateLimiter::check`] always returns `true`.
    limit_per_minute: u32,
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<[u8; 32], Bucket>>,
}

impl RateLimiter {
    /// Build a limiter from the configured per-minute-per-key rate.
    pub fn new(records_per_minute_per_key: u32) -> Self {
        let capacity = records_per_minute_per_key as f64;
        RateLimiter {
            limit_per_minute: records_per_minute_per_key,
            capacity,
            refill_per_sec: capacity / 60.0,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Attempt to consume one token for `identity`. Returns `true` if
    /// allowed (and consumes the token), `false` if the identity is
    /// currently rate-limited.
    pub fn check(&self, identity: &[u8; 32]) -> bool {
        if self.limit_per_minute == 0 {
            return true;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");
        let bucket = buckets.entry(*identity).or_insert_with(|| Bucket {
            tokens: self.capacity,
            last_refill: now,
        });
        let elapsed = now
            .saturating_duration_since(bucket.last_refill)
            .as_secs_f64();
        bucket.tokens = refill_tokens(bucket.tokens, elapsed, self.refill_per_sec, self.capacity);
        bucket.last_refill = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Pure refill arithmetic, split out so the math is testable without
/// depending on wall-clock time passing during a test run.
fn refill_tokens(tokens: f64, elapsed_secs: f64, refill_per_sec: f64, capacity: f64) -> f64 {
    (tokens + elapsed_secs * refill_per_sec).min(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_limiter_always_allows() {
        let limiter = RateLimiter::new(0);
        let id = [1u8; 32];
        for _ in 0..1000 {
            assert!(limiter.check(&id));
        }
    }

    #[test]
    fn exhausts_then_rejects() {
        let limiter = RateLimiter::new(3);
        let id = [2u8; 32];
        assert!(limiter.check(&id));
        assert!(limiter.check(&id));
        assert!(limiter.check(&id));
        // Fourth request in the same instant has no tokens left.
        assert!(!limiter.check(&id));
    }

    #[test]
    fn buckets_are_independent_per_identity() {
        let limiter = RateLimiter::new(1);
        let a = [1u8; 32];
        let b = [2u8; 32];
        assert!(limiter.check(&a));
        assert!(!limiter.check(&a));
        // b's bucket is untouched by a's exhaustion.
        assert!(limiter.check(&b));
    }

    #[test]
    fn refill_math_caps_at_capacity() {
        assert_eq!(refill_tokens(0.0, 60.0, 1.0, 5.0), 5.0);
        assert_eq!(refill_tokens(2.0, 1.0, 1.0, 5.0), 3.0);
        assert_eq!(refill_tokens(4.5, 10.0, 1.0, 5.0), 5.0);
    }
}
