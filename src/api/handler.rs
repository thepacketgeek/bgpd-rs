use hyper::StatusCode;
use hyper::{Body, Response};
use hyper_router::{Route, RouterBuilder, RouterService};
// use log::trace;
use serde::Serialize;

use super::peers::PeerSummaries;
use super::routes::{AdvertisedRoutes, LearnedRoutes};
use super::Responder;

pub fn api_router_service() -> Result<RouterService, std::io::Error> {
    let router = RouterBuilder::new()
        .add(
            Route::get("/show/neighbors")
                .using(|req| get_response_for(PeerSummaries::respond(req))),
        )
        .add(
            Route::get("/show/routes/learned")
                .using(|req| get_response_for(LearnedRoutes::respond(req))),
        )
        .add(
            Route::get("/show/routes/advertised")
                .using(|req| get_response_for(AdvertisedRoutes::respond(req))),
        )
        .build();

    Ok(RouterService::new(router))
}

fn get_response_for<T>(result: Result<T, StatusCode>) -> Response<Body>
where
    T: Serialize,
{
    let mut response = Response::new(Body::empty());
    match result {
        Ok(output) => {
            *response.body_mut() = Body::from(serde_json::to_string(&output).unwrap());
        }
        Err(code) => {
            *response.status_mut() = code;
        }
    }
    // trace!("{} [{}]", req.uri(), response.status());
    response
}
