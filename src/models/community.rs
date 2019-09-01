use std::fmt;
use std::slice::Iter;

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
