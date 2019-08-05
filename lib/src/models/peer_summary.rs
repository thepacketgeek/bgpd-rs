use std::convert::TryFrom;
use std::net::{AddrParseError, IpAddr};
use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::PeerState;

#[derive(Serialize, Deserialize, Debug)]
pub struct PeerSummary {
    pub neighbor: IpAddr,
    pub router_id: Option<IpAddr>,
    pub asn: u32,
    pub msg_received: Option<u64>,
    pub msg_sent: Option<u64>,
    // #[serde(with = "my_date_format")]
    pub connect_time: Option<DateTime<Utc>>,
    pub state: PeerState,
    pub prefixes_received: Option<u64>,
}

impl PeerSummary {
    pub fn new(addr: IpAddr, asn: u32, state: PeerState) -> Self {
        PeerSummary {
            neighbor: addr,
            router_id: None,
            asn,
            msg_received: None,
            msg_sent: None,
            connect_time: None,
            state,
            prefixes_received: None,
        }
    }
}
