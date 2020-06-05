use std::sync::Arc;

use bgp_rs::{NLRIEncoding, Segment};

use crate::api::rpc::LearnedRoute;
use crate::rib::ExportEntry;
use crate::utils::{format_time_as_elapsed, u32_to_dotted};

pub fn entry_to_route(entry: Arc<ExportEntry>) -> LearnedRoute {
    let prefix = {
        use NLRIEncoding::*;
        match &entry.update.nlri {
            IP(prefix) => prefix.to_string(),
            FLOWSPEC(filters) => filters
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join("; "),
            nlri => format!("{:?}", nlri),
        }
    };
    LearnedRoute {
        source: entry.source.to_string(),
        afi: entry.update.family.afi.to_string(),
        safi: entry.update.family.safi.to_string(),
        received_at: entry.timestamp.timestamp(),
        age: format_time_as_elapsed(entry.timestamp),
        prefix,
        next_hop: entry.update.attributes.next_hop,
        origin: entry.update.attributes.origin.to_string(),
        as_path: entry
            .update
            .attributes
            .as_path
            .segments
            .iter()
            .map(|segment| {
                let asns = match segment {
                    Segment::AS_SEQUENCE(asns) => asns,
                    Segment::AS_SET(asns) => asns,
                };
                asns.iter()
                    .map(|asn| u32_to_dotted(*asn, '.'))
                    .collect::<Vec<String>>()
                    .join(" ")
            })
            .collect::<Vec<String>>()
            .join("; "),
        local_pref: entry.update.attributes.local_pref,
        multi_exit_disc: entry.update.attributes.multi_exit_disc,
        communities: entry
            .update
            .attributes
            .communities
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    }
}
