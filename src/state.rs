use tokio::net::UdpSocket;

use crate::client_guard::ClientGuard;
use crate::metrics::Metrics;
use crate::upstream_resolver::UpstreamNameServer;

/// Everything a spawned query task needs, behind a single refcount.
///
/// `tokio::spawn` requires a `'static` future, so each task needs an owned handle to the
/// shared state — that is the one place `Arc` genuinely earns its keep here. Bundling the
/// components means one `Arc::clone` per packet instead of one per component, and
/// everything inside the task is then reached by plain `&`.
pub struct AppState {
    pub socket: UdpSocket,
    pub resolver: UpstreamNameServer,
    pub metrics: Metrics,
    /// `None` when client protection is disabled, so the hot path skips the checks.
    pub client_guard: Option<ClientGuard>,
}
