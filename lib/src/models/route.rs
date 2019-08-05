use std::convert::{From, TryFrom};
use std::fmt;
use std::net::IpAddr;
use std::string::ToString;

use bgp_rs::{ASPath, Origin, Segment};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::utils::{asn_to_dotted, ext_community_to_display};

#[derive(Serialize, Deserialize, Debug, Clone)]
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

impl fmt::Display for Community {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Community::STANDARD(value) => write!(f, "{}", asn_to_dotted(*value)),
            Community::EXTENDED(value) => write!(f, "{}", ext_community_to_display(*value)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
