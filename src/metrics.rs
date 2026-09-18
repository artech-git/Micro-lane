use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Identifies one counter in [`Metrics`].
///
/// Deliberately absent: a `queries_total` counter. Every query increments exactly one of
/// the three outcome counters, so the total is their sum — see [`Metrics::queries_total`].
/// Storing it separately would cost a second atomic RMW per query for information the
/// other three already carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Counter {
    QueriesOk = 0,
    QueriesServfail,
    QueriesFormerr,
    UpstreamTimeouts,
    CircuitBreakerRejections,
    ClientRateLimited,
    ClientCircuitRejections,
}

// Must match the number of `Counter` variants; `Metrics::new` and the indexing in
// `incr`/`get` both depend on it.
const COUNTER_COUNT: usize = 7;

pub struct Metrics {
    counters: [AtomicU64; COUNTER_COUNT],
    circuit_open: AtomicBool,
    start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            counters: std::array::from_fn(|_| AtomicU64::new(0)),
            circuit_open: AtomicBool::new(false),
            start_time: Instant::now(),
        }
    }

    /// Call sites pass a literal variant, so the index constant-folds and the bounds
    /// check disappears after inlining.
    #[inline]
    pub fn incr(&self, counter: Counter) {
        self.counters[counter as usize].fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn get(&self, counter: Counter) -> u64 {
        self.counters[counter as usize].load(Ordering::Relaxed)
    }

    /// Derived rather than stored. The three loads are not a consistent snapshot, but
    /// these are monotonic counters and metrics consumers already tolerate that skew.
    #[inline]
    pub fn queries_total(&self) -> u64 {
        self.get(Counter::QueriesOk)
            + self.get(Counter::QueriesServfail)
            + self.get(Counter::QueriesFormerr)
    }

    /// Stores only on an actual transition. `lookup` reports "closed" after every
    /// successful upstream response, and a blind store would invalidate this cache line
    /// in every other core each time; the load is a cheap shared read.
    #[inline]
    pub fn set_circuit_open(&self, open: bool) {
        if self.circuit_open.load(Ordering::Relaxed) != open {
            self.circuit_open.store(open, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn is_circuit_open(&self) -> bool {
        self.circuit_open.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_start_at_zero_and_count_independently() {
        let m = Metrics::new();

        assert_eq!(m.get(Counter::QueriesOk), 0);

        m.incr(Counter::QueriesOk);
        m.incr(Counter::QueriesOk);
        m.incr(Counter::UpstreamTimeouts);

        assert_eq!(m.get(Counter::QueriesOk), 2);
        assert_eq!(m.get(Counter::UpstreamTimeouts), 1);
        assert_eq!(
            m.get(Counter::QueriesServfail),
            0,
            "counters must not alias each other"
        );
    }

    #[test]
    fn queries_total_is_the_sum_of_the_three_outcomes() {
        let m = Metrics::new();

        m.incr(Counter::QueriesOk);
        m.incr(Counter::QueriesOk);
        m.incr(Counter::QueriesServfail);
        m.incr(Counter::QueriesFormerr);

        assert_eq!(m.queries_total(), 4);
    }

    #[test]
    fn non_outcome_counters_do_not_contribute_to_queries_total() {
        let m = Metrics::new();

        // A rate-limited or breaker-rejected packet never reaches an outcome, so it must
        // not inflate the derived total.
        m.incr(Counter::ClientRateLimited);
        m.incr(Counter::CircuitBreakerRejections);
        m.incr(Counter::UpstreamTimeouts);

        assert_eq!(m.queries_total(), 0);
    }

    #[test]
    fn circuit_flag_round_trips_and_is_idempotent() {
        let m = Metrics::new();
        assert!(!m.is_circuit_open());

        m.set_circuit_open(true);
        assert!(m.is_circuit_open());

        // Repeated same-value writes are skipped internally but must not change the value.
        m.set_circuit_open(true);
        assert!(m.is_circuit_open());

        m.set_circuit_open(false);
        assert!(!m.is_circuit_open());
    }
}
