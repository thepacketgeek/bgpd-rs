use std::net::IpAddr;

use bgp_rs::Segment;
use hyper::{Body, Request, StatusCode};
use log::error;
use serde::Serialize;

use crate::db::DB;
use crate::models::RouteState;
use crate::utils::format_time_as_elapsed;

use super::Responder;

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

#[derive(Serialize)]
pub struct LearnedRoutes(Vec<LearnedRoute>);

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

#[derive(Serialize)]
pub struct AdvertisedRoutes(Vec<AdvertisedRoute>);

impl Responder for LearnedRoutes {
    type Item = LearnedRoutes;

    fn respond(req: Request<Body>) -> Result<Self::Item, StatusCode> {
        let parts: Vec<&str> = req
            .uri()
            .path()
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();

        let mut query;
        if let Some(Ok(peer)) = parts.get(3).map(|p| p.parse::<IpAddr>()) {
            query = DB::new().and_then(|db| db.get_received_routes_for_peer(peer));
        } else {
            query = DB::new().and_then(|db| db.get_all_received_routes());
        }

        let routes = query.map_err(|err| {
            error!("Error fetching routes: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let output: Vec<LearnedRoute> = routes
            .iter()
            .map(|route| {
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
            })
            .collect();
        Ok(LearnedRoutes(output))
    }
}

impl Responder for AdvertisedRoutes {
    type Item = AdvertisedRoutes;

    fn respond(req: Request<Body>) -> Result<Self::Item, StatusCode> {
        let parts: Vec<&str> = req
            .uri()
            .path()
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();

        let mut query;
        if let Some(Ok(peer)) = parts.get(3).map(|p| p.parse::<IpAddr>()) {
            query = DB::new().and_then(|db| db.get_advertised_routes_for_peer(peer));
        } else {
            query = DB::new().and_then(|db| db.get_all_advertised_routes());
        }

        let routes = query.map_err(|err| {
            error!("Error fetching routes: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let output: Vec<AdvertisedRoute> = routes
            .iter()
            .map(|route| {
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
            })
            .collect();
        Ok(AdvertisedRoutes(output))
    }
}
