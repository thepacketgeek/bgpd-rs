use std::net::IpAddr;

use hyper::StatusCode;
use log::error;
use serde::Serialize;

use crate::db::DB;
use crate::utils::format_time_as_elapsed;

use super::Responder;

#[derive(Serialize)]
pub struct PeerSummary {
    peer: IpAddr,
    router_id: Option<IpAddr>,
    asn: u32,
    msg_received: Option<u64>,
    msg_sent: Option<u64>,
    connect_time: Option<i64>,
    uptime: Option<String>,
    state: String,
    prefixes_received: Option<u64>,
}

#[derive(Serialize)]
pub struct PeerSummaries(Vec<PeerSummary>);

impl Responder for PeerSummaries {
    type Item = PeerSummaries;

    fn respond() -> Result<Self::Item, StatusCode> {
        let peers = DB::new().and_then(|db| db.get_all_peers()).map_err(|err| {
            error!("Error fetching all peers: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let output: Vec<PeerSummary> = peers
            .iter()
            .map(|peer| PeerSummary {
                peer: peer.neighbor,
                router_id: peer.router_id,
                asn: peer.asn,
                msg_received: peer.msg_received,
                msg_sent: peer.msg_sent,
                connect_time: peer.connect_time.map(|time| time.timestamp()),
                uptime: peer.connect_time.map(format_time_as_elapsed),
                state: peer.state.to_string(),
                prefixes_received: peer.prefixes_received,
            })
            .collect();
        Ok(PeerSummaries(output))
    }
}
