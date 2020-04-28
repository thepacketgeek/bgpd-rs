use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::io;
use std::net::{AddrParseError, IpAddr};
use std::num::ParseIntError;

use bgp_rs::{
    flowspec::{FlowspecFilter, NumericOperator},
    ASPath, NLRIEncoding, Origin, PathAttribute, Prefix, Segment, AFI, SAFI,
};
use bgpd_rpc_lib::{FlowSpec, RouteSpec, SpecAttributes};
use ipnetwork::{IpNetwork, NetworkSize};

use crate::rib::{Community, CommunityList, Family};

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

impl From<io::Error> for ParseError {
    fn from(error: io::Error) -> Self {
        ParseError::new(error.to_string())
    }
}

// Determine if a given IPNetwork is for a single host
// If so, return the IpAddr
pub fn get_host_address(network: &IpNetwork) -> Option<IpAddr> {
    let is_host = match network.size() {
        NetworkSize::V4(size) => size == 1,
        NetworkSize::V6(size) => size == 1,
    };
    if is_host {
        Some(network.ip())
    } else {
        None
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

/// Convert a CIDR prefix (E.g. "192.168.0.0/24") to a bgp_rs::Prefix
/// ```
/// use bgp_rs::Prefix;
/// use bgpd::utils::prefix_from_str;
/// let prefix = prefix_from_str("192.168.10.0/24").unwrap();
/// assert_eq!(prefix.length, 24);
/// ```
pub fn prefix_from_str(prefix: &str) -> std::result::Result<Prefix, ParseError> {
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

pub fn prefix_from_network(network: &IpNetwork) -> Prefix {
    let (protocol, octets) = match network {
        IpNetwork::V4(v4) => (AFI::IPV4, v4.ip().octets().to_vec()),
        IpNetwork::V6(v6) => (AFI::IPV6, v6.ip().octets().to_vec()),
    };
    Prefix {
        protocol,
        length: network.prefix(),
        prefix: octets,
    }
}

pub fn parse_route_spec(
    spec: &RouteSpec,
) -> Result<(Family, Vec<PathAttribute>, NLRIEncoding), ParseError> {
    let prefix = prefix_from_network(&spec.prefix);
    let mut attributes = parse_attributes(&spec.attributes)?;
    attributes.push(PathAttribute::NEXT_HOP(spec.next_hop));
    Ok((
        Family::new(prefix.protocol, SAFI::Unicast),
        attributes,
        NLRIEncoding::IP(prefix),
    ))
}

pub fn parse_flow_spec(
    spec: &FlowSpec,
) -> Result<(Family, Vec<PathAttribute>, NLRIEncoding), ParseError> {
    let family = Family::new(AFI::try_from(spec.afi)?, SAFI::Flowspec);
    let mut attributes = parse_attributes(&spec.attributes)?;
    // Parse Action
    attributes.push(parse_flowspec_action(&spec.action)?.into());
    // Parse Filters
    let filters: Vec<_> = spec
        .matches
        .iter()
        .map(|m| parse_flowspec_match(&m))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((family, attributes, NLRIEncoding::FLOWSPEC(filters)))
}

fn parse_attributes(attrs: &SpecAttributes) -> Result<Vec<PathAttribute>, ParseError> {
    let mut attributes = vec![PathAttribute::ORIGIN(
        attrs
            .origin
            .as_ref()
            .map(|o| match o.to_lowercase().as_str() {
                "igp" => Origin::IGP,
                "egp" => Origin::EGP,
                _ => Origin::INCOMPLETE,
            })
            .unwrap_or(Origin::INCOMPLETE),
    )];
    if let Some(local_pref) = attrs.local_pref {
        attributes.push(PathAttribute::LOCAL_PREF(local_pref));
    }
    if let Some(med) = attrs.multi_exit_disc {
        attributes.push(PathAttribute::MULTI_EXIT_DISC(med));
    }

    let as_path = {
        let mut asns: Vec<u32> = Vec::with_capacity(attrs.as_path.len());
        for asn in &attrs.as_path {
            asns.push(asn_from_dotted(asn).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Error parsing ASN: {}", err.reason),
                )
            })?);
        }
        ASPath {
            segments: vec![Segment::AS_SEQUENCE(asns)],
        }
    };
    attributes.push(PathAttribute::AS_PATH(as_path));
    let communities = {
        let mut comms: Vec<Community> = Vec::with_capacity(attrs.communities.len());
        for comm in &attrs.communities {
            comms.push(Community::try_from(comm.as_str())?);
        }
        CommunityList(comms)
    };
    let standard_communities = communities.standard();
    if !standard_communities.is_empty() {
        attributes.push(PathAttribute::COMMUNITY(standard_communities));
    }
    let extd_communities = communities.extended();
    if !extd_communities.is_empty() {
        attributes.push(PathAttribute::EXTENDED_COMMUNITIES(extd_communities));
    }
    Ok(attributes)
}

