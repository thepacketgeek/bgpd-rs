use std::convert::TryFrom;
use std::net::IpAddr;

use bgp_rs::{ASPath, Origin, Segment};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Connection, Error as RError, Result, Row, NO_PARAMS};
use serde::{Deserialize, Serialize};

use bgpd_lib::utils::{asn_to_dotted, ext_community_to_display};

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

pub fn as_path_to_string(as_path: &ASPath) -> String {
    fn asns_to_string(asns: &[u32]) -> String {
        asns.iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(",")
    }

    fn segment_to_string(segment: &Segment) -> String {
        match segment {
            Segment::AS_SEQUENCE(sequence) => format!("seq:{}", asns_to_string(&sequence)),
            Segment::AS_SET(set) => format!("set:{}", asns_to_string(&set)),
        }
    }

    as_path
        .segments
        .iter()
        .map(segment_to_string)
        .collect::<Vec<String>>()
        .join(";")
}

pub fn as_path_from_string(as_path: &str) -> std::result::Result<ASPath, std::num::ParseIntError> {
    fn segment_from_string(
        segment: &str,
    ) -> std::result::Result<Option<Segment>, std::num::ParseIntError> {
        if let Some(i) = segment.find(':') {
            let (segment_type, paths) = segment.split_at(i + 1);
            let paths = paths
                .split(',')
                .collect::<Vec<&str>>()
                .into_iter()
                .map(|asn: &str| asn.parse::<u32>().unwrap())
                .collect();
            if segment_type.starts_with("seq") {
                Ok(Some(Segment::AS_SEQUENCE(paths)))
            } else {
                Ok(Some(Segment::AS_SET(paths)))
            }
        } else {
            Ok(None)
        }
    }

    let parts = as_path.split(';').collect::<Vec<&str>>();
    let mut segments: Vec<Segment> = Vec::new();
    for part in parts {
        match segment_from_string(part) {
            Ok(Some(segment)) => segments.push(segment),
            Err(err) => return Err(err),
            _ => continue,
        }
    }
    Ok(ASPath { segments })
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
    use std::string::ToString;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_path_from_string() {
        let as_path = as_path_from_string("seq:100,200").unwrap();
        if let Segment::AS_SEQUENCE(seq) = &as_path.segments[0] {
            assert_eq!(seq, &vec![100 as u32, 200 as u32]);
        } else {
            panic!("Segment sequence did not match!");
        }

        let as_path = as_path_from_string("").unwrap();
        assert_eq!(as_path.segments.len(), 0);
    }

    #[test]
    fn test_as_path_to_string() {
        let as_path = ASPath {
            segments: vec![Segment::AS_SEQUENCE(vec![100, 200])],
        };
        let as_path_str = as_path_to_string(&as_path);
        assert_eq!(&as_path_str, "seq:100,200");

        let as_path_str = as_path_to_string(&ASPath { segments: vec![] });
        assert_eq!(&as_path_str, "");
    }
}
