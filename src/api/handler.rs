use std::net::IpAddr;

use bgp_rs::{ASPath, Origin};
use chrono::Utc;
use http::StatusCode;
use log::error;
use serde_json::json;
use tower_web::error::Error;
use tower_web::*;

use crate::db::DB;
use crate::models::{CommunityList, Route, RouteState};
use crate::utils::{format_time_as_elapsed, prefix_from_string};

use super::peers::{PeerSummaries, PeerSummary};
use super::routes::{AdvertisedRoutes, LearnedRoutes};

#[derive(Clone, Debug)]
pub struct API;

#[derive(Debug, Extract)]
struct AdvertisePrefixData {
    router_id: String,
    prefix: String,
    next_hop: Option<String>,
    med: Option<u32>,
}

impl_web! {
    impl API {
        #[get("/show/neighbors")]
        #[content_type("json")]
        fn show_neighbors(&self) -> Result<PeerSummaries, Error> {
            let peers = DB::new().and_then(|db| db.get_all_peers()).map_err(|err| {
                error!("Error fetching all peers: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;
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
            Ok(PeerSummaries(output))
        }

        #[get("/show/routes/learned")]
        #[content_type("json")]
        fn show_routes_learned(&self) -> Result<LearnedRoutes, Error> {
            let query = DB::new().and_then(|db| db.get_all_received_routes());
            let routes = query.map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;

            Ok(LearnedRoutes::from_db(routes))
        }

        #[get("/show/routes/learned/:router_id")]
        #[content_type("json")]
        fn show_routes_learned_for_peer(&self, router_id: String) -> Result<LearnedRoutes, Error> {
            let router_id = router_id.parse::<IpAddr>()
                .map_err(|_err| Error::builder().status(StatusCode::BAD_REQUEST).detail("Invalid Router ID").build())?;
            let query = DB::new().and_then(|db| db.get_received_routes_for_peer(router_id));

            let routes = query.map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;

            Ok(LearnedRoutes::from_db(routes))
        }

        #[get("/show/routes/advertised")]
        #[content_type("json")]
        fn show_routes_advertised(&self) -> Result<AdvertisedRoutes, Error> {
            let query = DB::new().and_then(|db| db.get_all_advertised_routes());
            let routes = query.map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;

            Ok(AdvertisedRoutes::from_db(routes))
        }

        #[get("/show/routes/advertised/:router_id")]
        #[content_type("json")]
        fn show_routes_advertised_for_peer(&self, router_id: String) -> Result<AdvertisedRoutes, Error> {
            let router_id = router_id.parse::<IpAddr>()
                .map_err(|_err| Error::builder().status(StatusCode::BAD_REQUEST).detail("Invalid Router ID").build())?;
            let query = DB::new().and_then(|db| db.get_advertised_routes_for_peer(router_id));

            let routes = query.map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;

            Ok(AdvertisedRoutes::from_db(routes))
        }

        #[post("/advertise/prefix")]
        #[content_type("json")]
        fn advertise_prefix_to_peer(&self, body: AdvertisePrefixData) -> Result<serde_json::Value, Error> {
            let router_id = &body.router_id.parse::<IpAddr>()
                .map_err(|_err| Error::new("Bad Request", "Invalid Router ID", StatusCode::BAD_REQUEST))?;
            let prefix = prefix_from_string(&body.prefix).map_err(|_err|  Error::new("Bad Request", "Invalid Prefix", StatusCode::BAD_REQUEST))?;
            let next_hop = match body.next_hop {
                Some(next_hop) => next_hop.parse::<IpAddr>()
                    .map_err(|_err| Error::new("Bad Request", "Invalid Next Hop", StatusCode::BAD_REQUEST))?,
                None => "0.0.0.0".parse::<IpAddr>().unwrap(),
            };

            let route = Route {
                peer: *router_id,
                state: RouteState::Pending(Utc::now()),
                prefix,
                next_hop,
                origin: Origin::INCOMPLETE,
                as_path: ASPath { segments: vec![] },
                local_pref: None,
                multi_exit_disc: body.med,
                communities: CommunityList(vec![]),
            };

            DB::new().and_then(|db| db.insert_routes(vec![route])).map_err(|_err| Error::new("Internal Server Error", "", StatusCode::INTERNAL_SERVER_ERROR))?;
            Ok(json!({ "status": "success", }))
        }
    }
}
