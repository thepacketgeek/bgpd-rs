use std::convert::TryFrom;
use std::net::{AddrParseError, IpAddr};
use std::str::FromStr;

use bgp_rs::Origin;
use bgpd_lib::models::{
    as_path_from_string, Community, CommunityList, PeerState, PeerSummary, Route,
};
use chrono::{TimeZone, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Connection, Error as RError, Result, Row, NO_PARAMS};

use super::DBTable;

impl DBTable for Route {
    fn create_table(conn: &Connection) -> Result<usize> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS routes (
                id INTEGER PRIMARY KEY,
                router_id TEXT NOT NULL,
                received_at BIGINT NOT NULL,
                prefix TEXT NOT NULL,
                next_hop TEXT NOT NULL,
                origin TEXT NOT NULL,
                as_path TEXT NOT NULL,
                local_pref INTEGER,
                metric INTEGER,
                communities TEXT NOT NULL
            )",
            NO_PARAMS,
        )
    }
}

impl<'a> TryFrom<&Row<'a>> for Route {
    type Error = RError;

    fn try_from(row: &Row) -> std::result::Result<Self, Self::Error> {
        let received_from = row
            .get(0)
            .map(|prefix: String| prefix.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(0, Type::Text, Box::new(err)))?;
        let received_at = row
            .get(1)
            .map(|timestamp: i64| Utc.timestamp(timestamp, 0))?;
        let prefix = row
            .get(2)
            .map(|prefix: String| prefix.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(2, Type::Text, Box::new(err)))?;
        let next_hop = row
            .get(3)
            .map(|next_hop: String| next_hop.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(3, Type::Text, Box::new(err)))?;
        let origin = row
            .get(4)
            .map(|origin: String| Origin::try_from(&origin[..]))?
            .map_err(|err| RError::FromSqlConversionFailure(4, Type::Text, Box::new(err)))?;
        let as_path = row
            .get(5)
            .map(|as_path: String| as_path_from_string(&as_path))?
            .map_err(|err| RError::FromSqlConversionFailure(5, Type::Text, Box::new(err)))?;
        Ok(Route {
            received_from,
            received_at,
            prefix,
            next_hop,
            origin,
            as_path,
            local_pref: row.get(6)?,
            multi_exit_disc: row.get(7)?,
            communities: row.get(8)?,
        })
    }
}

impl DBTable for PeerSummary {
    fn create_table(conn: &Connection) -> Result<usize> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS peers (
                id INTEGER PRIMARY KEY,
                neighbor TEXT NOT NULL UNIQUE,
                router_id TEXT,
                asn BIGINT NOT NULL,
                msg_received BIGINT,
                msg_sent BIGINT,
                connect_time BIGINT,
                state TEXT NOT NULL
            )",
            NO_PARAMS,
        )
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

impl Community {
    fn parse_from_sql(value: &str) -> std::result::Result<Community, FromSqlError> {
        let rerr = |err| FromSqlError::Other(Box::new(err));
        match &value[..1] {
            "s" => {
                let community = value[1..].parse::<u32>().map_err(rerr)?;
                Ok(Community::STANDARD(community))
            }
            "e" => {
                let community = value[1..].parse::<u64>().map_err(rerr)?;
                Ok(Community::EXTENDED(community))
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

/// Encode a CommunityList for SQL Storage
/// Will prepend initial for Community Type (for decoding back to struct)
/// E.g.
///     s65000;e8008fde800000064
impl ToSql for CommunityList {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>> {
        let result = self
            .communities
            .iter()
            .map(|community| match community {
                Community::STANDARD(community) => format!("s{}", community.to_string()),
                Community::EXTENDED(community) => format!("e{}", community.to_string()),
            })
            .collect::<Vec<String>>()
            .join(";");
        Ok(ToSqlOutput::from(result))
    }
}

/// Decode SQL back to CommunityList
/// See `impl ToSql for CommunityList` for details
impl FromSql for CommunityList {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_str().and_then(|communities| {
            if communities.is_empty() {
                return Ok(CommunityList::new(vec![]));
            }
            let mut parsed: Vec<Community> = Vec::new();
            for community in communities.split(';') {
                parsed.push(Community::parse_from_sql(community)?);
            }
            Ok(CommunityList::new(parsed))
        })
    }
}
