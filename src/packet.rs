use hickory_proto::op::{Message, MessageType, ResponseCode};
use hickory_proto::rr::{Name, RData, RecordType};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket as TokioUdpSocket;

use crate::error::BackendResult;
use crate::upstream_resolver::UpstreamNameServer;

fn get_random_a(msg: &Message) -> Option<Ipv4Addr> {
    msg.answers().iter().find_map(|r| {
        if let Some(RData::A(addr)) = r.data() {
            Some(addr.0)
        } else {
            None
        }
    })
}

fn get_resolved_ns(msg: &Message, qname: &Name) -> Option<Ipv4Addr> {
    let ns_names: Vec<&Name> = msg
        .name_servers()
        .iter()
        .filter_map(|r| {
            if let Some(RData::NS(ns)) = r.data() {
                if r.name().zone_of(qname) {
                    return Some(&ns.0);
                }
            }
            None
        })
        .collect();

    ns_names.iter().find_map(|&ns_name| {
        msg.additionals().iter().find_map(move |r| {
            if r.name() == ns_name {
                if let Some(RData::A(addr)) = r.data() {
                    return Some(addr.0);
                }
            }
            None
        })
    })
}

fn get_unresolved_ns<'a>(msg: &'a Message, qname: &Name) -> Option<&'a Name> {
    msg.name_servers().iter().find_map(|r| {
        if let Some(RData::NS(ns)) = r.data() {
            if r.name().zone_of(qname) {
                return Some(&ns.0);
            }
        }
        None
    })
}

enum FrameState {
    Active(Ipv4Addr),
    Suspended(Message),
}

struct Frame {
    qname: String,
    qtype: RecordType,
    state: FrameState,
}

async fn recursive_lookup(
    qname: &str,
    qtype: RecordType,
    resolver: &UpstreamNameServer,
) -> BackendResult<Message> {
    let mut stack = vec![Frame {
        qname: qname.to_owned(),
        qtype,
        state: FrameState::Active(resolver.recursive_ns_seed),
    }];

    loop {
        let frame = stack.last_mut().expect("stack is never empty here");
        let ns = match &frame.state {
            FrameState::Active(ip) => *ip,
            FrameState::Suspended(_) => unreachable!("suspended frame must not be on top"),
        };

        tracing::info_span!(
            "connection_debug",
            "attempting lookup of {:?} {} with ns {}",
            frame.qtype,
            frame.qname,
            ns
        );

        let response = resolver
            .lookup(&frame.qname, frame.qtype, (ns, resolver.upstream_dns_port))
            .await?;

        let qname_name = Name::from_str(&frame.qname)?;

        let is_terminal = (!response.answers().is_empty()
            && response.response_code() == ResponseCode::NoError)
            || response.response_code() == ResponseCode::NXDomain;

        if is_terminal {
            stack.pop();
            let mut result = response;
            while let Some(parent) = stack.last_mut() {
                match &parent.state {
                    FrameState::Suspended(_) => {
                        if let Some(ip) = get_random_a(&result) {
                            parent.state = FrameState::Active(ip);
                            break;
                        }
                        let FrameState::Suspended(fallback) = std::mem::replace(
                            &mut parent.state,
                            FrameState::Active(resolver.recursive_ns_seed),
                        ) else {
                            unreachable!()
                        };
                        result = fallback;
                        stack.pop();
                    }
                    FrameState::Active(_) => unreachable!("active frame below suspended"),
                }
            }
            if stack.is_empty() {
                return Ok(result);
            }
            continue;
        }

        if let Some(new_ns) = get_resolved_ns(&response, &qname_name) {
            frame.state = FrameState::Active(new_ns);
            continue;
        }

        if let Some(ns_name) = get_unresolved_ns(&response, &qname_name) {
            let ns_name_str = ns_name.to_string();
            frame.state = FrameState::Suspended(response);
            stack.push(Frame {
                qname: ns_name_str,
                qtype: RecordType::A,
                state: FrameState::Active(resolver.recursive_ns_seed),
            });
            continue;
        }

        // No more leads — treat as terminal.
        stack.pop();
        let mut result = response;
        while let Some(parent) = stack.last_mut() {
            match &parent.state {
                FrameState::Suspended(_) => {
                    if let Some(ip) = get_random_a(&result) {
                        parent.state = FrameState::Active(ip);
                        break;
                    }
                    let FrameState::Suspended(fallback) = std::mem::replace(
                        &mut parent.state,
                        FrameState::Active(resolver.recursive_ns_seed),
                    ) else {
                        unreachable!()
                    };
                    result = fallback;
                    stack.pop();
                }
                FrameState::Active(_) => unreachable!("active frame below suspended"),
            }
        }
        if stack.is_empty() {
            return Ok(result);
        }
    }
}

pub async fn handle_query(
    socket: &TokioUdpSocket,
    addr: SocketAddr,
    data: Vec<u8>,
    resolver: Arc<UpstreamNameServer>,
) -> BackendResult<()> {
    let request = Message::from_vec(&data)?;

    let mut response = Message::new();
    response
        .set_id(request.id())
        .set_message_type(MessageType::Response)
        .set_recursion_desired(true)
        .set_recursion_available(true);

    let _span = tracing::span!(tracing::Level::TRACE, "connection_debug");
    let _ = _span.enter();

    if let Some(query) = request.queries().first().cloned() {
        tracing::debug!("Received query: {:?}", query);

        let qname = query.name().to_string();
        let qtype = query.query_type();

        if let Ok(result) = recursive_lookup(&qname, qtype, &resolver).await {
            response.add_query(query);
            response.set_response_code(result.response_code());

            for rec in result.answers() {
                tracing::info!("Answer: {:?}", rec);
                response.add_answer(rec.clone());
            }
            for rec in result.name_servers() {
                tracing::info!("Authority: {:?}", rec);
                response.add_name_server(rec.clone());
            }
            for rec in result.additionals() {
                tracing::info!("Resource: {:?}", rec);
                response.add_additional(rec.clone());
            }
        } else {
            response.set_response_code(ResponseCode::ServFail);
        }
    } else {
        response.set_response_code(ResponseCode::FormErr);
    }

    tracing::debug!("Sending response: {:?}", response);

    let wire = response.to_vec()?;
    socket.send_to(&wire, addr).await?;

    Ok(())
}