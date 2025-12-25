use clap::Parser;
use tokio::net::UdpSocket;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use std::sync::Arc;

use error::BackendResult;
use packet::handle_query;

mod packet;
mod record;
mod bytes; 
mod error;
mod header;
mod query;
mod question;
mod config; 
mod util; 


use tracing::error_span as err; 
use tracing::trace_span as trace;
use tracing::info_span as info;
use tracing::debug_span as debug;

use crate::util::shutdown_signal;


/* 
    Problems to solve: 

        2. handling signal_c for graceful shutdown


        3. upstream stream lookup from another dns server
            (FIXED) 3.1 random port binding for upstream queries
            3.2 Solution NO.2 -> using single socket for handling the upstream queries in Rows
        
        DONE: 
        1. collecting the config from command line
        4. logging using tracing crate


        TODO: 
        // use single upstream port for upstream queries and not ephemeral ports
*/

#[tokio::main]
async fn  main() -> BackendResult<()> {
    
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
        },
    };

    let layers = util::setup_log_target_layer(config_data.log_path);
    // Initialize the subscriber with the layers
    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    
    tracing_subscriber::registry()
        .with(layers)
        .with(stdout_layer)
        .init();

    tracing::info!(target: "connection_debug", name = "decode_packet");

    let inner_socket = UdpSocket::bind((config_data.bind_ip, config_data.port)).await?;
    let shared_socket = Arc::new(inner_socket);

    // buffer for receiving data, and transferring to the handler
    let mut temp_buffer = [0u8; 2048];

    // tokio task handler for tracking the tasks spawned for given connections
    let mut task_handler = tokio_util::task::TaskTracker::new();

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
                
                task_handler.spawn(async move { 

                    match handle_query(&shared_socket_internal, addr, data).await {
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

    // finish the existing tokio task handles
    task_handler.wait().await;

    Ok(())
}