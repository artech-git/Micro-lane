use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Config {
    /// IP address to bind the DNS server to
    /// Default is
    /// - IPv4:
    #[clap(short, long, default_value = "0.0.0.0")]
    pub bind_ip: String,

    /// Port to bind the DNS server to
    /// Default is 53
    #[clap(short, long)]
    pub port: u16,

    /// Upstream DNS servers for forwarding queries (format: IP:PORT)
    #[clap(long, num_args = 1.., default_values = ["8.8.8.8:53", "8.8.4.4:53"])]
    pub upstream_servers: Vec<SocketAddr>,

    /// Starting nameserver IP for recursive DNS resolution
    #[clap(long, default_value = "8.8.4.4")]
    pub recursive_ns_seed: Ipv4Addr,

    /// Port used when querying nameservers during recursive resolution
    #[clap(long, default_value = "53")]
    pub upstream_dns_port: u16,

    /// UDP receive buffer size in bytes
    #[clap(long, default_value = "2048")]
    pub recv_buffer_size: usize,

    /// Enable stdout logging
    #[clap(short, long, default_value = "true")]
    pub stdout_logging: bool,

    /// Enable file logging
    /// Creates separate log files for each log target
    #[clap(short, long, default_value = "false")]
    pub file_logging: bool,

    /// file logging location
    #[clap(short, long, default_value =  "./", value_parser = check_log_path)]
    pub log_path: PathBuf,

    /// Timeout in seconds for upstream DNS lookups
    #[clap(long, default_value = "5")]
    pub upstream_timeout_secs: u64,
}

fn check_log_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);

    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|err| {
            err.to_string()
        })?;
    }

    Ok(path.to_path_buf())
}
