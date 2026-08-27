//! Retry backoff policy (open-design lesson B2 — `run-retry-policy.ts`).
//!
//! Pure, deterministic-when-random-is-injected backoff for transient failures:
//! exponential growth with a cap and **equal jitter** (half fixed + half
//! random) so concurrent retries don't synchronize. Rate-limit failures wait
//! longer than other transient classes because the upstream is explicitly
//! asking us to slow down.
//!
//! Used for render/cloud retriable failures; inject `random` in tests to make
//! schedules deterministic.

/// Base delay for rate-limit failures (upstream asked us to slow down).
pub const RATE_LIMIT_RETRY_BASE_DELAY_MS: u64 = 1_000;
/// Base delay for other transient failures.
pub const TRANSIENT_RETRY_BASE_DELAY_MS: u64 = 500;
/// Exponential growth factor per attempt.
pub const RETRY_BACKOFF_MULTIPLIER: u64 = 2;
/// Upper bound on a single backoff delay.
pub const MAX_RETRY_BACKOFF_DELAY_MS: u64 = 8_000;

/// Failure class for a retryable operation. Rate limits get a larger base
/// than generic transient errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    RateLimit,
    Transient,
}

fn base_delay_ms(category: Option<FailureCategory>) -> u64 {
    match category {
        Some(FailureCategory::RateLimit) => RATE_LIMIT_RETRY_BASE_DELAY_MS,
        _ => TRANSIENT_RETRY_BASE_DELAY_MS,
    }
}

/// Compute the delay before retry attempt `attempt_index` (1-based: the first
/// retry is 1). Equal jitter returns a value in `[delay/2, delay]` — half a
/// fixed floor plus half a random sample — so concurrent runs retry at
/// different times. Deterministic when `random` is supplied.
pub fn compute_retry_backoff_ms(
    attempt_index: u32,
    category: Option<FailureCategory>,
    random: impl Fn() -> f64,
) -> u64 {
    let exponent = attempt_index.saturating_sub(1);
    let raw = base_delay_ms(category).saturating_mul(RETRY_BACKOFF_MULTIPLIER.saturating_pow(exponent));
    let capped = raw.min(MAX_RETRY_BACKOFF_DELAY_MS);
    let half = capped / 2;
    let sample = (random)().clamp(0.0, 1.0);
    half + (half as f64 * sample) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_retry_uses_half_to_full_base_delay() {
        // attempt 1 → base 500ms → jitter in [250, 500].
        let lo = compute_retry_backoff_ms(1, None, || 0.0);
        let hi = compute_retry_backoff_ms(1, None, || 1.0);
        assert_eq!(lo, 250);
        assert_eq!(hi, 500);
    }

    #[test]
    fn grows_exponentially_and_caps() {
        let at = |n: u32| compute_retry_backoff_ms(n, None, || 1.0);
        assert_eq!(at(1), 500);
        assert_eq!(at(2), 1_000);
        assert_eq!(at(3), 2_000);
        assert_eq!(at(4), 4_000);
        assert_eq!(at(5), 8_000);
        assert_eq!(at(6), 8_000, "capped at MAX_RETRY_BACKOFF_DELAY_MS");
    }

    #[test]
    fn rate_limit_gets_larger_base() {
        let lo = compute_retry_backoff_ms(1, Some(FailureCategory::RateLimit), || 0.0);
        assert_eq!(lo, 500); // half of 1000
        assert!(lo > compute_retry_backoff_ms(1, None, || 0.0));
    }

    #[test]
    fn random_out_of_range_is_clamped() {
        // A misbehaving random source can't produce negative or oversized delays.
        let lo = compute_retry_backoff_ms(1, None, || -5.0);
        let hi = compute_retry_backoff_ms(1, None, || 42.0);
        assert_eq!(lo, 250);
        assert_eq!(hi, 500);
    }

    #[test]
    fn statistical_spread_avoids_synchronized_retries() {
        // With a real RNG, equal jitter spreads samples across the range
        // [half, cap] — attempt 3 → [1000, 2000]. Assert samples land in both
        // halves rather than clustering.
        let mut in_lower_half = 0;
        for _ in 0..100 {
            let d = compute_retry_backoff_ms(3, None, rand::random::<f64>);
            assert!((1_000..=2_000).contains(&d), "out of jitter range: {d}");
            if d < 1_500 {
                in_lower_half += 1;
            }
        }
        assert!(
            (20..=80).contains(&in_lower_half),
            "jitter should spread retries; only {in_lower_half}/100 in lower half"
        );
    }
}