#[derive(Debug)]
pub enum FlowAction {
    /// Redirect (with 2:4 byte route target)
    Redirect(Community),
    /// Traffic Rate (with 2-byte ASN and 4 byte float)
    TrafficRate(f32),
    /// Action to take (sample, terminal)
    TrafficAction((bool, bool)),
    /// DSCP Value to mark
    MarkDSCP(u8),
}

impl From<FlowAction> for PathAttribute {
    fn from(action: FlowAction) -> PathAttribute {
        use FlowAction::*;
        let community = match action {
            TrafficRate(bps) => {
                let mut comm_bytes = [0x80, 0x06, 0, 0, 0, 0, 0, 0];
                comm_bytes[4..8].clone_from_slice(&bps.to_be_bytes());
                u64::from_be_bytes(comm_bytes)
            }
            TrafficAction((sample, terminal)) => {
                let mut comm_bytes = [0x80, 0x07, 0, 0, 0, 0, 0, 0];
                comm_bytes[0..7].clone_from_slice(&[0x80, 0x07, 0, 0, 0, 0, 0]);
                let mut val = 0u8;
                if sample {
                    val |= 0b10;
                }
                if terminal {
                    val |= 0b1;
                }
                comm_bytes[7] = val;
                u64::from_be_bytes(comm_bytes)
            }
            Redirect(comm) => match comm {
                Community::STANDARD(val) => {
                    let mut comm_bytes = [0u8; 8];
                    let bytes = val.to_be_bytes();
                    comm_bytes[0..2].clone_from_slice(&[0x80, 0x08]);
                    comm_bytes[2..4].clone_from_slice(&[bytes[0], bytes[1]]);
                    comm_bytes[4..6].clone_from_slice(&[0; 2]);
                    comm_bytes[6..8].clone_from_slice(&[bytes[2], bytes[3]]);
                    u64::from_be_bytes(comm_bytes)
                }
                _ => unreachable!(),
            },
            MarkDSCP(dscp) => {
                let mut comm_bytes = [0x80, 0x09, 0, 0, 0, 0, 0, 0];
                comm_bytes[7] = dscp;
                u64::from_be_bytes(comm_bytes)
            }
        };
        PathAttribute::EXTENDED_COMMUNITIES(vec![community])
    }
}

