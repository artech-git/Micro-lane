use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::metrics::Metrics;

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
struct MetricsResponse {
    queries: QueryStats,
    upstream: UpstreamStats,
    uptime_secs: u64,
}

async fn health_handler(
    State(metrics): State<Arc<Metrics>>,
) -> (StatusCode, Json<HealthResponse>) {
    let circuit_open = metrics.is_circuit_open();
    let uptime_secs = metrics.uptime_secs();

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

async fn metrics_handler(State(metrics): State<Arc<Metrics>>) -> Json<MetricsResponse> {
    Json(MetricsResponse {
        queries: QueryStats {
            total: metrics.queries_total(),
            ok: metrics.queries_ok(),
            servfail: metrics.queries_servfail(),
            formerr: metrics.queries_formerr(),
        },
        upstream: UpstreamStats {
            timeouts: metrics.upstream_timeouts(),
            circuit_breaker_rejections: metrics.circuit_breaker_rejections(),
        },
        uptime_secs: metrics.uptime_secs(),
    })
}

pub async fn serve_metrics(metrics: Arc<Metrics>, port: u16, shutdown: Arc<Notify>) {
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(metrics);

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
