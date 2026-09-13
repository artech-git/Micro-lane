use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

pub struct Metrics {
    pub queries_total: AtomicU64,
    pub queries_ok: AtomicU64,
    pub queries_servfail: AtomicU64,
    pub queries_formerr: AtomicU64,
    pub upstream_timeouts: AtomicU64,
    pub circuit_breaker_rejections: AtomicU64,
    pub circuit_open: AtomicBool,
    pub start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            queries_total: AtomicU64::new(0),
            queries_ok: AtomicU64::new(0),
            queries_servfail: AtomicU64::new(0),
            queries_formerr: AtomicU64::new(0),
            upstream_timeouts: AtomicU64::new(0),
            circuit_breaker_rejections: AtomicU64::new(0),
            circuit_open: AtomicBool::new(false),
            start_time: Instant::now(),
        }
    }
    #[inline]
    pub fn record_query(&self) {
        self.queries_total.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_query_ok(&self) {
        self.queries_ok.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_query_servfail(&self) {
        self.queries_servfail.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_query_formerr(&self) {
        self.queries_formerr.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_upstream_timeout(&self) {
        self.upstream_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_circuit_breaker_rejection(&self) {
        self.circuit_breaker_rejections.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn set_circuit_open(&self, open: bool) {
        self.circuit_open.store(open, Ordering::Relaxed);
    }

    #[inline]
    pub fn is_circuit_open(&self) -> bool {
        self.circuit_open.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    #[inline]
    pub fn queries_total(&self) -> u64 {
        self.queries_total.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn queries_ok(&self) -> u64 {
        self.queries_ok.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn queries_servfail(&self) -> u64 {
        self.queries_servfail.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn queries_formerr(&self) -> u64 {
        self.queries_formerr.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn upstream_timeouts(&self) -> u64 {
        self.upstream_timeouts.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn circuit_breaker_rejections(&self) -> u64 {
        self.circuit_breaker_rejections.load(Ordering::Relaxed)
    }
}
