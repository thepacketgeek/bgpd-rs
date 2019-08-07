use std::convert::From;
use std::net::IpAddr;

use prettytable::{cell, row, Row};
use serde_json::{self, Value};

use crate::table::ToRow;

pub const EMPTY_VALUE: &str = "";

pub fn should_exist(value: Option<&Value>) -> &Value {
    value.expect("Provide a valid JSON key")
}

pub fn display_cell(value: Option<&Value>) -> String {
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
            display_cell(peer.get("neighbor")),
            display_cell(peer.get("router_id")),
            display_cell(peer.get("asn")),
            display_cell(peer.get("msg_received")),
            display_cell(peer.get("msg_sent")),
            display_cell(peer.get("uptime")),
            display_cell(peer.get("state")),
            display_cell(peer.get("prefixes_received")),
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
        let row = row![
            display_cell(route.get("received_from")),
            afi,
            prefix,
            display_cell(route.get("next_hop")),
            display_cell(route.get("age")),
            display_cell(route.get("origin")),
            display_cell(route.get("local_pref")),
            display_cell(route.get("multi_exit_disc")),
            // TODO, this just displays the first segment as space delimited ASNs
            // Should it display more?
            display_cell(route.get("as_path")),
            display_cell(route.get("communities")),
        ];
        Ok(row)
    }
}
