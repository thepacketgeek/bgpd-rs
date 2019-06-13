use std::convert::From;
use std::net::IpAddr;

use serde::Serialize;
use tower_web::Response;

use crate::models::Peer;
use crate::session::Session;
use crate::utils::format_time_as_elapsed;

#[derive(Debug, Serialize)]
pub struct PeerSummary {
    pub peer: IpAddr,
    pub router_id: Option<IpAddr>,
    pub asn: u32,
    pub msg_received: Option<u64>,
    pub msg_sent: Option<u64>,
    pub connect_time: Option<i64>,
    pub uptime: Option<String>,
    pub state: String,
    pub prefixes_received: Option<u64>,
}

#[derive(Debug, Response)]
pub struct PeerSummaries(pub Vec<PeerSummary>);

impl From<&Peer> for PeerSummary {
    fn from(peer: &Peer) -> PeerSummary {
        PeerSummary {
            peer: peer.addr,
            router_id: peer.remote_id.router_id,
            asn: peer.remote_id.asn,
            msg_received: None,
            msg_sent: None,
            connect_time: None,
            uptime: None,
            state: peer.get_state().to_string(),
            prefixes_received: None,
        }
    }
}

impl From<&Session> for PeerSummary {
    fn from(session: &Session) -> PeerSummary {
        PeerSummary {
            peer: session.peer.addr,
            router_id: session.peer.remote_id.router_id,
            asn: session.peer.remote_id.asn,
            msg_received: Some(session.counts.received()),
            msg_sent: Some(session.counts.sent()),
            connect_time: Some(session.connect_time.timestamp()),
            uptime: Some(format_time_as_elapsed(session.connect_time)),
            state: session.peer.get_state().to_string(),
            prefixes_received: None,
        }
    }
}
