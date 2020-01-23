mod file;

pub use file::AdvertiseSource;

use std::collections::HashSet;
use std::io::Result;
use std::net::IpAddr;
use std::sync::Arc;

use bgpd_rpc_lib::{FlowSpec, RouteSpec};
use ipnetwork::IpNetwork;

use crate::rib::Family;

/// Parse a TOML config file and return a ServerConfig
pub fn from_file(path: &str) -> Result<ServerConfig> {
    let spec = file::ServerConfigSpec::from_file(path)?;
    Ok(ServerConfig::from_spec(spec))
}

#[derive(Debug)]
pub struct ServerConfig {
    pub router_id: IpAddr,
    pub default_as: u32,
    pub poll_interval: u16,
    pub peers: Vec<Arc<PeerConfig>>,
}

/// In-Memory Server representation of a Peer config
///   Has missing PeerConfigSpec items defaulted to Server values
#[derive(Debug)]
pub struct PeerConfig {
    pub remote_ip: IpNetwork,
    pub remote_as: u32,
    pub local_as: u32,
    pub local_router_id: IpAddr,
    pub enabled: bool,
    pub passive: bool,
    pub hold_timer: u16,
    pub dest_port: u16,
    pub families: Vec<Family>,
    pub advertise_sources: HashSet<AdvertiseSource>,
    pub static_routes: Vec<RouteSpec>,
    pub static_flows: Vec<FlowSpec>,
}

impl PeerConfig {
    // Is this an eBGP session
    pub fn is_ebgp(&self) -> bool {
        self.remote_as != self.local_as
    }
}

impl ServerConfig {
    fn from_spec(spec: file::ServerConfigSpec) -> Self {
        let peers: Vec<_> = spec
            .peers
            .iter()
            .map(|p| {
                Arc::new(PeerConfig {
                    remote_ip: p.remote_ip,
                    remote_as: p.remote_as,
                    local_as: p.local_as.unwrap_or(spec.default_as),
                    local_router_id: p.local_router_id.unwrap_or(spec.router_id),
                    enabled: p.enabled,
                    passive: p.passive,
                    hold_timer: p.hold_timer,
                    dest_port: p.dest_port,
                    families: p.families.clone(),
                    advertise_sources: p.advertise_sources.clone().into_iter().collect(),
                    static_routes: p.static_routes.clone().into_iter().collect(),
                    static_flows: p.static_flows.clone().into_iter().collect(),
                })
            })
            .collect();

        Self {
            router_id: spec.router_id,
            default_as: spec.default_as,
            poll_interval: spec.poll_interval,
            peers,
        }
    }
}
