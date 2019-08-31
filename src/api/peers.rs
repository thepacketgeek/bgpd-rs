use std::net::IpAddr;

use serde::Serialize;
use tower_web::Response;

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
