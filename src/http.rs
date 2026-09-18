use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::metrics::Counter;
use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
    circuit_breaker: &'static str,
}

#[derive(Serialize)]
struct QueryStats {
    total: u64,
    ok: u64,
    servfail: u64,
    formerr: u64,
}

#[derive(Serialize)]
struct UpstreamStats {
    timeouts: u64,
    circuit_breaker_rejections: u64,
}

#[derive(Serialize)]
struct DownstreamStats {
    rate_limited: u64,
    circuit_rejections: u64,
}

#[derive(Serialize)]
struct MetricsResponse {
    queries: QueryStats,
    upstream: UpstreamStats,
    downstream: DownstreamStats,
    uptime_secs: u64,
}

async fn health_handler(State(state): State<Arc<AppState>>) -> (StatusCode, Json<HealthResponse>) {
    let circuit_open = state.metrics.is_circuit_open();
    let uptime_secs = state.metrics.uptime_secs();

    if circuit_open {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "degraded",
                uptime_secs,
                circuit_breaker: "open",
            }),
        )
    } else {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "healthy",
                uptime_secs,
                circuit_breaker: "closed",
            }),
        )
    }
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<MetricsResponse> {
    let m = &state.metrics;
    Json(MetricsResponse {
        queries: QueryStats {
            // Derived from the three outcome counters rather than stored separately.
            total: m.queries_total(),
            ok: m.get(Counter::QueriesOk),
            servfail: m.get(Counter::QueriesServfail),
            formerr: m.get(Counter::QueriesFormerr),
        },
        upstream: UpstreamStats {
            timeouts: m.get(Counter::UpstreamTimeouts),
            circuit_breaker_rejections: m.get(Counter::CircuitBreakerRejections),
        },
        downstream: DownstreamStats {
            rate_limited: m.get(Counter::ClientRateLimited),
            circuit_rejections: m.get(Counter::ClientCircuitRejections),
        },
        uptime_secs: m.uptime_secs(),
    })
}

pub async fn serve_metrics(state: Arc<AppState>, port: u16, shutdown: Arc<Notify>) {
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind metrics HTTP listener");

    tracing::info!(
        target: "connection_troubleshoot",
        "Metrics server listening on http://{}",
        addr
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move { shutdown.notified().await })
        .await
        .ok();
}
