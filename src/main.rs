use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use upstream_resolver::UpstreamNameServer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use error::BackendResult;
use packet::handle_query;

mod bytes;
mod config;
mod error;
mod header;
mod packet;
mod query;
mod question;
mod record;
mod util;
mod upstream_resolver;

use tracing::debug_span as debug;
use tracing::error_span as err;

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

    let file_layers = config_data.file_logging
        .then(|| util::setup_log_target_layer(config_data.log_path));

    let stdout_layer = config_data.stdout_logging
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stdout));

    tracing_subscriber::registry()
        .with(file_layers)
        .with(stdout_layer)
        .init();

    tracing::info!(target: "connection_debug", name = "decode_packet");

    let inner_socket = UdpSocket::bind((config_data.bind_ip, config_data.port)).await?;
    let shared_socket = Arc::new(inner_socket);

    // Single circuit-breaker-backed resolver shared across all query tasks.
    let resolver = Arc::new(UpstreamNameServer::init(
        &config_data.upstream_servers,
        Duration::from_secs(config_data.upstream_timeout_secs),
        config_data.recursive_ns_seed,
        config_data.upstream_dns_port,
    ));

    // buffer for receiving data, and transferring to the handler
    let mut temp_buffer = vec![0u8; config_data.recv_buffer_size];

    // tokio task handler for tracking the tasks spawned for given connections
    let task_handler = tokio_util::task::TaskTracker::new();

    let shutdown_handle = shutdown_signal().await?;
    let notified_owned = shutdown_handle.notified();
    tokio::pin!(notified_owned);

    'connection: loop {
        tokio::select! {

            _ = notified_owned.as_mut() => {
                tracing::info!(target: "connection_debug", "Shutdown signal received Loops...");
                break 'connection;
            }

            value = shared_socket.recv_from(&mut temp_buffer) => {

                let (len , addr) = match value {
                    Err(e) => {
                        tracing::error!(target: "connection_err", "Failed to receive data: {}", e);
                        continue 'connection;
                    }
                    Ok((l, a)) => (l, a),
                };

                let data = temp_buffer[..len].to_vec();
                let shared_socket_internal = shared_socket.clone();
                let resolver_clone = Arc::clone(&resolver);

                task_handler.spawn(async move {

                    match handle_query(&shared_socket_internal, addr, data, resolver_clone).await {
                        Ok(_) => {
                            debug!("connection_debug", "Query handled successfully for {addr}");
                        }
                        Err(e) => {
                            let err_msg = format!("An error occurred: {:?}", e);
                            err!("connection_err", err_msg);
                        },
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
