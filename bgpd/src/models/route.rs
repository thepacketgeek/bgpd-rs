use std::convert::TryFrom;
use std::net::IpAddr;

use bgp_rs::{ASPath, Origin};
use bgpd_lib::utils::{as_path_from_string, as_path_to_string};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::types::Type;
use rusqlite::{Error as RError, Row};
use serde::{Deserialize, Serialize};

use super::community::CommunityList;

#[derive(Serialize, Deserialize, Debug)]
pub struct Route {
    pub received_from: IpAddr, // router_id
    pub received_at: DateTime<Utc>,
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
