use std::fs::File;
use std::io::{Read, Result};
use std::net::IpAddr;
use std::path;

use log::debug;
use serde::Deserialize;
use toml;

struct Defaults {}

impl Defaults {
    fn passive() -> bool {
        false
    }

    fn hold_timer() -> u16 {
        180
    }
}

#[derive(Debug, Deserialize)]
pub struct PeerConfig {
    pub remote_ip: IpAddr,
    pub remote_as: u32,
    pub local_as: Option<u32>,
    pub router_id: Option<IpAddr>,

    // Only listen to incoming TCP sessions for passive peers
    // And don't attempt outbound TCP connections
    #[serde(default = "Defaults::passive")]
    pub passive: bool,

    // Timer to keep peers active
    // Will send keepalives every 1/3rd of this value
    #[serde(default = "Defaults::hold_timer")]
    pub hold_timer: u16,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub router_id: IpAddr,
    pub default_as: u32,
    pub output_dir: path::PathBuf,
    pub peers: Vec<PeerConfig>,
}

impl ServerConfig {
    pub fn from_file(path: &str) -> Result<ServerConfig> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config: ServerConfig = toml::from_str(&contents).unwrap();
        debug!("Using config: {:?}", config);
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_parse_config() {
        let config = ServerConfig::from_file("./examples/config.toml").unwrap();
        assert_eq!(config.router_id, IpAddr::from(Ipv4Addr::new(1, 1, 1, 1)));
        assert_eq!(config.default_as, 65000);
        assert_eq!(config.peers.len(), 3);
        let peer = config
            .peers
            .iter()
            .find(|p| p.remote_ip == IpAddr::from(Ipv4Addr::new(127, 0, 0, 2)))
            .unwrap();
        assert_eq!(peer.local_as, Some(65000));
        assert_eq!(peer.hold_timer, 180);
        assert!(!peer.passive);
    }
}
