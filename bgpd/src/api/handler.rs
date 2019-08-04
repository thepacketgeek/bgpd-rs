use futures::future;
use hyper::{Body, Request, Response};
use hyper::rt::Future;
use hyper::{Method, StatusCode};
use serde::{Serialize, Deserialize};

use bgpd_lib::db::DB;
use log::trace;
// use bgpd_lib::peer::PeerState;

type BoxFut = Box<dyn Future<Item=Response<Body>, Error=hyper::Error> + Send>;


pub fn handle_api_request(req: Request<Body>) -> BoxFut {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/show/neighbors") => {
            let peers = DB::new().and_then(|db| db.get_all_peers()).unwrap();
            *response.body_mut() = Body::from(serde_json::to_string(&peers).unwrap());
        },
        (&Method::GET, "/show/routes/learned") => {
            let routes = DB::new()
                .and_then(|db| db.get_all_routes()).unwrap();
            *response.body_mut() = Body::from(serde_json::to_string(&routes).unwrap());
        },
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        },
    };
    trace!("{} [{}]", req.uri(), response.status());
    Box::new(future::ok(response))
}