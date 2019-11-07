use std::convert::TryFrom;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::net::IpAddr;

use bgp_rs::{ASPath, NLRIEncoding, Origin, PathAttribute, Segment, AFI, SAFI};
use serde::{self, Deserialize, Deserializer, Serialize, Serializer};
use toml;

use crate::rib::{Community, CommunityList, Family};
use crate::utils::{asn_from_dotted, prefix_from_string};

struct Defaults {}

impl Defaults {
    fn enabled() -> bool {
        true
    }

    fn passive() -> bool {
        false
    }
    fn poll_interval() -> u16 {
        30
    }

    fn hold_timer() -> u16 {
        180
    }

    fn dest_port() -> u16 {
        179
    }

    fn families() -> Vec<Family> {
        vec![
            Family::new(AFI::IPV4, SAFI::Unicast),
            Family::new(AFI::IPV4, SAFI::Flowspec),
            Family::new(AFI::IPV6, SAFI::Unicast),
            Family::new(AFI::IPV6, SAFI::Flowspec),
        ]
    }
    fn advertise_sources() -> Vec<AdvertiseSource> {
        vec![AdvertiseSource::Api, AdvertiseSource::Config]
    }
}

/// Config (toml) representation of a Peer Config
#[derive(Clone, Debug, Deserialize)]
pub(super) struct PeerConfigSpec {
    // Peer connection details
    pub(super) remote_ip: IpAddr,
    pub(super) remote_as: u32,
    // Local connection details
    pub(super) local_as: Option<u32>,
    pub(super) local_router_id: Option<IpAddr>, // Will defer to server config if not provided

    // Peer is configured and allowed to connect
    #[serde(default = "Defaults::enabled")]
    pub(super) enabled: bool,

    // Only listen to incoming TCP sessions for passive peers
    // And don't attempt outbound TCP connections
    #[serde(default = "Defaults::passive")]
    pub(super) passive: bool,

    // Timer to keep peers active
    // Will send keepalives every 1/3rd of this value
    #[serde(default = "Defaults::hold_timer")]
    pub(super) hold_timer: u16,

    // Destination port for BGP session
    // Used when initiating connection to peer
    #[serde(default = "Defaults::dest_port")]
    pub(super) dest_port: u16,

    // AFI/SAFI Families to Rx/TX for this peer
    #[serde(default = "Defaults::families")]
    pub(super) families: Vec<Family>,
    // Routes from which source(s) should we advertise to this peer?
    #[serde(default = "Defaults::advertise_sources")]
    pub(super) advertise_sources: Vec<AdvertiseSource>,
    // Static routes to advertise to peer (if enabled in advertise_sources)
    #[serde(default = "Vec::new")]
    pub(super) static_routes: Vec<RouteSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ServerConfigSpec {
    // Global Router-ID (can be overriden per-peer in peer config)
    pub(super) router_id: IpAddr,
    // Global ASN (can be overriden per-peer in peer config)
    pub(super) default_as: u32,
    // Inverval to poll idle peers (outbound connection)
    #[serde(default = "Defaults::poll_interval")]
    pub(super) poll_interval: u16,
    #[serde(default = "Vec::new")]
    pub(super) peers: Vec<PeerConfigSpec>,
}

impl ServerConfigSpec {
    pub(super) fn from_file(path: &str) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config: ServerConfigSpec = toml::from_str(&contents).unwrap();
        Ok(config)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RouteSpec {
    pub prefix: String,
    pub next_hop: IpAddr,
    pub origin: Option<String>,
    pub local_pref: Option<u32>,
    pub med: Option<u32>,
    #[serde(default = "Vec::new")]
    pub as_path: Vec<String>,
    #[serde(default = "Vec::new")]
    pub communities: Vec<String>,
}

impl RouteSpec {
    pub fn parse(&self) -> Result<(Family, Vec<PathAttribute>, NLRIEncoding), io::Error> {
        let prefix = prefix_from_string(&self.prefix)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        let mut attributes = vec![
            PathAttribute::NEXT_HOP(self.next_hop),
            PathAttribute::ORIGIN(
                self.origin
                    .as_ref()
                    .map(|o| match o.to_lowercase().as_str() {
                        "igp" => Origin::IGP,
                        "egp" => Origin::EGP,
                        _ => Origin::INCOMPLETE,
                    })
                    .unwrap_or(Origin::INCOMPLETE),
            ),
        ];
        if let Some(local_pref) = self.local_pref {
            attributes.push(PathAttribute::LOCAL_PREF(local_pref));
        }
        if let Some(med) = self.med {
            attributes.push(PathAttribute::MULTI_EXIT_DISC(med));
        }

        let as_path = {
            let mut asns: Vec<u32> = Vec::with_capacity(self.as_path.len());
            for asn in &self.as_path {
                asns.push(asn_from_dotted(asn).map_err(|err| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Error parsing ASN: {}", err.reason),
                    )
                })?);
            }
            ASPath {
                segments: vec![Segment::AS_SEQUENCE(asns)],
            }
        };
        attributes.push(PathAttribute::AS_PATH(as_path));
        let communities = {
            let mut comms: Vec<Community> = Vec::with_capacity(self.communities.len());
            for comm in &self.communities {
                comms.push(Community::try_from(comm.as_str())?);
            }
            CommunityList(comms)
        };
        let standard_communities = communities.standard();
        if !standard_communities.is_empty() {
            attributes.push(PathAttribute::COMMUNITY(standard_communities));
        }
        let extd_communities = communities.extended();
        if !extd_communities.is_empty() {
            attributes.push(PathAttribute::EXTENDED_COMMUNITIES(extd_communities));
        }

        Ok((
            Family::new(prefix.protocol, SAFI::Unicast),
            attributes,
            NLRIEncoding::IP(prefix),
        ))
    }
}

// Temporary way to select which routes to advertise to a peer
// TODO: Replace this with import/export Policies
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AdvertiseSource {
    Api,
    Peer,
    Config,
}

impl fmt::Display for AdvertiseSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use AdvertiseSource::*;
        let display = match self {
            Api => "API",
            Config => "Config",
            Peer => "Peer",
        };
        write!(f, "{}", display)
    }
}

impl Serialize for AdvertiseSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AdvertiseSource {
    fn deserialize<D>(deserializer: D) -> Result<AdvertiseSource, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "api" => Ok(AdvertiseSource::Api),
            "config" => Ok(AdvertiseSource::Config),
            "peer" => Ok(AdvertiseSource::Peer),
            _ => Err(serde::de::Error::custom(format!(
                "Unsupported AdvertiseSource: '{}'",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_parse_config() {
        let config = ServerConfigSpec::from_file("./examples/config.toml").unwrap();
        assert_eq!(config.router_id, IpAddr::from(Ipv4Addr::new(1, 1, 1, 1)));
        assert_eq!(config.default_as, 65000);
        assert_eq!(config.peers.len(), 3);
        let v4_peer = config
            .peers
            .iter()
            .find(|p| p.remote_ip == IpAddr::from(Ipv4Addr::new(127, 0, 0, 2)))
            .unwrap();
        assert_eq!(v4_peer.local_as, Some(65000));
        assert_eq!(v4_peer.hold_timer, 30);
        assert_eq!(v4_peer.dest_port, 1179);
        assert!(v4_peer.passive);

        let v6_peer = config
            .peers
            .iter()
            .find(|p| p.remote_ip == IpAddr::from("::2".parse::<Ipv6Addr>().unwrap()))
            .unwrap();
        assert_eq!(v6_peer.families.len(), 2);
        assert_eq!(v6_peer.hold_timer, 180);
        assert!(v6_peer.passive);
    }
}
