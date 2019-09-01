use std::convert::From;
use std::net::IpAddr;

use bgp_rs::Segment;
use serde::Serialize;
use tower_web::Response;

use crate::models::{Route, RouteState};
use crate::utils::format_time_as_elapsed;

#[derive(Serialize)]
pub struct LearnedRoute {
    received_from: IpAddr,
    received_at: i64,
    age: String,
    prefix: String,
    next_hop: IpAddr,
    origin: String,
    as_path: String,
    local_pref: Option<u32>,
    multi_exit_disc: Option<u32>,
    communities: Vec<String>,
}

impl From<&Route> for LearnedRoute {
    fn from(route: &Route) -> Self {
        let received_at = match route.state {
            RouteState::Received(timestamp) => timestamp,
            _ => unreachable!(),
        };
        LearnedRoute {
            received_from: route.peer,
            received_at: received_at.timestamp(),
            age: format_time_as_elapsed(received_at),
            prefix: route.prefix.to_string(),
            next_hop: route.next_hop,
            origin: String::from(&route.origin),
            as_path: route
                .as_path
                .segments
                .iter()
                .map(|segment| {
                    let asns = match segment {
                        Segment::AS_SEQUENCE(asns) => asns,
                        Segment::AS_SET(asns) => asns,
                    };
                    asns.iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(" ")
                })
                .collect::<Vec<String>>()
                .join("; "),
            local_pref: route.local_pref,
            multi_exit_disc: route.multi_exit_disc,
            communities: route
                .communities
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }
}

#[derive(Response)]
pub struct LearnedRoutes(pub Vec<LearnedRoute>);

#[derive(Serialize)]
pub struct AdvertisedRoute {
    sent_to: IpAddr,
    sent_at: i64,
    age: String,
    prefix: String,
    next_hop: IpAddr,
    origin: String,
    as_path: String,
    local_pref: Option<u32>,
    multi_exit_disc: Option<u32>,
    communities: Vec<String>,
}

impl From<&Route> for AdvertisedRoute {
    fn from(route: &Route) -> Self {
        let sent_at = match route.state {
            RouteState::Advertised(timestamp) => timestamp,
            _ => unreachable!(),
        };
        AdvertisedRoute {
            sent_to: route.peer,
            sent_at: sent_at.timestamp(),
            age: format_time_as_elapsed(sent_at),
            prefix: route.prefix.to_string(),
            next_hop: route.next_hop,
            origin: String::from(&route.origin),
            as_path: route
                .as_path
                .segments
                .iter()
                .map(|segment| {
                    let asns = match segment {
                        Segment::AS_SEQUENCE(asns) => asns,
                        Segment::AS_SET(asns) => asns,
                    };
                    asns.iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(" ")
                })
                .collect::<Vec<String>>()
                .join("; "),
            local_pref: route.local_pref,
            multi_exit_disc: route.multi_exit_disc,
            communities: route
                .communities
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }
}

#[derive(Response)]
pub struct AdvertisedRoutes(pub Vec<AdvertisedRoute>);
