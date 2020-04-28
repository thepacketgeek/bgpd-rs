use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::IpAddr;
use std::slice::Iter;

use serde::Serialize;

use crate::utils::u32_to_dotted;

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
            // TODO: support extended community parsing
            _ => unimplemented!(),
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

fn ext_community_to_display(value: u64) -> String {
    let c_type: u16 = ((value >> 48) & 0xff) as u16;
    match c_type {
        // 2-octet AS Specific Extended Community (RFC 4360)
        0x0 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let community: u32 = (value & 0xffff_ffff) as u32;
            format!("{}:{}", asn, u32_to_dotted(community, '.'))
        }
        // IPv4 Address Specific Extended Community (RFC 4360)
        0x1 => {
            let addr: u32 = ((value >> 24) & 0xffff_ffff) as u32;
            let asn: u16 = (value & 0xffff) as u16;
            format!("{}:{}", IpAddr::from(addr.to_be_bytes()), asn)
        }
        // 4-octet AS Specific BGP Extended Community (RFC 5668)
        0x2 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let addr: u32 = (value & 0xffff_ffff) as u32;
            format!("target:{}:{}", asn, IpAddr::from(addr.to_be_bytes()))
        }
        // Opaque Extended Community (RFC 4360)
        0x3 => format!("opaque:{}", value),
        // Flow-Spec Traffic Rate community
        0x6 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let rate = f32::from_bits((value & 0xffff_ffff) as u32);
            format!("traffic-rate:{}:{}bps", asn, rate)
        }
        // Flow-Spec Traffic Action community
        0x7 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let action: u32 = (value & 0xffff_ffff) as u32;
            let sample: bool = (action << 1) != 0;
            let desc = if sample {
                "sample".to_string()
            } else {
                action.to_string()
            };
            format!("traffic-action:{}:{}", asn, desc)
        }
        // Flow-Spec Redirect community
        0x8 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let number: u32 = (value & 0xffff_ffff) as u32;
            format!("redirect:{}:{}", asn, u32_to_dotted(number, '.'))
        }
        // Flow-Spec Marking community
        0x9 => {
            let dscp: u8 = (value & 0xff) as u8;
            format!("traffic-marking:{}", dscp)
        }
        _ => format!("unknown:{}:{}", c_type, value),
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

    #[test]
    fn test_ext_community_to_display() {
        let two_byte_asn: u64 =
            u64::from_be_bytes([0x00, 0x00, 0xfd, 0xe8, 0x00, 0x64, 0x00, 0x64]);
        assert_eq!(
            ext_community_to_display(two_byte_asn),
            String::from("65000:100.100")
        );

        let ipv4_comm: u64 = u64::from_be_bytes([0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x64]);
        assert_eq!(
            ext_community_to_display(ipv4_comm),
            String::from("1.1.1.1:100")
        );

        let target: u64 = u64::from_be_bytes([0x00, 0x02, 0xfd, 0xe8, 0x01, 0x01, 0x01, 0x01]);
        assert_eq!(
            ext_community_to_display(target),
            String::from("target:65000:1.1.1.1")
        );

        let redirect: u64 = u64::from_be_bytes([0x80, 0x08, 0xfd, 0xe8, 0x00, 0x00, 0x00, 0x64]);
        assert_eq!(
            ext_community_to_display(redirect),
            String::from("redirect:65000:100")
        );

        let traffic_rate: u64 =
            u64::from_be_bytes([0x80, 0x06, 0xfd, 0xe8, 0x3f, 0xa0, 0x00, 0x00]);
        assert_eq!(
            ext_community_to_display(traffic_rate),
            String::from("traffic-rate:65000:1.25bps")
        );
        let traffic_action: u64 =
            u64::from_be_bytes([0x80, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02]);
        assert_eq!(
            ext_community_to_display(traffic_action),
            String::from("traffic-action:0:sample")
        );
    }
}
