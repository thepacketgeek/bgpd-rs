use std::fs::File;
use std::net::IpAddr;
use std::io::{Read, Result};

use log::debug;
use serde_derive::Deserialize;
use toml;

#[derive(Debug, Deserialize)]
pub struct PeerConfig {
    pub remote_ip: IpAddr,
    pub remote_as: u32,
    pub local_as: Option<u32>,
    pub router_id: Option<IpAddr>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub router_id: IpAddr,
    pub default_as: u32,
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