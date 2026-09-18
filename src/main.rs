use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use upstream_resolver::UpstreamNameServer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use error::BackendResult;
use packet::handle_query;

mod client_guard;
mod config;
mod error;
mod http;
mod metrics;
mod packet;
mod state;
mod util;
mod upstream_resolver;

use client_guard::{ClientGuard, RateDecision};
use failsafe::futures::CircuitBreaker as _;

use tracing::debug_span as debug;
use tracing::error_span as err;

use crate::http::serve_metrics;
use crate::metrics::{Counter, Metrics};
use crate::state::AppState;
use crate::util::shutdown_signal;

#[tokio::main]
async fn main() -> BackendResult<()> {
    let config_data = match config::Config::try_parse() {
        Err(err) => {
            tracing::error!("Failed to parse config: {err}");
            println!("Failed to parse config: {err}");
            // return Err(format!("Failed to parse config: {:#?}", err.to_string()).into());
            return Err("error".into());
        }
        Ok(cfg) => {
            tracing::info!(target: "connection_troubleshoot", name = "decode_packet", "Config parsed: {:#?}", cfg);
            cfg
        }
    };

    let file_layers = if config_data.file_logging {
        std::fs::create_dir_all(&config_data.log_path)?;
        Some(util::setup_log_target_layer(config_data.log_path))
    } else {
        None
    };

    let stdout_layer = config_data.stdout_logging
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stdout));

    tracing_subscriber::registry()
        .with(file_layers)
        .with(stdout_layer)
        .init();

    tracing::info!(target: "connection_debug", name = "decode_packet");

    let socket = UdpSocket::bind((config_data.bind_ip, config_data.port)).await?;

    // Single circuit-breaker-backed resolver shared across all query tasks.
    let resolver = UpstreamNameServer::init(
        &config_data.upstream_servers,
        Duration::from_secs(config_data.upstream_timeout_secs),
        config_data.recursive_ns_seed,
        config_data.upstream_dns_port,
    );

    // Per-client rate limiter + circuit breaker for downstream queries. `None` when disabled,
    // so the receive loop and query tasks skip the checks entirely.
    let client_guard = config_data.client_protection_enabled.then(|| {
        ClientGuard::new(
            config_data.client_rate_capacity,
            config_data.client_rate_refill_per_sec,
            Duration::from_secs(config_data.client_idle_ttl_secs),
        )
    });

    // One refcount for the whole shared surface: the spawn boundary needs `'static`, and
    // this way each packet pays a single `Arc::clone` rather than one per component.
    let state = Arc::new(AppState {
        socket,
        resolver,
        metrics: Metrics::new(),
        client_guard,
    });

    // buffer for receiving data, and transferring to the handler
    let mut temp_buffer = vec![0u8; config_data.recv_buffer_size];

    // tokio task handler for tracking the tasks spawned for given connections
    let task_handler = tokio_util::task::TaskTracker::new();

    let shutdown_handle = shutdown_signal().await?;

    if config_data.metrics_enabled {
        tokio::spawn(serve_metrics(
            Arc::clone(&state),
            config_data.metrics_port,
            Arc::clone(&shutdown_handle),
        ));
    }

    if state.client_guard.is_some() {
        let sweep_state = Arc::clone(&state);
        let sweep_shutdown = Arc::clone(&shutdown_handle);
        let sweep_interval = Duration::from_secs(config_data.client_sweep_interval_secs);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(sweep_interval);
            let notified = sweep_shutdown.notified();
            tokio::pin!(notified);
            'guard_logic: loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Some(guard) = &sweep_state.client_guard {
                            guard.sweep();
                        }
                    }
                    _ = &mut notified => {
                        break 'guard_logic;
                    }
                }
            }
        });
    }

    let notified_owned = shutdown_handle.notified();
    tokio::pin!(notified_owned);

    'connection: loop {
        tokio::select! {

            _ = notified_owned.as_mut() => {
                tracing::info!(target: "connection_debug", "Shutdown signal received Loops...");
                break 'connection;
            }

            value = state.socket.recv_from(&mut temp_buffer) => {

                let (len , addr) = match value {
                    Err(e) => {
                        tracing::error!(target: "connection_err", "Failed to receive data: {}", e);
                        continue 'connection;
                    }
                    Ok((l, a)) => (l, a),
                };

                let client_entry = if let Some(guard) = &state.client_guard {
                    match guard.check_rate(addr.ip()) {
                        RateDecision::Allowed(entry) => Some(entry),
                        RateDecision::RateLimited => {
                            state.metrics.incr(Counter::ClientRateLimited);
                            tracing::warn!(target: "connection_err", "Rate limit exceeded for {addr}, dropping query");
                            continue 'connection;
                        }
                    }
                } else {
                    None
                };

                let data = temp_buffer[..len].to_vec();
                let task_state = Arc::clone(&state);

                task_handler.spawn(async move {

                    let query_future = handle_query(&task_state, addr, data);

                    let result = if let Some(entry) = &client_entry {
                        entry.breaker.call(query_future).await
                    } else {
                        query_future.await.map_err(failsafe::Error::Inner)
                    };

                    match result {
                        Ok(_) => {
                            debug!("connection_debug", "Query handled successfully for {addr}");
                        }
                        Err(failsafe::Error::Inner(e)) => {
                            let err_msg = format!("An error occurred: {:?}", e);
                            err!("connection_err", err_msg);
                        }
                        Err(failsafe::Error::Rejected) => {
                            task_state.metrics.incr(Counter::ClientCircuitRejections);
                            tracing::warn!(target: "connection_err", "Circuit breaker OPEN for client {addr} — dropping query");
                        }
                    }
                });
            }
        }
    }

    // close() signals the tracker that no new tasks will be spawned, which is
    // required for wait() to ever resolve — without it wait() blocks forever.
    task_handler.close();
    task_handler.wait().await;

    Ok(())
}
