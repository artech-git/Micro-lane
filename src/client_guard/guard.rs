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
        // Steady-state path: a shard *read* lock, so clients hashing to the same shard are
        // not serialised against each other. `ClientEntry` is all interior mutability, so a
        // shared reference is enough for both `touch` and `try_consume`. The refcount is
        // only touched when the packet is actually admitted — a rejected flood costs no
        // `Arc` clone/drop pair at all.
        if let Some(entry) = self.entries.get(&ip) {
            entry.touch();
            return if entry.try_consume() {
                RateDecision::Allowed(Arc::clone(&entry))
            } else {
                RateDecision::RateLimited
            };
        }

        // Slow path: first packet seen from this address.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn client_ip(last_octet: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, last_octet))
    }

    #[test]
    fn allows_the_first_request_from_a_new_client() {
        let guard = ClientGuard::new(5.0, 1.0, Duration::from_secs(60));

        match guard.check_rate(client_ip(1)) {
            RateDecision::Allowed(_) => {}
            RateDecision::RateLimited => panic!("first request from a fresh client must be allowed"),
        }
    }

    #[test]
    fn rate_limits_once_capacity_is_exhausted() {
        let guard = ClientGuard::new(2.0, 1.0, Duration::from_secs(60));
        let client = client_ip(1);

        assert!(matches!(guard.check_rate(client), RateDecision::Allowed(_)));
        assert!(matches!(guard.check_rate(client), RateDecision::Allowed(_)));
        assert!(matches!(guard.check_rate(client), RateDecision::RateLimited));
    }

    #[test]
    fn different_clients_have_independent_buckets() {
        let guard = ClientGuard::new(1.0, 1.0, Duration::from_secs(60));

        assert!(matches!(guard.check_rate(client_ip(1)), RateDecision::Allowed(_)));
        assert!(matches!(guard.check_rate(client_ip(1)), RateDecision::RateLimited));

        // A different source IP must not be affected by client 1 exhausting its bucket.
        assert!(matches!(guard.check_rate(client_ip(2)), RateDecision::Allowed(_)));
    }

    #[test]
    fn repeated_requests_from_the_same_client_share_one_entry() {
        let guard = ClientGuard::new(5.0, 1.0, Duration::from_secs(60));
        let client = client_ip(1);

        let first = match guard.check_rate(client) {
            RateDecision::Allowed(entry) => entry,
            RateDecision::RateLimited => panic!("unexpected rate limit on first request"),
        };
        let second = match guard.check_rate(client) {
            RateDecision::Allowed(entry) => entry,
            RateDecision::RateLimited => panic!("unexpected rate limit on second request"),
        };

        assert!(
            Arc::ptr_eq(&first, &second),
            "the same client IP must reuse the same entry, not a fresh one per request"
        );
    }

    #[test]
    fn sweep_evicts_idle_entries() {
        let guard = ClientGuard::new(5.0, 1.0, Duration::from_millis(1));
        guard.check_rate(client_ip(1));
        assert_eq!(guard.entries.len(), 1);

        std::thread::sleep(Duration::from_millis(20));
        guard.sweep();

        assert!(guard.entries.is_empty(), "an idle entry should have been evicted");
    }

    #[test]
    fn sweep_keeps_recently_active_entries() {
        let guard = ClientGuard::new(5.0, 1.0, Duration::from_secs(60));
        guard.check_rate(client_ip(1));

        guard.sweep();

        assert_eq!(
            guard.entries.len(),
            1,
            "an entry seen well within the idle TTL must survive a sweep"
        );
    }
}
