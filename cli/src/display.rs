use std::convert::From;
use std::net::IpAddr;

use bgp_rs::Segment;
use bgpd_lib::models::{PeerSummary, Route};
use bgpd_lib::utils::{asn_to_dotted, format_time_as_elapsed, maybe_string, EMPTY_VALUE};
use prettytable::{cell, row, Row};

use crate::table::ToRow;

impl ToRow for &PeerSummary {
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

    fn to_row(&self) -> Row {
        row![
            self.neighbor.to_string(),
            maybe_string(self.router_id.as_ref()),
            asn_to_dotted(self.asn),
            maybe_string(self.msg_received.as_ref()),
            maybe_string(self.msg_sent.as_ref()),
            if let Some(connect_time) = self.connect_time {
                format_time_as_elapsed(connect_time)
            } else {
                String::from(EMPTY_VALUE)
            },
            self.state.to_string(),
            maybe_string(self.prefixes_received.as_ref()),
        ]
    }
}

impl ToRow for Route {
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

    fn to_row(&self) -> Row {
        let afi = match self.prefix {
            IpAddr::V4(_) => "IPv4",
            IpAddr::V6(_) => "IPv6",
        };
        // TODO, this just displays the first segment as space delimited ASNs
        // Should it display more?
        let as_path = match self.as_path.segments.iter().next() {
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
        };
        row![
            self.received_from,
            afi,
            self.prefix,
            self.next_hop,
            format_time_as_elapsed(self.received_at),
            String::from(&self.origin),
            maybe_string(self.local_pref.as_ref()),
            maybe_string(self.multi_exit_disc.as_ref()),
            maybe_string(as_path.as_ref()),
            &self.communities,
        ]
    }
}
