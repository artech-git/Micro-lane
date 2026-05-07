use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use failsafe::futures::CircuitBreaker as _;
use tokio::net::UdpSocket;
use tracing::trace;

use crate::{
    bytes::BytePacketBuffer,
    error::BackendResult,
    packet::DnsPacket,
    query::QueryType,
    question::DnsQuestion,
};

// Wraps around at u16::MAX; uniqueness is best-effort for in-flight queries.
static QUERY_ID: AtomicU16 = AtomicU16::new(1);

// Circuit breaker type produced by Config::new().build().
// Default policy: SuccessRateOverTimeWindow OR ConsecutiveFailures, both with EqualJittered backoff.
type DnsCircuitBreaker = failsafe::StateMachine<
    failsafe::failure_policy::OrElse<
        failsafe::failure_policy::SuccessRateOverTimeWindow<failsafe::backoff::EqualJittered>,
        failsafe::failure_policy::ConsecutiveFailures<failsafe::backoff::EqualJittered>,
    >,
    (),
>;

pub struct UpstreamNameServer {
    pub resolvers: Vec<std::net::SocketAddr>,
    pub recursive_ns_seed: Ipv4Addr,
    pub upstream_dns_port: u16,
    timeout: Duration,
    // Shared across clones so all query tasks see the same breaker state.
    circuit_breaker: Arc<DnsCircuitBreaker>,
}

impl UpstreamNameServer {
    pub fn new(
        resolvers: Vec<std::net::SocketAddr>,
        timeout: Duration,
        recursive_ns_seed: Ipv4Addr,
        upstream_dns_port: u16,
    ) -> Self {
        UpstreamNameServer {
            resolvers,
            timeout,
            recursive_ns_seed,
            upstream_dns_port,
            circuit_breaker: Arc::new(failsafe::Config::new().build()),
        }
    }

    pub fn init(
        lookup_servers: impl AsRef<[std::net::SocketAddr]>,
        timeout: Duration,
        recursive_ns_seed: Ipv4Addr,
        upstream_dns_port: u16,
    ) -> Self {
        let servers = lookup_servers.as_ref();
        let servers = if servers.is_empty() {
            vec![std::net::SocketAddr::from(([8, 8, 8, 8], 53))]
        } else {
            servers.to_vec()
        };
        Self::new(servers, timeout, recursive_ns_seed, upstream_dns_port)
    }

    pub async fn resolve(&self, _query: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(vec![])
    }

    /// Send a DNS query to `server`, guarded by the shared circuit breaker.
    ///
    /// Returns `Err` immediately (without hitting the network) when the breaker is open,
    /// giving callers a fast SERVFAIL instead of waiting on a dead upstream.
    pub async fn lookup(
        &self,
        qname: &str,
        qtype: QueryType,
        server: (Ipv4Addr, u16),
    ) -> BackendResult<DnsPacket> {
        // Wrap raw_lookup in a timeout so a silent/slow upstream doesn't stall the task
        // indefinitely. The timeout resolves to Err, which the circuit breaker counts as a
        // failure — repeated timeouts will open the breaker.
        let timeout = self.timeout;
        let timed = async move {
            match tokio::time::timeout(timeout, raw_lookup(qname, qtype, server)).await {
                Ok(result) => result,
                Err(_elapsed) => {
                    tracing::warn!(
                        target: "connection_err",
                        "Upstream lookup timed out after {}s: {} via {}",
                        timeout.as_secs(),
                        qname,
                        server.0
                    );
                    Err(format!(
                        "upstream lookup timed out after {}s",
                        timeout.as_secs()
                    ).into())
                }
            }
        };

        match self.circuit_breaker.call(timed).await {
            Ok(packet) => Ok(packet),
            Err(failsafe::Error::Inner(e)) => Err(e),
            Err(failsafe::Error::Rejected) => {
                tracing::warn!(
                    target: "connection_err",
                    "Circuit breaker OPEN — fast-failing lookup for {} via {}",
                    qname,
                    server.0
                );
                Err("circuit breaker open: upstream DNS server temporarily unavailable".into())
            }
        }
    }
}

async fn raw_lookup(
    qname: &str,
    qtype: QueryType,
    server: (Ipv4Addr, u16),
) -> BackendResult<DnsPacket> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).await?;

    trace!(
        target: "connection_debug",
        "Performing lookup for {:?} {} with server {}",
        qtype,
        qname,
        server.0
    );

    let mut packet = DnsPacket::new();
    packet.header.id = QUERY_ID.fetch_add(1, Ordering::Relaxed);
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet.questions.push(DnsQuestion::new(qname.to_string(), qtype));

    let mut req_buffer = BytePacketBuffer::new();
    packet.write(&mut req_buffer)?;
    socket.send_to(&req_buffer.buf[0..req_buffer.pos], server).await?;

    let mut res_buffer = BytePacketBuffer::new();
    socket.recv_from(&mut res_buffer.buf).await?;

    DnsPacket::from_buffer(&mut res_buffer)
}
