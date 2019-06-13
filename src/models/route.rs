use std::net::IpAddr;

use bgp_rs::{ASPath, Origin, Prefix};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::community::CommunityList;
use crate::utils::as_path_to_string;

#[derive(Serialize, Debug)]
pub enum RouteState {
    Pending(DateTime<Utc>),
    Advertised(DateTime<Utc>),
    Received(DateTime<Utc>),
}

#[derive(Serialize, Debug)]
pub struct Route {
    pub peer: IpAddr,      // source or destination router_id
    pub state: RouteState, // State of route (and timestamp of last state change)
    #[serde(with = "serialize_prefix")]
    pub prefix: Prefix,
    pub next_hop: IpAddr,
    #[serde(with = "serialize_origin")]
    pub origin: Origin,
    #[serde(with = "serialize_as_path")]
    pub as_path: ASPath,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    pub communities: CommunityList,
}

mod serialize_prefix {
    use bgp_rs::Prefix;
    use serde::{self, Serializer};

    pub fn serialize<S>(prefix: &Prefix, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&prefix.to_string())
    }
}

mod serialize_as_path {
    use super::as_path_to_string;
    use bgp_rs::ASPath;
    use serde::{self, Serializer};

    pub fn serialize<S>(as_path: &ASPath, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&as_path_to_string(as_path))
    }
}

mod serialize_origin {
    use bgp_rs::Origin;
    use serde::{self, Serializer};

    pub fn serialize<S>(origin: &Origin, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&origin.to_string())
    }
}
