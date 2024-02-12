use std::net::UdpSocket;
use std::net::{Ipv4Addr, Ipv6Addr};

use error::BackendResult;
use packet::handle_query;


mod packet;
mod record;
mod bytes; 
mod error;
mod header;
mod query;
mod question;


fn main() -> BackendResult<()> {
    let socket = UdpSocket::bind(("0.0.0.0", 2053))?;

    loop {
        match handle_query(&socket) {
            Ok(_) => {}
            Err(e) => eprintln!("An error occurred: {}", e),
        }
    }
}