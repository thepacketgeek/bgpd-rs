use std::convert::From;
use std::net::IpAddr;

use bgp_rs::Segment;
use bgpd_lib::utils::{as_path_from_string, asn_to_dotted, format_time_as_elapsed, EMPTY_VALUE};
use chrono::DateTime;
use prettytable::{cell, row, Row};
use serde_json::{self, Value};

use crate::table::ToRow;

pub fn maybe_string<T>(item: Option<&T>) -> String
where
    T: ToString,
{
    item.map(std::string::ToString::to_string)
        .unwrap_or_else(|| String::from(EMPTY_VALUE))
}

pub fn should_exist(value: Option<&Value>) -> &Value {
    value.expect("Provide a valid JSON key")
}

pub fn as_possible_string(value: Option<&Value>) -> String {
    match should_exist(value) {
        Value::Null => String::from(EMPTY_VALUE),
        Value::Number(inner) => inner.to_string(),
        Value::String(inner) => inner.to_owned(),
        _ => unreachable!(),
    }
}

pub struct PeerSummaryRow(pub Value);

impl ToRow for &PeerSummaryRow {
    fn columns() -> Row {
        row![
            "Neighbor",
            "Router ID",
            "AS",
            "MsgRcvd",
            "MsgSent",
            "Uptime",
            "State",
            "PfxRcd"
        ]
    }

    fn to_row(&self) -> Result<Row, String> {
        let peer = &self.0;
        let row = row![
            should_exist(peer.get("neighbor")).as_str().unwrap(),
            as_possible_string(peer.get("router_id")),
            match should_exist(peer.get("asn")) {
                Value::Number(asn) => {
                    let asn = asn.as_u64().expect("ASN must be valid integer");
                    asn_to_dotted(asn as u32)
                }
                _ => unreachable!(),
            },
            as_possible_string(peer.get("msg_received")),
            as_possible_string(peer.get("msg_sent")),
            match should_exist(peer.get("connect_time")) {
                Value::Null => String::from(EMPTY_VALUE),
                Value::String(connect_time) => format_time_as_elapsed(
                    DateTime::parse_from_rfc3339(connect_time.as_str()).unwrap()
                ),
                _ => unreachable!(),
            },
            should_exist(peer.get("state")).as_str().unwrap(),
            as_possible_string(peer.get("prefixes_received")),
        ];
        Ok(row)
    }
}

pub struct RouteRow(pub Value);

impl ToRow for &RouteRow {
    fn columns() -> Row {
        row![
            "Neighbor",
            "AFI",
            "Prefix",
            "Next Hop",
            "Age",
            "Origin",
            "Local Pref",
            "Metric",
            "AS Path",
            "Communities"
        ]
    }

    fn to_row(&self) -> Result<Row, String> {
        let route = &self.0;
        let prefix = {
            let prefix = should_exist(route.get("prefix"));
            prefix
                .as_str()
                .unwrap()
                .parse::<IpAddr>()
                .expect("Must have valid Prefix")
        };
        let afi = match prefix {
            IpAddr::V4(_) => "IPv4",
            IpAddr::V6(_) => "IPv6",
        };
        // TODO, this just displays the first segment as space delimited ASNs
        // Should it display more?
        let as_path = {
            let json = should_exist(route.get("as_path")).as_str().unwrap();
            let as_path = as_path_from_string(json).map_err(|err| format!("{}", err))?;
            match as_path.segments.iter().next() {
                Some(segment) => {
                    let path = match segment {
                        Segment::AS_SEQUENCE(sequence) => sequence,
                        Segment::AS_SET(set) => set,
                    };
                    Some(
                        path.iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<String>>()
                            .join(" "),
                    )
                }
                None => None,
            }
        };
        let row = row![
            should_exist(route.get("received_from")).as_str().unwrap(),
            afi,
            prefix,
            should_exist(route.get("next_hop")).as_str().unwrap(),
            match should_exist(route.get("received_at")) {
                // Value::Null => String::from(EMPTY_VALUE),
                Value::String(connect_time) => format_time_as_elapsed(
                    DateTime::parse_from_rfc3339(connect_time.as_str()).unwrap()
                ),
                _ => unreachable!(),
            },
            should_exist(route.get("origin")).as_str().unwrap(),
            as_possible_string(route.get("local_pref")),
            as_possible_string(route.get("multi_exit_disc")),
            maybe_string(as_path.as_ref()),
            maybe_string(route.get("communities")),
        ];
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maybe_string() {
        let value: Option<u64> = Some(5);
        assert_eq!(maybe_string(value.as_ref()), String::from("5"));
        let value: Option<&str> = Some("test");
        assert_eq!(maybe_string(value.as_ref()), String::from("test"));
        let value: Option<&str> = None;
        assert_eq!(maybe_string(value.as_ref()), String::from(EMPTY_VALUE));
    }
}
