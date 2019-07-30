use std::convert::From;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::marker::PhantomData;
use std::net::IpAddr;
use std::time::Instant;

use bgp_rs::{ASPath, Origin, Prefix, Segment};
use bgpd_lib::db::{PeerStatus, Route};
use bgpd_lib::peer::{Peer, PeerState};
use bgpd_lib::utils::{asn_to_dotted, format_elapsed_time, maybe_string, EMPTY_VALUE};
use chrono::{DateTime, TimeZone, Utc};
use prettytable::{cell, format, row, Row, Table};

use crate::table::{OutputTable, ToRow};

impl ToRow for PeerStatus {
    fn columns() -> Row {
        row!["Neighbor", "AS", "MsgRcvd", "MsgSent", "Uptime", "State", "PfxRcd"]
    }

    fn to_row(&self) -> Row {
        row![
            self.neighbor.to_string(),
            asn_to_dotted(self.asn),
            maybe_string(self.msg_received.as_ref()),
            maybe_string(self.msg_sent.as_ref()),
            if let Some(connect_time) = self.connect_time {
                format_elapsed_time(connect_time.elapsed())
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
        let duration = Utc::now().signed_duration_since(self.received_at);
        let afi = match self.prefix {
            IpAddr::V4(_) => "IPv4",
            IpAddr::V6(_) => "IPv6",
        };
        let elapsed = duration
            .to_std()
            .map(format_elapsed_time)
            .unwrap_or_else(|_| duration.to_string());
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
            elapsed,
            String::from(&self.origin),
            maybe_string(self.local_pref.as_ref()),
            maybe_string(self.multi_exit_disc.as_ref()),
            maybe_string(as_path.as_ref()),
            &self.communities,
        ]
    }
}

impl From<&Peer> for StatusRow {
    fn from(peer: &Peer) -> Self {
        StatusRow {
            neighbor: peer.addr,
            asn: peer.remote_id.asn,
            msg_received: None,
            msg_sent: None,
            connect_time: None,
            state: peer.state,
            prefixes_received: None,
        }
    }
}

// impl ToRow for PeerStatus {
//     fn columns() -> Row {
//         row![
//             "Neighbor",
//             "AFI",
//             "Prefix",
//             "Next Hop",
//             "Age",
//             "Origin",
//             "Local Pref",
//             "Metric",
//             "AS Path",
//             "Communities"
//         ]
//     }

//     fn to_row(&self) -> Row {
//         let duration = Utc::now().signed_duration_since(self.received_at);
//         let afi = match self.prefix {
//             IpAddr::V4(_) => "IPv4",
//             IpAddr::V6(_) => "IPv6",
//         };
//         let elapsed = duration
//             .to_std()
//             .map(format_elapsed_time)
//             .unwrap_or_else(|_| duration.to_string());
//         // TODO, this just displays the first segment as space delimited ASNs
//         // Should it display more?
//         let as_path = match self.as_path.segments.iter().next() {
//             Some(segment) => {
//                 let path = match segment {
//                     Segment::AS_SEQUENCE(sequence) => sequence,
//                     Segment::AS_SET(set) => set,
//                 };
//                 Some(
//                     path.iter()
//                         .map(std::string::ToString::to_string)
//                         .collect::<Vec<String>>()
//                         .join(" "),
//                 )
//             }
//             None => None,
//         };
//         row![
//             self.received_from,
//             afi,
//             self.prefix,
//             self.next_hop,
//             elapsed,
//             String::from(&self.origin),
//             maybe_string(self.local_pref.as_ref()),
//             maybe_string(self.multi_exit_disc.as_ref()),
//             maybe_string(as_path.as_ref()),
//             &self.communities,
//         ]
//     }
// }
