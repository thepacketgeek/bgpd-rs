use hyper::StatusCode;

mod handler;
mod peers;
mod routes;

pub use handler::handle_api_request;

trait Responder {
    type Item;

    fn respond() -> Result<Self::Item, StatusCode>;
}
