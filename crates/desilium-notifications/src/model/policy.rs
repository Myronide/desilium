//! Expiry defaults and per-application rate limiting.

use std::collections::HashMap;

use super::Urgency;
use super::limits::MAX_EXPIRE_MS;

/// Auto-expiry for low urgency when the client sends `-1`.
pub const LOW_TIMEOUT_MS: u64 = 5_000;
/// Auto-expiry for normal urgency when the client sends `-1`.
pub const NORMAL_TIMEOUT_MS: u64 = 8_000;
/// Applications tracked by the rate limiter before the least recent is evicted.
pub const MAX_TRACKED_APPS: usize = 64;

/// Default timeout per urgency; critical notifications never expire on their own.
pub fn default_timeout_ms(urgency: Urgency) -> Option<u64> {
    match urgency {
        Urgency::Low => Some(LOW_TIMEOUT_MS),
        Urgency::Normal => Some(NORMAL_TIMEOUT_MS),
        Urgency::Critical => None,
    }
}

/// Resolves the `expire_timeout` argument: `-1` is the server default, `0` never expires.
///
/// Critical notifications ignore the client's request and stay until closed,
/// so an application cannot make an alert vanish before the user saw it.
pub fn expiry_ms(urgency: Urgency, requested: i32) -> Option<u64> {
    if urgency == Urgency::Critical {
        return None;
    }
    match requested {
        0 => None,
        ms if ms > 0 => Some(
            u64::try_from(ms)
                .unwrap_or(MAX_EXPIRE_MS)
                .min(MAX_EXPIRE_MS),
        ),
        _ => default_timeout_ms(urgency),
    }
}

/// Token bucket parameters: `burst` notifications at once, then one per `refill_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub burst: u32,
    pub refill_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: u32,
    refilled_at: u64,
}

/// Per-application token buckets with bounded memory.
#[derive(Debug)]
pub struct RateLimiter {
    limit: RateLimit,
    buckets: HashMap<String, Bucket>,
}

impl RateLimiter {
    /// Creates a limiter with every bucket initially full.
    pub fn new(limit: RateLimit) -> Self {
        Self {
            limit,
            buckets: HashMap::new(),
        }
    }

    /// Takes one token for `app` at `now_ms`; `false` means the notification is throttled.
    pub fn allow(&mut self, app: &str, now_ms: u64) -> bool {
        let limit = self.limit;
        if !self.buckets.contains_key(app) && self.buckets.len() >= MAX_TRACKED_APPS {
            self.evict_least_recent();
        }
        let bucket = self.buckets.entry(app.to_string()).or_insert(Bucket {
            tokens: limit.burst,
            refilled_at: now_ms,
        });
        refill(bucket, limit, now_ms);
        if bucket.tokens == 0 {
            return false;
        }
        bucket.tokens -= 1;
        true
    }

    fn evict_least_recent(&mut self) {
        let oldest = self
            .buckets
            .iter()
            .min_by_key(|(_, bucket)| bucket.refilled_at)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            self.buckets.remove(&key);
        }
    }
}

fn refill(bucket: &mut Bucket, limit: RateLimit, now_ms: u64) {
    let interval = limit.refill_ms.max(1);
    let elapsed = now_ms.saturating_sub(bucket.refilled_at);
    let earned = elapsed / interval;
    if earned == 0 {
        return;
    }
    let gained = u32::try_from(earned.min(u64::from(limit.burst))).unwrap_or(limit.burst);
    bucket.tokens = bucket.tokens.saturating_add(gained).min(limit.burst);
    bucket.refilled_at = if bucket.tokens == limit.burst {
        now_ms
    } else {
        bucket
            .refilled_at
            .saturating_add(earned.saturating_mul(interval))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: RateLimit = RateLimit {
        burst: 3,
        refill_ms: 1_000,
    };

    #[test]
    fn urgency_defaults_match_the_brief() {
        assert_eq!(default_timeout_ms(Urgency::Low), Some(5_000));
        assert_eq!(default_timeout_ms(Urgency::Normal), Some(8_000));
        assert_eq!(default_timeout_ms(Urgency::Critical), None);
    }

    #[test]
    fn minus_one_uses_urgency_default() {
        assert_eq!(expiry_ms(Urgency::Low, -1), Some(5_000));
        assert_eq!(expiry_ms(Urgency::Normal, -1), Some(8_000));
        assert_eq!(expiry_ms(Urgency::Normal, -7), Some(8_000));
    }

    #[test]
    fn zero_means_never_expire() {
        assert_eq!(expiry_ms(Urgency::Normal, 0), None);
    }

    #[test]
    fn explicit_timeouts_are_honoured_and_capped() {
        assert_eq!(expiry_ms(Urgency::Normal, 1_500), Some(1_500));
        assert_eq!(expiry_ms(Urgency::Low, i32::MAX), Some(MAX_EXPIRE_MS));
    }

    #[test]
    fn critical_never_expires_even_when_requested() {
        assert_eq!(expiry_ms(Urgency::Critical, -1), None);
        assert_eq!(expiry_ms(Urgency::Critical, 2_000), None);
    }

    #[test]
    fn bucket_allows_burst_then_throttles() {
        let mut limiter = RateLimiter::new(LIMIT);
        assert!(limiter.allow("app", 0));
        assert!(limiter.allow("app", 0));
        assert!(limiter.allow("app", 0));
        assert!(!limiter.allow("app", 0));
    }

    #[test]
    fn bucket_refills_one_token_per_interval() {
        let mut limiter = RateLimiter::new(LIMIT);
        for _ in 0..3 {
            assert!(limiter.allow("app", 0));
        }
        assert!(!limiter.allow("app", 999));
        assert!(limiter.allow("app", 1_000));
        assert!(!limiter.allow("app", 1_500));
        assert!(limiter.allow("app", 2_000));
    }

    #[test]
    fn refill_never_exceeds_burst() {
        let mut limiter = RateLimiter::new(LIMIT);
        assert!(limiter.allow("app", 0));
        assert!(limiter.allow("app", 1_000_000));
        assert!(limiter.allow("app", 1_000_000));
        assert!(limiter.allow("app", 1_000_000));
        assert!(!limiter.allow("app", 1_000_000));
    }

    #[test]
    fn applications_are_limited_independently() {
        let mut limiter = RateLimiter::new(LIMIT);
        for _ in 0..3 {
            assert!(limiter.allow("noisy", 0));
        }
        assert!(!limiter.allow("noisy", 0));
        assert!(limiter.allow("quiet", 0));
    }

    #[test]
    fn clock_going_backwards_does_not_grant_tokens() {
        let mut limiter = RateLimiter::new(LIMIT);
        for _ in 0..3 {
            assert!(limiter.allow("app", 5_000));
        }
        assert!(!limiter.allow("app", 1_000));
    }

    #[test]
    fn tracked_applications_are_bounded() {
        let mut limiter = RateLimiter::new(LIMIT);
        for index in 0..(MAX_TRACKED_APPS as u64 * 2) {
            limiter.allow(&format!("app-{index}"), index);
        }
        assert_eq!(limiter.buckets.len(), MAX_TRACKED_APPS);
    }
}
