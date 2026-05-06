use std::net::Ipv4Addr;

use tokio::net::UdpSocket;
use tracing::trace;

use crate::{bytes::BytePacketBuffer, error::BackendResult, packet::DnsPacket, query::QueryType, question::DnsQuestion};

#[derive(Debug)]
pub struct UpstreamNameServer {
    // Add fields as necessary
    resolvers: Vec<std::net::SocketAddr>,
}

impl UpstreamNameServer {
    
    pub fn new(resolvers: Vec<std::net::SocketAddr>) -> Self {
        UpstreamNameServer { resolvers }
    }

    pub fn init(lookup_servers: impl AsRef<[std::net::SocketAddr]>) -> Self {
        // Initialize with default values or configurations
        let servers = lookup_servers.as_ref();
        if servers.is_empty() {
            // Return a default server if no servers are provided
            UpstreamNameServer {
                resolvers: vec![std::net::SocketAddr::from(([8, 8, 8, 8], 53))]
            }
        } else {
            // Return the first server in the list
            let addrs = servers.to_vec();
            UpstreamNameServer { resolvers: addrs }
        }
    }

    pub async fn resolve(&self, query: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Implement the logic to resolve the DNS query using upstream servers
        // For demonstration, we'll just return an empty response
        Ok(vec![])
    }

    pub async fn lookup(qname: &str, qtype: QueryType, server: (Ipv4Addr, u16)) -> BackendResult<DnsPacket> {
        // allow the port to bind to any ephemeral port
        let socket = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await?;

        trace!(
            "connection_debug",
            "Performing lookup for {:?} {:?} with server {:?}",
            qtype,
            qname,
            server.0
        );

        let mut packet = DnsPacket::new();

        packet.header.id = 6666;
        packet.header.questions = 1;
        packet.header.recursion_desired = true;
        packet
            .questions
            .push(DnsQuestion::new(qname.to_string(), qtype));

        let mut req_buffer = BytePacketBuffer::new();
        packet.write(&mut req_buffer)?;
        socket.send_to(&req_buffer.buf[0..req_buffer.pos], server).await?;

        let mut res_buffer = BytePacketBuffer::new();
        socket.recv_from(&mut res_buffer.buf).await?;
        
        DnsPacket::from_buffer(&mut res_buffer)
    }

    
}