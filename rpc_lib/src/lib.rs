#![allow(unused_variables)]
use std::net::IpAddr;
use ipnetwork::IpNetwork;

use serde::{self, Deserialize, Serialize};

jsonrpsee::rpc_api! {
    pub Api {
        fn show_peers() -> Vec<PeerSummary>;
        fn show_peer_detail() -> Vec<PeerDetail>;
        fn show_routes_learned(from_peer: Option<IpNetwork>) -> Vec<LearnedRoute>;
        fn show_routes_advertised(to_peer: Option<IpNetwork>) -> Vec<LearnedRoute>;
        fn advertise_route(route: RouteSpec) -> Result<LearnedRoute, String>;
        fn advertise_flow(flow: FlowSpec) -> Result<LearnedRoute, String>;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeerSummary {
    pub peer: String,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpecAttributes {
    pub origin: Option<String>,
    #[serde(default = "Vec::new")]
    pub as_path: Vec<String>,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    #[serde(default = "Vec::new")]
    pub communities: Vec<String>,
    // TODO: Accept some sort of Policy Object
    //       So that this can be targeted at peer(s)
}

impl std::default::Default for SpecAttributes {
    fn default() -> Self {
        Self {
            origin: None,
            as_path: vec![],
            local_pref: None,
            multi_exit_disc: None,
            communities: vec![],
        }
    }
}

/// API Input for Route to advertise to peers
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RouteSpec {
    /// Prefix to advertise (E.g. "100.1.0.0/16" or "2620:100:ab::/64")
    pub prefix: IpNetwork,
    /// Next-hop to reach this prefix
    pub next_hop: IpAddr,
    #[serde(default = "SpecAttributes::default")]
    pub attributes: SpecAttributes,
}

impl RouteSpec {
    pub fn new(prefix: IpNetwork, next_hop: IpAddr) -> Self {
        Self {
            prefix,
            next_hop,
            attributes: SpecAttributes::default(),
        }
    }
}

/// API Input for Route to advertise to peers
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowSpec {
    /// Primary address family (ipv4=1, ipv6=2)
    pub afi: u16,
    /// Flowspec action (redirect, traffic-rate, traffic-action, ...)
    pub action: String,
    /// Match rules (Src/Dst prefix, Src/Dst Port, TcpFlags, ...)
    pub matches: Vec<String>,
    #[serde(default = "SpecAttributes::default")]
    pub attributes: SpecAttributes,
}

impl FlowSpec {
    pub fn new(afi: u16, action: String, matches: Vec<String>) -> Self {
        Self {
            afi,
            action,
            matches,
            attributes: SpecAttributes::default(),
        }
    }
}
