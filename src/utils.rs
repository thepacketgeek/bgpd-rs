use std::error::Error;
use std::fmt;
use std::net::{AddrParseError, IpAddr};
use std::num::ParseIntError;

use bgp_rs::{ASPath, Message, Prefix, Segment, AFI};
use chrono::{DateTime, Duration, TimeZone, Utc};

#[derive(Debug)]
pub struct ParseError {
    pub reason: String,
}

impl ParseError {
    pub fn new(reason: String) -> Self {
        ParseError { reason }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ParseError: {}", self.reason)
    }
}

impl Error for ParseError {
    fn description(&self) -> &str {
        "Error parsing to/from IP/BGP messages"
    }
}

pub fn get_message_type(message: &Message) -> &'static str {
    match message {
        Message::KeepAlive => "KEEPALIVE",
        Message::Open(_) => "OPEN",
        Message::Notification(_) => "NOTIFICATION",
        Message::RouteRefresh(_) => "ROUTEREFRESH",
        Message::Update(_) => "UPDATE",
    }
}

pub fn transform_u8_to_bytes(x: u8) -> [u8; 1] {
    let b1: u8 = x as u8;
    [b1]
}

pub fn transform_u16_to_bytes(x: u16) -> [u8; 2] {
    let b1: u8 = ((x >> 8) & 0xff) as u8;
    let b2: u8 = (x & 0xff) as u8;
    [b1, b2]
}

pub fn transform_u32_to_bytes(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;
    [b1, b2, b3, b4]
}

pub fn as_u32_be(array: [u8; 4]) -> u32 {
    (u32::from(array[0]) << 24)
        + (u32::from(array[1]) << 16)
        + (u32::from(array[2]) << 8)
        + u32::from(array[3])
}

pub fn as_u16_be(array: [u8; 2]) -> u16 {
    (u16::from(array[0]) << 8) + u16::from(array[1])
}

/// Convert an ASN (4 byte) as dotted if it exceeds the 2-byte limit
/// E.g. 42598400100 -> "65000.100"
pub fn u32_to_dotted(asn: u32, sep: char) -> String {
    if asn < std::u16::MAX as u32 {
        format!("{}", asn)
    } else {
        let bytes = transform_u32_to_bytes(asn);
        format!(
            "{}{}{}",
            as_u16_be([bytes[0], bytes[1]]),
            sep,
            as_u16_be([bytes[2], bytes[3]])
        )
    }
}

/// Convert an ASN string to a u32
/// E.g. "65000.100" -> 42598400100
pub fn asn_from_dotted(value: &str) -> std::result::Result<u32, ParseError> {
    // Parse to list of u32, since we should support 4 byte aSN as a single int
    // (E.g. "42598400100")
    let mut chunks = [0; 2];
    let check_for_overflow = value.contains('.');
    // Iterate through chunks in reverse, so if there's no dot (only one number),
    // it will be in the least significant position
    for (i, chunk) in value
        .splitn(2, '.')
        .collect::<Vec<&str>>()
        .into_iter()
        .rev()
        .enumerate()
    {
        let chunk: u32 = chunk
            .parse()
            .map_err(|err| ParseError::new(format!("{} '{}'", err, value)))?;
        if check_for_overflow && chunk > std::u16::MAX as u32 {
            return Err(ParseError::new(format!("Unsupported ASN '{}'", value)));
        }
        chunks[i] = chunk;
    }
    Ok((chunks[1] * 65536) + chunks[0])
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

pub fn prefix_from_string(prefix: &str) -> std::result::Result<Prefix, ParseError> {
    if let Some(i) = prefix.find('/') {
        let (addr, mask) = prefix.split_at(i);
        let mask = &mask[1..]; // Skip remaining '/'
        let addr: IpAddr = addr
            .parse()
            .map_err(|err: AddrParseError| ParseError::new(format!("{} '{}'", err, prefix)))?;
        let length: u8 = mask
            .parse()
            .map_err(|err: ParseIntError| ParseError::new(format!("{} '{}'", err, prefix)))?;
        let (protocol, octets) = match addr {
            IpAddr::V4(v4) => (AFI::IPV4, v4.octets().to_vec()),
            IpAddr::V6(v6) => (AFI::IPV6, v6.octets().to_vec()),
        };
        Ok(Prefix {
            protocol,
            length,
            prefix: octets,
        })
    } else {
        Err(ParseError {
            reason: format!("Not a valid prefix: '{}'", prefix),
        })
    }
}

/// Convert first 16 bytes (1 IPv6 address) to IpAddr
/// TODO: Handle multiple next hops
///       Can they be variable length?
pub fn bytes_to_ipv6(bytes: &Vec<u8>) -> IpAddr {
    let mut buffer: [u8; 16] = [0; 16];
    buffer[..16].clone_from_slice(&bytes[..16]);
    IpAddr::from(buffer)
}

pub fn ext_community_to_display(value: u64) -> String {
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
            format!("{}:{}", IpAddr::from(transform_u32_to_bytes(addr)), asn)
        }
        // 4-octet AS Specific BGP Extended Community (RFC 5668)
        0x2 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let addr: u32 = (value & 0xffff_ffff) as u32;
            format!(
                "target:{}:{}",
                asn,
                IpAddr::from(transform_u32_to_bytes(addr))
            )
        }
        // Opaque Extended Community (RFC 4360)
        0x3 => format!("opaque:{}", value),
        // Flow-Spec Traffic Rate community
        0x6 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            format!("traffic-rate:{}:?", asn)
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
        _ => format!("unknown:{}:{}", c_type, value),
    }
}

