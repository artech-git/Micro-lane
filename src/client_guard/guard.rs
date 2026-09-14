use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;

use super::entry::ClientEntry;
use super::time::now_ms;

pub enum RateDecision {
    Allowed(Arc<ClientEntry>),
    RateLimited,
}

pub struct ClientGuard {
    entries: DashMap<IpAddr, Arc<ClientEntry>>,
    capacity: f64,
    refill_per_sec: f64,
    idle_ttl: Duration,
}

impl ClientGuard {
    pub fn new(capacity: f64, refill_per_sec: f64, idle_ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            capacity,
            refill_per_sec,
            idle_ttl,
        }
    }

    pub fn check_rate(&self, ip: IpAddr) -> RateDecision {
        let entry = self
            .entries
            .entry(ip)
            .or_insert_with(|| Arc::new(ClientEntry::new(self.capacity, self.refill_per_sec)))
            .clone();

        entry.touch();

        if entry.try_consume() {
            RateDecision::Allowed(entry)
        } else {
            RateDecision::RateLimited
        }
    }

    /// Evicts entries that have been idle for longer than `idle_ttl`, so per-client state
    /// doesn't grow unbounded under a flood of distinct (possibly spoofed) source addresses.
    pub fn sweep(&self) {
        let cutoff = now_ms() - (self.idle_ttl.as_millis() as i64);
        self.entries.retain(|_, entry| !entry.is_stale(cutoff));
    }
}
