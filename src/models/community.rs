use std::fmt;
use std::slice::Iter;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result;
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
pub struct CommunityList(pub Vec<Community>);

impl CommunityList {
    pub fn iter(&self) -> Iter<Community> {
        self.0.iter()
    }
}

impl fmt::Display for CommunityList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let communities = self
            .0
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(" ");
        write!(f, "{}", communities)
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
            .0
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
                return Ok(CommunityList(vec![]));
            }
            let mut parsed: Vec<Community> = Vec::new();
            for community in communities.split(';') {
                parsed.push(Community::parse_from_sql(community)?);
            }
            Ok(CommunityList(parsed))
        })
    }
}
