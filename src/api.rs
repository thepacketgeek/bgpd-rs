use std::net::IpAddr;

use bgp_rs::Segment;
use futures::future;
use hyper::rt::Future;
use hyper::{Body, Request, Response};
use hyper::{Method, StatusCode};
use log::{error, trace};
use serde::Serialize;

use crate::db::DB;
use crate::models::RouteState;
use crate::utils::format_time_as_elapsed;

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

#[derive(Serialize)]
struct PeerSummary {
    peer: IpAddr,
    router_id: Option<IpAddr>,
    asn: u32,
    msg_received: Option<u64>,
    msg_sent: Option<u64>,
    connect_time: Option<i64>,
    uptime: Option<String>,
    state: String,
    prefixes_received: Option<u64>,
}

#[derive(Serialize)]
struct PeerSummaries(Vec<PeerSummary>);

#[derive(Serialize)]
struct LearnedRoute {
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
struct LearnedRoutes(Vec<LearnedRoute>);

#[derive(Serialize)]
struct AdvertisedRoute {
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
struct AdvertisedRoutes(Vec<AdvertisedRoute>);

pub fn handle_api_request(req: Request<Body>) -> BoxFut {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/show/neighbors") => match DB::new().and_then(|db| db.get_all_peers()) {
            Ok(peers) => {
                let output: Vec<PeerSummary> = peers
                    .iter()
                    .map(|peer| PeerSummary {
                        peer: peer.neighbor,
                        router_id: peer.router_id,
                        asn: peer.asn,
                        msg_received: peer.msg_received,
                        msg_sent: peer.msg_sent,
                        connect_time: peer.connect_time.map(|time| time.timestamp()),
                        uptime: peer.connect_time.map(format_time_as_elapsed),
                        state: peer.state.to_string(),
                        prefixes_received: peer.prefixes_received,
                    })
                    .collect();
                *response.body_mut() =
                    Body::from(serde_json::to_string(&PeerSummaries(output)).unwrap());
            }
            Err(err) => {
                error!("{}", err);
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            }
        },
        (&Method::GET, "/show/routes/learned") => {
            match DB::new().and_then(|db| db.get_all_received_routes()) {
                Ok(routes) => {
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
                                    .map(|c| c.to_string())
                                    .collect(),
                            }
                        })
                        .collect();
                    *response.body_mut() =
                        Body::from(serde_json::to_string(&LearnedRoutes(output)).unwrap());
                }
                Err(err) => {
                    error!("{}", err);
                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                }
            }
        }
        (&Method::GET, "/show/routes/advertised") => {
            match DB::new().and_then(|db| db.get_all_advertised_routes()) {
                Ok(routes) => {
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
                                    .map(|c| c.to_string())
                                    .collect(),
                            }
                        })
                        .collect();
                    *response.body_mut() =
                        Body::from(serde_json::to_string(&AdvertisedRoutes(output)).unwrap());
                }
                Err(err) => {
                    error!("{}", err);
                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                }
            }
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };
    trace!("{} [{}]", req.uri(), response.status());
    Box::new(future::ok(response))
}
