use std::net::IpAddr;

use http::StatusCode;
use log::error;
use tower_web::error::Error;
use tower_web::*;

use crate::db::DB;
use crate::utils::format_time_as_elapsed;

use super::peers::{PeerSummaries, PeerSummary};
use super::routes::{AdvertisedRoutes, LearnedRoutes};

#[derive(Clone, Debug)]
pub struct API;

#[derive(Debug, Extract)]
struct AdvertisePrefixData {
    prefix: String,
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
                error!("Error fetching routess: {}", err);
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
                error!("Error fetching routess: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;

            Ok(LearnedRoutes::from_db(routes))
        }

        #[get("/show/routes/advertised")]
        #[content_type("json")]
        fn show_routes_advertised(&self) -> Result<AdvertisedRoutes, Error> {
            let query = DB::new().and_then(|db| db.get_all_advertised_routes());
            let routes = query.map_err(|err| {
                error!("Error fetching routess: {}", err);
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
                error!("Error fetching routess: {}", err);
                Error::from(StatusCode::INTERNAL_SERVER_ERROR)
            })?;

            Ok(AdvertisedRoutes::from_db(routes))
        }
    }
}
