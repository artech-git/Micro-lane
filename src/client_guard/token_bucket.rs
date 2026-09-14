use std::time::Instant;

pub(super) struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub(super) fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    pub(super) fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn allows_bursts_up_to_capacity_then_blocks() {
        let mut bucket = TokenBucket::new(3.0, 1.0);

        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(
            !bucket.try_consume(),
            "a fourth immediate call must be blocked once capacity is exhausted"
        );
    }

    #[test]
    fn refills_over_time() {
        // 20 tokens/sec => roughly one token every 50ms.
        let mut bucket = TokenBucket::new(1.0, 20.0);

        assert!(bucket.try_consume());
        assert!(!bucket.try_consume());

        sleep(Duration::from_millis(80));

        assert!(
            bucket.try_consume(),
            "bucket should have refilled at least one token after waiting past the refill interval"
        );
    }

    #[test]
    fn refill_never_exceeds_capacity() {
        let mut bucket = TokenBucket::new(2.0, 1000.0);

        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(!bucket.try_consume());

        // Plenty of time for the (fast) refill rate to overshoot capacity if uncapped.
        sleep(Duration::from_millis(50));

        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(
            !bucket.try_consume(),
            "tokens must be capped at capacity, not accumulate without bound while idle"
        );
    }
}
