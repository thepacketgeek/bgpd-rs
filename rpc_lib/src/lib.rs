#[allow(unused_variables)]
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

jsonrpsee::rpc_api! {
    pub Api {
        fn show_peers() -> Vec<PeerSummary>;
        fn show_peer_detail() -> Vec<PeerDetail>;
        fn show_routes_learned(from_peer: Option<IpAddr>) -> Vec<LearnedRoute>;
        fn show_routes_advertised(to_peer: Option<IpAddr>) -> Vec<LearnedRoute>;
        fn advertise_route(route: AdvertiseRoute) -> Result<LearnedRoute, String>;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeerSummary {
    pub peer: IpAddr,
    pub enabled: bool,
    pub router_id: Option<IpAddr>,
    pub remote_asn: u32,
    pub local_asn: u32,
    pub msg_received: Option<u64>,
    pub msg_sent: Option<u64>,
    pub connect_time: Option<i64>,
    pub uptime: Option<String>,
    pub state: String,
    pub prefixes_received: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeerDetail {
    pub summary: PeerSummary,
    pub hold_timer: u16,
    pub hold_timer_interval: u16,
    // Either the negotiated (if active) or configured hold_time
    pub hold_time: Option<String>,
    pub last_received: Option<String>,
    pub last_sent: Option<String>,
    // TCP Stream info Local, Remote
    pub tcp_connection: Option<(String, String)>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LearnedRoute {
    pub source: String,
    pub afi: String,
    pub safi: String,
    pub received_at: i64,
    pub age: String,
    pub prefix: String,
    pub next_hop: Option<IpAddr>,
    pub origin: String,
    pub as_path: String,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    pub communities: Vec<String>,
}

/// API Input for Route to advertise out to peers
#[derive(Debug, Deserialize, Serialize)]
pub struct AdvertiseRoute {
    pub prefix: String,
    pub next_hop: String,
    pub origin: Option<String>,
    pub as_path: Vec<String>,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    pub communities: Vec<String>,
    // TODO: Accept some sort of Policy Object
    //       So that this can be targeted at peer(s)
}

impl AdvertiseRoute {
    pub fn new(prefix: String, next_hop: IpAddr) -> Self {
        Self {
            prefix: prefix,
            next_hop: next_hop.to_string(),
            origin: None,
            as_path: vec![],
            local_pref: None,
            multi_exit_disc: None,
            communities: vec![],
        }
    }
}