fn parse_flowspec_action(action: &str) -> Result<FlowAction, ParseError> {
    let words: Vec<_> = action.split_whitespace().collect();
    if words.is_empty() {
        return Err(ParseError::new(String::from("No FlowSpec Action found")));
    }
    if words.len() < 2 {
        return Err(ParseError::new(format!(
            "Cannot parse action: '{}'",
            action
        )));
    }
    match words[0].to_lowercase().as_str() {
        "redirect" => Ok(FlowAction::Redirect(
            Community::try_from(words[1]).map_err(|_| {
                ParseError::new(format!("Unable to parse redirect community '{}'", words[1]))
            })?,
        )),
        "traffic-action" => Ok(FlowAction::TrafficAction((
            words.contains(&"sample"),
            false,
        ))),
        "traffic-rate" => Ok(FlowAction::TrafficRate(words[1].parse::<f32>().map_err(
            |_| ParseError::new(format!("Unable to parse traffic-rate bps '{}'", words[1])),
        )?)),
        "mark" => {
            let dscp = words[1].parse::<u8>().map_err(|_| {
                ParseError::new(format!("Unable to parse DSCP value '{}'", words[1]))
            })?;
            if dscp > 64 {
                return Err(ParseError::new(format!(
                    "Not a valid DSCP value '{}'",
                    dscp
                )));
            }
            Ok(FlowAction::MarkDSCP(dscp))
        }
        _ => Err(ParseError::new(format!(
            "Unsupported Flowspec Action: {}",
            words[0]
        ))),
    }
}

fn parse_flowspec_match(rule: &str) -> Result<FlowspecFilter, ParseError> {
    let words: Vec<_> = rule.splitn(2, ' ').collect();
    if words.is_empty() {
        return Err(ParseError::new(String::from("No FlowSpec Match found")));
    }
    if words.len() < 2 {
        return Err(ParseError::new(format!("Cannot parse match: '{}'", rule)));
    }
    match words[0].to_lowercase().as_str() {
        "destination" => {
            let dest = prefix_from_str(words[1])
                .map_err(|_| ParseError::new(format!("Unable to parse prefix '{}'", words[1])))?;
            Ok(FlowspecFilter::DestinationPrefix(dest))
        }
        "source" => {
            let src = prefix_from_str(words[1])
                .map_err(|_| ParseError::new(format!("Unable to parse prefix '{}'", words[1])))?;
            Ok(FlowspecFilter::SourcePrefix(src))
        }
        "protocol" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::IpProtocol(values))
        }
        "port" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::Port(values))
        }
        "destination-port" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::DestinationPort(values))
        }
        "source-port" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::SourcePort(values))
        }
        "icmp-type" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::IcmpType(values))
        }
        "icmp-code" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::IcmpCode(values))
        }
        "packet-length" => {
            let values: Vec<_> = words[1]
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| parse_num_operator(&w, i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FlowspecFilter::PacketLength(values))
        }
        _ => Err(ParseError::new(format!(
            "Unsupported Flowspec Match: {}",
            words[0]
        ))),
    }
}

