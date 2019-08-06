use std::convert::TryFrom;
use std::net::{AddrParseError, IpAddr};
use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::types::Type;
use rusqlite::{Error as RError, Row};
use serde::{Deserialize, Serialize};

use super::PeerState;

#[derive(Serialize, Deserialize, Debug)]
pub struct PeerSummary {
    pub neighbor: IpAddr,
    pub router_id: Option<IpAddr>,
    pub asn: u32,
    pub msg_received: Option<u64>,
    pub msg_sent: Option<u64>,
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

impl<'a> TryFrom<&Row<'a>> for PeerSummary {
    type Error = RError;

    fn try_from(row: &Row) -> std::result::Result<Self, Self::Error> {
        let addr = row
            .get(0)
            .map(|prefix: String| prefix.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(0, Type::Text, Box::new(err)))?;
        let router_id = row
            .get(1)
            .map(|prefix: Option<String>| match prefix {
                Some(prefix) => {
                    let addr = prefix.parse::<IpAddr>()?;
                    Ok(Some(addr))
                }
                None => Ok(None),
            })?
            .map_err(|err: AddrParseError| {
                RError::FromSqlConversionFailure(0, Type::Text, Box::new(err))
            })?;

        let maybe_u64 = |count: Option<i64>| match count {
            Some(count) => Some(count as u64),
            None => None,
        };
        let connect_time = row.get(5).map(|timestamp: Option<i64>| match timestamp {
            Some(timestamp) => Some(Utc.timestamp(timestamp, 0)),
            None => None,
        })?;
        let state = row
            .get(6)
            .map(|state: String| PeerState::from_str(&state))?
            .map_err(|err| RError::FromSqlConversionFailure(6, Type::Text, Box::new(err)))?;
        let prefixes_received = match state {
            PeerState::Established => row.get(7).map(maybe_u64)?,
            _ => None,
        };
        Ok(PeerSummary {
            neighbor: addr,
            router_id,
            asn: row.get(2)?,
            msg_received: row.get(3).map(maybe_u64)?,
            msg_sent: row.get(4).map(maybe_u64)?,
            connect_time,
            state,
            prefixes_received,
        })
    }
}
