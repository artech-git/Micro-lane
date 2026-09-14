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