fn fit_with_remainder(dividend: u64, divisor: u64) -> (u64, u64) {
    let fit = dividend / divisor;
    let remainder = dividend % divisor;
    (fit, remainder)
}

pub fn get_elapsed_time<Tz>(time: DateTime<Tz>) -> Duration
where
    Tz: TimeZone,
{
    Utc::now().signed_duration_since(time)
}

/// Given a duration, format like "00:00:00"
pub fn format_elapsed_time(elapsed: Duration) -> String {
    let elapsed = elapsed.num_seconds().abs() as u64;
    let (hours, remainder) = fit_with_remainder(elapsed, 3600);
    let (minutes, seconds) = fit_with_remainder(remainder, 60);
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Given a timestamp, get the elapsed time and return formatted string
pub fn format_time_as_elapsed<Tz>(time: DateTime<Tz>) -> String
where
    Tz: TimeZone,
{
    format_elapsed_time(get_elapsed_time(time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u32_transforms() {
        assert_eq!(transform_u32_to_bytes(65000), [0, 0, 253, 232]); // u8
        assert_eq!(as_u32_be([0, 0, 253, 232]), 65000);

        assert_eq!(transform_u32_to_bytes(65300), [0x0, 0x0, 0xff, 0x14]); // hex
        assert_eq!(as_u32_be([0x0, 0x0, 0xff, 0x14]), 65300);
    }

    #[test]
    fn test_u16_transforms() {
        assert_eq!(transform_u16_to_bytes(180), [0, 180]); // u8
        assert_eq!(as_u16_be([0, 180]), 180);

        assert_eq!(transform_u16_to_bytes(300), [0x01, 0x2c]); // u8
        assert_eq!(as_u16_be([0x01, 0x2c]), 300); // hex
    }

    #[test]
    fn test_u8_transforms() {
        assert_eq!(transform_u8_to_bytes(180), [180]);
        assert_eq!(transform_u8_to_bytes(252), [0xfc]); // u8
        assert_eq!(as_u16_be([0x01, 0x2c]), 300); // hex
    }

    #[test]
    fn test_as_u32_be() {
        assert_eq!(as_u32_be([1, 1, 1, 1]), 16843009);
    }

    #[test]
    fn test_u32_to_dotted() {
        assert_eq!(u32_to_dotted(100, '.'), "100".to_string());
        assert_eq!(u32_to_dotted(4259840100, '.'), "65000.100".to_string());
    }
    #[test]
    fn test_asn_from_dotted() {
        dbg!(&std::u16::MAX);
        assert_eq!(asn_from_dotted("100").unwrap(), 100);
        assert_eq!(asn_from_dotted("65000.100").unwrap(), 4259840100);
        assert_eq!(asn_from_dotted("4259840100").unwrap(), 4259840100);
        assert!(asn_from_dotted("4259840100.200").is_err());
        assert!(asn_from_dotted("200.4259840100").is_err());
        assert!(asn_from_dotted("100.200300").is_err());
        assert!(asn_from_dotted("test").is_err());
    }

    #[test]
    fn test_prefix_from_string() {
        let prefix = prefix_from_string("1.1.1.0/24").unwrap();
        assert_eq!(prefix.length, 24);
        assert_eq!(prefix.prefix, [1, 1, 1, 0]);

        let prefix = prefix_from_string("2001:10::2/64").unwrap();
        assert_eq!(prefix.length, 64);
        assert_eq!(
            prefix.prefix,
            [32, 1, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]
        );
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

    #[test]
    fn test_format_elapsed_time() {
        assert_eq!(
            format_elapsed_time(Duration::seconds(30)),
            "00:00:30".to_string()
        );
        assert_eq!(
            format_elapsed_time(Duration::seconds(301)),
            "00:05:01".to_string()
        );
        assert_eq!(
            format_elapsed_time(Duration::seconds(32768)),
            "09:06:08".to_string()
        );
    }

    #[test]
    fn test_format_time_as_elapsed() {
        let interval = Utc::now() - Duration::seconds(14);
        assert_eq!(format_time_as_elapsed(interval), "00:00:14".to_string());
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
            String::from("traffic-rate:65000:?")
        );
        let traffic_action: u64 =
            u64::from_be_bytes([0x80, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02]);
        assert_eq!(
            ext_community_to_display(traffic_action),
            String::from("traffic-action:0:sample")
        );
    }
}
