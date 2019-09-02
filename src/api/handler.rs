use std::net::IpAddr;
use std::sync::Arc;

use bgp_rs::{ASPath, Origin};
use chrono::Utc;
use http::StatusCode;
use log::error;
use serde_json::json;
use tower_web::error::Error;
use tower_web::*;

use crate::handler::State;
use crate::models::{CommunityList, Route, RouteState};
use crate::utils::prefix_from_string;

use super::peers::{PeerSummaries, PeerSummary};
use super::routes::{AdvertisedRoutes, LearnedRoutes};

pub struct API(pub Arc<State>);

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
            let mut output: Vec<PeerSummary> = vec![];
            self.0.idle_peers.lock().map(|idle_peers| {
                output.extend(idle_peers.peers().iter().map(|&p| p.into()).collect::<Vec<_>>());
            }).map_err(|err| {
                error!("Error fetching peers: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;
            self.0.sessions.lock().map(|sessions| {
                output.extend(sessions.values().map(|s| s.into()).collect::<Vec<_>>());
            }).map_err(|err| {
                error!("Error fetching sessions: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;
            Ok(PeerSummaries(output))
        }

        #[get("/show/routes/learned")]
        #[content_type("json")]
        fn show_routes_learned(&self) -> Result<LearnedRoutes, Error> {
            self.0.learned_routes.lock().map(|routes|{
                Ok(LearnedRoutes(routes.iter().map(|r| r.into()).collect()))
            }).map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?
        }

        #[get("/show/routes/learned/:router_id")]
        #[content_type("json")]
        fn show_routes_learned_for_peer(&self, router_id: String) -> Result<LearnedRoutes, Error> {
            let router_id = router_id.parse::<IpAddr>()
                .map_err(|_err| Error::builder().status(StatusCode::BAD_REQUEST).detail("Invalid Router ID").build())?;
            self.0.learned_routes.lock().map(move |routes| {
                Ok(LearnedRoutes(routes.iter().filter(|r| r.peer == router_id).map(|r| r.into()).collect()))
            }).map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?
        }

        #[get("/show/routes/advertised")]
        #[content_type("json")]
        fn show_routes_advertised(&self) -> Result<AdvertisedRoutes, Error> {
            self.0.advertised_routes.lock().map(|routes|{
                Ok(AdvertisedRoutes(routes.iter().map(|r| r.into()).collect()))
            }).map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?
        }

        #[get("/show/routes/advertised/:router_id")]
        #[content_type("json")]
        fn show_routes_advertised_for_peer(&self, router_id: String) -> Result<AdvertisedRoutes, Error> {
            let router_id = router_id.parse::<IpAddr>()
                .map_err(|_err| Error::builder().status(StatusCode::BAD_REQUEST).detail("Invalid Router ID").build())?;
            self.0.advertised_routes.lock().map(move |routes| {
                Ok(AdvertisedRoutes(routes.iter().filter(|r| r.peer == router_id).map(|r| r.into()).collect()))
            }).map_err(|err| {
                error!("Error fetching routes: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?
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

            self.0.pending_routes.lock().map(|mut p_routes| {
                p_routes.push(route);
            }).map_err(|err| {
                error!("Error adding route: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;
            Ok(json!({ "status": "success", }))
        }
    }
}
