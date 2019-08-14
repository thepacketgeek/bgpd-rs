use futures::future;
use hyper::rt::Future;
use hyper::{Body, Request, Response};
use hyper::{Method, StatusCode};
use log::trace;

use super::peers::PeerSummaries;
use super::routes::{AdvertisedRoutes, LearnedRoutes};
use super::Responder;

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

pub fn handle_api_request(req: Request<Body>) -> BoxFut {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/show/neighbors") => match PeerSummaries::respond() {
            Ok(output) => {
                *response.body_mut() = Body::from(serde_json::to_string(&output).unwrap());
            }
            Err(code) => {
                *response.status_mut() = code;
            }
        },
        (&Method::GET, "/show/routes/learned") => match LearnedRoutes::respond() {
            Ok(output) => {
                *response.body_mut() = Body::from(serde_json::to_string(&output).unwrap());
            }
            Err(code) => {
                *response.status_mut() = code;
            }
        },
        (&Method::GET, "/show/routes/advertised") => match AdvertisedRoutes::respond() {
            Ok(output) => {
                *response.body_mut() = Body::from(serde_json::to_string(&output).unwrap());
            }
            Err(code) => {
                *response.status_mut() = code;
            }
        },
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };
    trace!("{} [{}]", req.uri(), response.status());
    Box::new(future::ok(response))
}
