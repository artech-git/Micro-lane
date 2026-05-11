use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct Config {
    /// IP address to bind the DNS server to (IPv4 or IPv6)
    #[clap(short, long, default_value = "0.0.0.0")]
    pub bind_ip: IpAddr,

    /// Port to bind the DNS server to
    #[clap(short, long, default_value = "53")]
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

    /// UDP receive buffer size in bytes (minimum 512)
    #[clap(long, default_value = "2048", value_parser = parse_buffer_size)]
    pub recv_buffer_size: usize,

    /// Enable stdout logging
    #[clap(short, long, default_value = "true", action = clap::ArgAction::Set)]
    pub stdout_logging: bool,

    /// Enable file logging — creates separate log files for each log target
    #[clap(short, long, default_value = "false", action = clap::ArgAction::Set)]
    pub file_logging: bool,

    /// Directory to write log files into (only used when --file-logging is true)
    #[clap(short, long, default_value = "./")]
    pub log_path: PathBuf,

    /// Timeout in seconds for upstream DNS lookups
    #[clap(long, default_value = "5")]
    pub upstream_timeout_secs: u64,
}

fn parse_buffer_size(s: &str) -> Result<usize, String> {
    let n: usize = s.parse().map_err(|_| format!("'{}' is not a valid number", s))?;
    if n < 512 {
        return Err(format!("recv-buffer-size must be at least 512 bytes (got {})", n));
    }
    Ok(n)
}
