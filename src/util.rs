#![allow(dead_code)]
use std::sync::Arc;

use tokio::sync::Notify;
use tracing_subscriber::Layer;

pub mod log_target_data {
    pub const CONNECTION_DEBUG: &str = "connection_debug";
    pub const CONNECTION_ERR: &str = "connection_err";
    pub const CONNECTION_TROUBLESHOOT: &str = "connection_troubleshoot";
    pub const CONN_OPERATIONS: &str = "connection_operations";

    pub const TARGETS: [&str; 4] = [
        CONNECTION_DEBUG,
        CONNECTION_ERR,
        CONNECTION_TROUBLESHOOT,
        CONN_OPERATIONS,
    ];
}

const UPSTREAM_SERVER: [&str; 2] = ["8.8.8.8", "8.8.4.4"];

// only shutdown on SIGTERM or SIGINT
pub async fn shutdown_signal() -> Result<Arc<Notify>, Box<dyn std::error::Error>> {
    let notifier = Arc::new(Notify::new());
    let notifier_clone = notifier.clone();

    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!(target: log_target_data::CONNECTION_TROUBLESHOOT, "Shutdown signal received...");
            }
            Err(e) => {
                tracing::error!(target: log_target_data::CONNECTION_ERR, "Unable to listen for shutdown signal: {}", e);
            }
        }

        //despite outcome notify all the notifiers
        notifier_clone.notify_waiters();
    });

    Ok(notifier)
}

pub fn setup_log_target_layer(
    path: impl Into<std::path::PathBuf>,
) -> Vec<impl tracing_subscriber::Layer<tracing_subscriber::Registry>> {
    let mut layers = vec![];

    let log_path = path.into();
    // read them step by step from the LOG_TARGET_DATA
    for target in log_target_data::TARGETS.iter() {
        let file_writer = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path.join(format!("{}.log", target)))
            .unwrap();

        layers.push(
            tracing_subscriber::fmt::layer()
                // .with_writer(std::io::stdout)
                .with_writer(file_writer)
                .with_file(true)
                .with_target(true)
                .with_filter(tracing_subscriber::filter::filter_fn(move |meta| {
                    meta.target() == *target
                })),
        );
    }

    layers
}
