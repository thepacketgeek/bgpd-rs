use hyper::{Body, Request, StatusCode};

mod handler;
mod peers;
mod routes;

pub use handler::api_router_service;

trait Responder {
    type Item;

    fn respond(req: Request<Body>) -> Result<Self::Item, StatusCode>;
}
