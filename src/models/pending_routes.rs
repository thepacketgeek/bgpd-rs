use std::fmt;
use std::io::Error;

use futures::{Async, Poll};
use tokio::prelude::*;

use crate::models::Route;

pub struct PendingRoutes(pub Vec<Route>);

impl PendingRoutes {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, route: Route) {
        self.0.push(route);
    }
}

impl Stream for PendingRoutes {
    type Item = Route;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Error> {
        if let Some(route) = self.0.pop() {
            Ok(Async::Ready(Some(route)))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl fmt::Display for PendingRoutes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PendingRoutes({})", self.0.len())
    }
}
