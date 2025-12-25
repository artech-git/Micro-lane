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
}

fn check_log_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);

    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|err| {
            // clap::Error::raw(
            //     clap::error::ErrorKind::InvalidValue,
            //     format!("Failed to create log directory: {}", err),
            // )
            err.to_string()
        })?;
    }

    Ok(path.to_path_buf())
}
