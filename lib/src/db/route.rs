use std::convert::{From, TryFrom};
use std::fmt;
use std::net::IpAddr;
use std::string::ToString;

use bgp_rs::{ASPath, Origin, Segment};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use rusqlite::{Error as RError, Result, Row};

use crate::utils::{asn_to_dotted, ext_community_to_display};

#[derive(Debug, Clone)]
pub enum Community {
    // TODO: Consider another datamodel for these
    //       size of the max variant (EXTENDED) is much larger than
    //       the most typical use case (STANDARD)
    STANDARD(u32),
    EXTENDED(u64),
    // TODO
    // LARGE(Vec<(u32, u32, u32)>),
    // IPV6_EXTENDED((u8, u8, Ipv6Addr, u16)),
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

impl fmt::Display for Community {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Community::STANDARD(value) => write!(f, "{}", asn_to_dotted(*value)),
            Community::EXTENDED(value) => write!(f, "{}", ext_community_to_display(*value)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommunityList {
    communities: Vec<Community>,
}

impl CommunityList {
    pub fn new(communities: Vec<Community>) -> Self {
        CommunityList { communities }
    }
}

impl fmt::Display for CommunityList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let communities = self
            .communities
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(" ");
        write!(f, "{}", communities)
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

#[derive(Debug)]
pub struct Route {
    pub received_from: IpAddr, // router_id
    pub received_at: DateTime<Utc>,
    pub prefix: IpAddr,
    pub next_hop: IpAddr,
    pub origin: Origin,
    pub as_path: ASPath,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    pub communities: CommunityList,
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

fn as_path_from_string(as_path: &str) -> std::result::Result<ASPath, std::num::ParseIntError> {
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
