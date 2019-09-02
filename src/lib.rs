use std::convert::From;
use std::fmt;
use std::io::Error;

use bgp_rs::{Capabilities, Message, Open};

mod api;
mod codec;
mod config;
mod handler;
mod models;
mod poller;
mod session;
mod utils;

pub use api::API;
pub use config::ServerConfig;
pub use handler::Server;

use models::Route;

#[derive(Debug)]
pub enum MessageResponse {
    Open((Open, Capabilities, u16)),
    Message(Message),
    LearnedRoutes(Vec<Route>),
    Empty,
}

#[derive(Debug)]
pub struct SessionError {
    pub reason: String,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Session Error: {}", self.reason)
    }
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        SessionError {
            reason: error.to_string(),
        }
    }
}
