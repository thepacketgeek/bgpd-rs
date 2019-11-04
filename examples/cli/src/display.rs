use std::convert::From;
use std::error::Error;
use std::fmt::Display;

use bgpd_rpc_lib::{LearnedRoute, PeerSummary};
use prettytable::{cell, row, Row};

use crate::table::ToRow;

pub const EMPTY_VALUE: &str = "";

pub fn display_cell<T>(value: Option<&T>) -> String
where
    T: Display,
{
    match value {
        None => String::from(EMPTY_VALUE),
        Some(v) => v.to_string(),
    }
}

pub struct PeerSummaryRow(pub PeerSummary);

impl ToRow for PeerSummaryRow {
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

    fn to_row(&self) -> Result<Row, Box<dyn Error>> {
        let peer = &self.0;
        let peer_display = if self.0.enabled {
            peer.peer.to_string()
        } else {
            format!("*{}", peer.peer)
        };
        let row = row![
            peer_display,
            display_cell(peer.router_id.as_ref()),
            peer.remote_asn.to_string(),
            display_cell(peer.msg_received.as_ref()),
            display_cell(peer.msg_sent.as_ref()),
            display_cell(peer.uptime.as_ref()),
            peer.state.to_string(),
            display_cell(peer.prefixes_received.as_ref()),
        ];
        Ok(row)
    }
}

pub struct LearnedRouteRow(pub LearnedRoute);

impl ToRow for LearnedRouteRow {
    fn columns() -> Row {
        row![
            "Received From",
            "Prefix",
            "Next Hop",
            "Age",
            "Origin",
            "Local Pref",
            "Metric",
            "AS Path",
            "Communities",
            "Age",
        ]
    }

    fn to_row(&self) -> Result<Row, Box<dyn Error>> {
        let route = &self.0;
        let row = row![
            route.source,
            route.prefix,
            display_cell(route.next_hop.as_ref()),
            route.age,
            route.origin,
            display_cell(route.local_pref.as_ref()),
            display_cell(route.multi_exit_disc.as_ref()),
            route.as_path,
            route.communities.join(" "),
            route.age,
        ];
        Ok(row)
    }
}

pub struct AdvertisedRouteRow(pub LearnedRoute);

impl ToRow for AdvertisedRouteRow {
    fn columns() -> Row {
        row![
            "Advertised To",
            "Prefix",
            "Next Hop",
            "Age",
            "Origin",
            "Local Pref",
            "Metric",
            "AS Path",
            "Communities",
            "Age",
        ]
    }

    fn to_row(&self) -> Result<Row, Box<dyn Error>> {
        let route = &self.0;
        let row = row![
            route.source,
            route.prefix,
            display_cell(route.next_hop.as_ref()),
            route.age,
            route.origin,
            display_cell(route.local_pref.as_ref()),
            display_cell(route.multi_exit_disc.as_ref()),
            route.as_path,
            route.communities.join(" "),
            route.age,
        ];
        Ok(row)
    }
}
