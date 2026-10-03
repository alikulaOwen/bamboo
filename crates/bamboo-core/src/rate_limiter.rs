use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Reason for a rate limiter rejection, aligning with on-chain and gateway error classifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateLimitRejectionReason {
    RateLimitExceeded,
    ServicePaused,
    ZeroTokensRequested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitRejection {
    pub reason: RateLimitRejectionReason,
    pub retry_after_secs: u64,
    pub tokens_remaining: u64,
}

/// In-memory mathematical token-bucket rate limiter.
/// Direct Rust implementation of the algorithm from `solana-rate-limiter`.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: u64,
    pub refill_rate_per_sec: u64,
    pub tokens_remaining: u64,
    pub last_refill_instant: Instant,
    pub paused: bool,
    pub total_consumed: u64,
}

impl TokenBucket {
    pub fn new(capacity: u64, refill_rate_per_sec: u64) -> Self {
        Self {
            capacity,
            refill_rate_per_sec,
            tokens_remaining: capacity,
            last_refill_instant: Instant::now(),
            paused: false,
            total_consumed: 0,
        }
    }

    /// Evaluates replenishment and consumes `tokens_requested`.
    ///
    /// Mathematical Formula:
    ///   elapsed = max(0, now - last_refill)
    ///   replenished = elapsed * refill_rate_per_sec
    ///   available = min(capacity, tokens_remaining + replenished)
    ///
    /// Returns `Ok(tokens_remaining)` on success, or `Err(RateLimitRejection)` on failure.
    pub fn try_consume(&mut self, tokens_requested: u64) -> Result<u64, RateLimitRejection> {
        if tokens_requested == 0 {
            return Err(RateLimitRejection {
                reason: RateLimitRejectionReason::ZeroTokensRequested,
                retry_after_secs: 0,
                tokens_remaining: self.tokens_remaining,
            });
        }

        if self.paused {
            return Err(RateLimitRejection {
                reason: RateLimitRejectionReason::ServicePaused,
                retry_after_secs: 60,
                tokens_remaining: self.tokens_remaining,
            });
        }

        let now = Instant::now();
        let elapsed_secs = now.duration_since(self.last_refill_instant).as_secs();
        let replenished = elapsed_secs.saturating_mul(self.refill_rate_per_sec);
        let available = self.capacity.min(self.tokens_remaining.saturating_add(replenished));

        if available < tokens_requested {
            // Update accumulated tokens so partial refill is preserved
            self.tokens_remaining = available;
            if elapsed_secs > 0 {
                self.last_refill_instant = now;
            }

            let deficit = tokens_requested - available;
            let retry_after_secs = if self.refill_rate_per_sec > 0 {
                (deficit + self.refill_rate_per_sec - 1) / self.refill_rate_per_sec
            } else {
                60
            };

            return Err(RateLimitRejection {
                reason: RateLimitRejectionReason::RateLimitExceeded,
                retry_after_secs: retry_after_secs.max(1),
                tokens_remaining: self.tokens_remaining,
            });
        }

        self.tokens_remaining = available - tokens_requested;
        self.last_refill_instant = now;
        self.total_consumed = self.total_consumed.saturating_add(tokens_requested);

        Ok(self.tokens_remaining)
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
}

/// Thread-safe multi-client token-bucket rate limiter.
/// Tracks quotas per client identifier (IP address, wallet address, or API key).
#[derive(Clone)]
pub struct MultiClientRateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    default_capacity: u64,
    default_refill_rate_per_sec: u64,
}

impl MultiClientRateLimiter {
    pub fn new(default_capacity: u64, default_refill_rate_per_sec: u64) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            default_capacity,
            default_refill_rate_per_sec,
        }
    }

    /// Checks quota for a given client key and consumes `tokens_requested`.
    pub fn check_and_consume(&self, client_key: &str, tokens_requested: u64) -> Result<u64, RateLimitRejection> {
        let mut map = self.buckets.lock().unwrap();
        let bucket = map.entry(client_key.to_string()).or_insert_with(|| {
            TokenBucket::new(self.default_capacity, self.default_refill_rate_per_sec)
        });

        bucket.try_consume(tokens_requested)
    }

    /// Sets emergency pause for a specific client bucket.
    pub fn set_client_paused(&self, client_key: &str, paused: bool) {
        let mut map = self.buckets.lock().unwrap();
        if let Some(bucket) = map.get_mut(client_key) {
            bucket.set_paused(paused);
        }
    }

    /// Cleans up buckets that haven't been accessed recently to prevent memory growth.
    pub fn prune_stale_buckets(&self, max_idle: Duration) {
        let mut map = self.buckets.lock().unwrap();
        let now = Instant::now();
        map.retain(|_, bucket| {
            now.duration_since(bucket.last_refill_instant) < max_idle
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_burst_and_rate_limit_exceeded() {
        let mut bucket = TokenBucket::new(10, 2);

        // 1. Consume 6 tokens out of 10 -> 4 remaining
        assert_eq!(bucket.try_consume(6).unwrap(), 4);

        // 2. Consume 4 tokens -> 0 remaining
        assert_eq!(bucket.try_consume(4).unwrap(), 0);

        // 3. Try consuming 1 more token -> RateLimitExceeded
        let err = bucket.try_consume(1).unwrap_err();
        assert_eq!(err.reason, RateLimitRejectionReason::RateLimitExceeded);
        assert_eq!(err.retry_after_secs, 1);
    }

    #[test]
    fn test_replenishment_over_time() {
        let mut bucket = TokenBucket::new(5, 5); // 5 tokens/sec
        assert_eq!(bucket.try_consume(5).unwrap(), 0);

        // Wait 1.1s for replenishment
        sleep(Duration::from_millis(1100));

        // Should have replenished to full capacity (5)
        assert!(bucket.try_consume(3).is_ok());
    }

    #[test]
    fn test_emergency_circuit_breaker() {
        let mut bucket = TokenBucket::new(10, 2);
        bucket.set_paused(true);

        let err = bucket.try_consume(1).unwrap_err();
        assert_eq!(err.reason, RateLimitRejectionReason::ServicePaused);

        bucket.set_paused(false);
        assert!(bucket.try_consume(1).is_ok());
    }

    #[test]
    fn test_multi_client_rate_limiter() {
        let limiter = MultiClientRateLimiter::new(3, 1);

        assert!(limiter.check_and_consume("client_a", 2).is_ok());
        assert!(limiter.check_and_consume("client_a", 1).is_ok());
        assert!(limiter.check_and_consume("client_a", 1).is_err()); // A exhausted

        // Client B is isolated and has full capacity
        assert!(limiter.check_and_consume("client_b", 3).is_ok());
        assert!(limiter.check_and_consume("client_b", 1).is_err()); // B exhausted
    }
}
