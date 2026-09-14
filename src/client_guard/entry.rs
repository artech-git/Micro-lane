use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use super::breaker::{new_client_breaker, ClientCircuitBreaker};
use super::time::now_ms;
use super::token_bucket::TokenBucket;

pub struct ClientEntry {
    bucket: Mutex<TokenBucket>,
    pub breaker: ClientCircuitBreaker,
    last_seen_ms: AtomicI64,
}

impl ClientEntry {
    pub(super) fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            bucket: Mutex::new(TokenBucket::new(capacity, refill_per_sec)),
            breaker: new_client_breaker(),
            last_seen_ms: AtomicI64::new(now_ms()),
        }
    }

    pub(super) fn touch(&self) {
        self.last_seen_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub(super) fn try_consume(&self) -> bool {
        self.bucket.lock().unwrap().try_consume()
    }

    pub(super) fn is_stale(&self, cutoff_ms: i64) -> bool {
        self.last_seen_ms.load(Ordering::Relaxed) <= cutoff_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_entry_stamps_a_recent_timestamp_and_starts_with_a_full_bucket() {
        let before = now_ms();
        let entry = ClientEntry::new(5.0, 1.0);
        let after = now_ms();

        let seen = entry.last_seen_ms.load(Ordering::Relaxed);
        assert!(seen >= before && seen <= after);
        assert!(entry.try_consume());
    }

    #[test]
    fn touch_refreshes_last_seen() {
        let entry = ClientEntry::new(5.0, 1.0);
        entry.last_seen_ms.store(0, Ordering::Relaxed); // simulate a long-idle entry

        entry.touch();

        assert!(entry.last_seen_ms.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn is_stale_compares_last_seen_against_the_given_cutoff() {
        let entry = ClientEntry::new(5.0, 1.0);
        entry.last_seen_ms.store(1_000, Ordering::Relaxed);

        assert!(
            entry.is_stale(1_000),
            "last_seen at or before the cutoff counts as stale"
        );
        assert!(entry.is_stale(1_500));
        assert!(
            !entry.is_stale(999),
            "last_seen after the cutoff must not be stale"
        );
    }

    #[test]
    fn bucket_exhaustion_does_not_affect_the_breaker() {
        let entry = ClientEntry::new(1.0, 1.0);

        assert!(entry.try_consume());
        assert!(!entry.try_consume(), "single-token bucket should now be empty");
        assert!(
            entry.breaker.is_call_permitted(),
            "a fresh breaker must start closed regardless of rate-limit state"
        );
    }
}