fn parse_num_operator<T>(word: &str, index: usize) -> Result<(NumericOperator, T), ParseError>
where
    T: std::str::FromStr,
{
    let mut pos = 0usize;
    let mut oper = NumericOperator::new(0);
    for (i, chr) in word.chars().enumerate() {
        match chr {
            // '&' => oper |= NumericOperator::AND,
            '>' => oper |= NumericOperator::GT,
            '<' => oper |= NumericOperator::LT,
            '=' => oper |= NumericOperator::EQ,
            _ => {
                pos = i;
                break;
            }
        }
    }
    let value = word[pos..]
        .parse()
        .map_err(|_| ParseError::new(format!("Unable to parse '{}'", word)))?;
    // No operator was included (I.e. "8080" instead of "=8080"), assume EQ
    if oper.is_empty() {
        oper |= NumericOperator::EQ;
    }
    // All subsequent items in the same filter are AND'd
    if index > 0 {
        oper |= NumericOperator::AND;
    }
    Ok((oper, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_get_host_address() {
        assert!(get_host_address(&"1.1.1.0/24".parse::<IpNetwork>().unwrap()).is_none(),);
        assert_eq!(
            get_host_address(&"1.1.1.1".parse::<IpNetwork>().unwrap()),
            Some(IpAddr::from(Ipv4Addr::new(1, 1, 1, 1)))
        );
        assert!(get_host_address(&"2001:1:2::10".parse::<IpNetwork>().unwrap()).is_some(),);
        assert!(get_host_address(&"2001:1:2::10/64".parse::<IpNetwork>().unwrap()).is_none());
    }

    #[test]
    fn test_asn_from_dotted() {
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
        let prefix = prefix_from_str("1.1.1.0/24").unwrap();
        assert_eq!(prefix.length, 24);
        assert_eq!(prefix.prefix, [1, 1, 1, 0]);

        let prefix = prefix_from_str("2001:10::2/64").unwrap();
        assert_eq!(prefix.length, 64);
        assert_eq!(
            prefix.prefix,
            [32, 1, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]
        );
    }
    #[test]
    fn test_flowspec_actions() {
        match parse_flowspec_action("redirect 6:302") {
            Ok(FlowAction::Redirect(comm)) => {
                assert_eq!(String::from("6:302"), comm.to_string());
            }
            _ => panic!(),
        }
        match parse_flowspec_action("redirect vrf:test") {
            Err(_) => (),
            _ => panic!(),
        }
        match parse_flowspec_action("traffic-rate 1000000") {
            Ok(FlowAction::TrafficRate(bps)) => {
                assert_eq!(bps, 1000000.0);
            }
            _ => panic!(),
        }
        match parse_flowspec_action("traffic-rate 10Kbps") {
            Err(_) => (),
            _ => panic!(),
        }
        match parse_flowspec_action("traffic-action sample") {
            Ok(FlowAction::TrafficAction((sample, _terminal))) => {
                assert!(sample);
            }
            _ => panic!(),
        }
        match parse_flowspec_action("mark 63") {
            Ok(FlowAction::MarkDSCP(dscp)) => {
                assert_eq!(dscp, 63);
            }
            _ => panic!(),
        }
        match parse_flowspec_action("mark 255") {
            Err(_) => (),
            _ => panic!(),
        }
    }

    #[test]
    fn test_flowspec_matches() {
        match parse_flowspec_match("source 192.168.10.0/24") {
            Ok(FlowspecFilter::SourcePrefix(prefix)) => {
                assert_eq!(prefix.protocol, AFI::IPV4);
            }
            _ => panic!(),
        }
        match parse_flowspec_match("destination 3001:100::/64") {
            Ok(FlowspecFilter::DestinationPrefix(prefix)) => {
                assert_eq!(prefix.protocol, AFI::IPV6);
            }
            _ => panic!(),
        }
        match parse_flowspec_match("protocol =6") {
            Ok(FlowspecFilter::IpProtocol(values)) => {
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].1, 6);
                assert_eq!(values[0].0, NumericOperator::EQ);
            }
            _ => panic!(),
        }
        match parse_flowspec_match("protocol <=17") {
            Ok(FlowspecFilter::IpProtocol(values)) => {
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].1, 17);
                assert_eq!(values[0].0, NumericOperator::EQ | NumericOperator::LT);
            }
            _ => panic!(),
        }
        match parse_flowspec_match("port >=1024") {
            Ok(FlowspecFilter::Port(values)) => {
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].1, 1024);
                assert_eq!(values[0].0, NumericOperator::EQ | NumericOperator::GT);
            }
            _ => panic!(),
        }
        match parse_flowspec_match("port 443") {
            Ok(FlowspecFilter::Port(values)) => {
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].1, 443);
                assert_eq!(values[0].0, NumericOperator::EQ);
            }
            _ => panic!(),
        }
        match parse_flowspec_match("destination-port >8000 <=8080") {
            Ok(FlowspecFilter::DestinationPort(values)) => {
                assert_eq!(values.len(), 2);
                assert_eq!(values[0].1, 8000);
                assert_eq!(values[0].0, NumericOperator::GT);
                assert_eq!(values[1].1, 8080);
                assert_eq!(
                    values[1].0,
                    NumericOperator::LT | NumericOperator::EQ | NumericOperator::AND
                );
            }
            _ => panic!(),
        }
        match parse_flowspec_match("source-port =179") {
            Ok(FlowspecFilter::SourcePort(values)) => {
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].1, 179);
                assert_eq!(values[0].0, NumericOperator::EQ);
            }
            _ => panic!(),
        }
    }
}
