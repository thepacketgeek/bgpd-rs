use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::slice::Iter;

use serde::Serialize;

use crate::utils::{ext_community_to_display, u32_to_dotted};

#[derive(Serialize, Debug, Copy, Clone)]
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
            Community::STANDARD(value) => write!(f, "{}", u32_to_dotted(*value, ':')),
            Community::EXTENDED(value) => write!(f, "{}", ext_community_to_display(*value)),
        }
    }
}

impl TryFrom<&str> for Community {
    type Error = io::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Parse to list of u32, since we should support 4 byte aSN as a single int
        // (E.g. "42598400100")
        let chunks: Vec<_> = value.split(':').collect();
        match chunks.len() {
            1 => chunks[0]
                .parse()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid community"))
                .map(|c| Ok(Community::STANDARD(c)))?,
            2 => {
                let (a, b) = (
                    chunks[0].parse::<u32>().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "Invalid community")
                    })?,
                    chunks[1].parse::<u32>().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "Invalid community")
                    })?,
                );
                Ok(Community::STANDARD((a * 65536) + b))
            }
            _ => Ok(Community::EXTENDED(404)), // TODO: support extended community parsing
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct CommunityList(pub Vec<Community>);

impl CommunityList {
    pub fn iter(&self) -> Iter<Community> {
        self.0.iter()
    }

    pub fn standard(&self) -> Vec<u32> {
        self.0
            .iter()
            .filter_map(|c| {
                if let Community::STANDARD(comm) = c {
                    Some(*comm)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn extended(&self) -> Vec<u64> {
        self.0
            .iter()
            .filter_map(|c| {
                if let Community::EXTENDED(comm) = c {
                    Some(*comm)
                } else {
                    None
                }
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_community_list_serialize() {
        assert_eq!(
            CommunityList(vec![Community::STANDARD(100), Community::STANDARD(200)]).to_string(),
            "100 200"
        );
        assert_eq!(
            CommunityList(vec![
                Community::EXTENDED(279172874240100),
                Community::STANDARD(200)
            ])
            .to_string(),
            "65000:100 200"
        );
    }
}
