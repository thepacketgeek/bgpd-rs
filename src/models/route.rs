use std::convert::TryFrom;
use std::net::IpAddr;

use bgp_rs::{ASPath, Origin};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Error as RError, Result as RResult, Row};
use serde::{Deserialize, Serialize};

use super::community::CommunityList;
use crate::utils::{as_path_from_string, as_path_to_string};

#[derive(Serialize, Deserialize, Debug)]
pub enum RouteState {
    Pending(DateTime<Utc>),
    Advertised(DateTime<Utc>),
    Received(DateTime<Utc>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Route {
    pub peer: IpAddr, // source or destination router_id
    pub state: RouteState,
    pub prefix: IpAddr,
    pub next_hop: IpAddr,
    #[serde(with = "serialize_origin")]
    pub origin: Origin,
    #[serde(with = "serialize_as_path")]
    pub as_path: ASPath,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    pub communities: CommunityList,
}

mod serialize_as_path {
    use super::{as_path_from_string, as_path_to_string};
    use bgp_rs::ASPath;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(as_path: &ASPath, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&as_path_to_string(as_path))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ASPath, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        as_path_from_string(&s).map_err(serde::de::Error::custom)
    }
}

mod serialize_origin {
    use bgp_rs::Origin;
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::convert::TryFrom;

    pub fn serialize<S>(origin: &Origin, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&String::from(origin))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Origin, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Origin::try_from(&s[..]).map_err(serde::de::Error::custom)
    }
}

impl<'a> TryFrom<&Row<'a>> for Route {
    type Error = RError;

    fn try_from(row: &Row) -> std::result::Result<Self, Self::Error> {
        let peer = row
            .get(0)
            .map(|prefix: String| prefix.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(0, Type::Text, Box::new(err)))?;
        let state = row.get(1)?;
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
            peer,
            state,
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

/// Encode a RouteState for SQL Storage
/// Will prepend initial for state (for decoding back to struct)
/// E.g.
///     s65000;e8008fde800000064
impl ToSql for RouteState {
    fn to_sql(&self) -> RResult<ToSqlOutput<'_>> {
        let result = match self {
            RouteState::Received(timestamp) => format!("r{}", timestamp.timestamp()),
            RouteState::Advertised(timestamp) => format!("a{}", timestamp.timestamp()),
            RouteState::Pending(timestamp) => format!("p{}", timestamp.timestamp()),
        };
        Ok(ToSqlOutput::from(result))
    }
}

/// Decode SQL back to RouteState
/// See `impl ToSql for RouteState` for details
impl FromSql for RouteState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_str().and_then(|value| {
            let (state, timestamp) = value.split_at(1);
            let timestamp = timestamp
                .parse::<i64>()
                .map(|ts| Utc.timestamp(ts, 0))
                .map_err(|err| FromSqlError::Other(Box::new(err)))?;
            match state {
                "r" => Ok(RouteState::Received(timestamp)),
                "a" => Ok(RouteState::Advertised(timestamp)),
                "p" => Ok(RouteState::Pending(timestamp)),
                _ => Err(FromSqlError::InvalidType),
            }
        })
    }
}
