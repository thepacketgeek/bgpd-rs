use std::collections::HashMap;
use std::fmt;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use futures::{Async, Poll};
use tokio::prelude::*;
use tokio::timer::Interval;

use crate::models::Peer;

type Peers = HashMap<IpAddr, IdlePeer>;

/// Signal to the Poller when an Idle Peer is ready to attempt connection to
pub struct IdlePeer {
    peer: Peer,
    last_connect_attempt: Instant,
    interval: Duration,
}

impl IdlePeer {
    pub fn new(peer: Peer, interval: Duration) -> Self {
        IdlePeer {
            peer,
            last_connect_attempt: Instant::now(),
            interval,
        }
    }

    /// Consumes self and returns Peer
    pub fn connecting(self) -> Peer {
        self.peer
    }

    pub fn should_init_connection(&self) -> bool {
        !self.peer.is_passive()
            && self.last_connect_attempt.elapsed().as_secs() > self.interval.as_secs()
    }
}

/// Stores Idle peers and checks every interval if there are peers that the Handler
/// can attempt to connect to
pub struct Poller {
    interval: Duration,
    timer: Interval,
    idle_peers: Peers,
}

impl Poller {
    pub fn new(interval: u32 /* seconds */) -> Self {
        Self {
            interval: Duration::from_secs(interval.into()),
            timer: Interval::new(Instant::now(), Duration::from_secs(2)),
            idle_peers: HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, peer: Peer) {
        self.idle_peers
            .insert(peer.addr, IdlePeer::new(peer, self.interval));
    }

    pub fn connect_peer(&mut self, addr: &IpAddr) -> Option<Peer> {
        if let Some(idle_peer) = self.idle_peers.remove(&addr) {
            Some(idle_peer.connecting())
        } else {
            None
        }
    }

    pub fn peers(&self) -> Vec<&Peer> {
        self.idle_peers.values().map(|p| &p.peer).collect()
    }
}

impl Stream for Poller {
    type Item = SocketAddr;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Error> {
        while let Ok(Async::Ready(_)) = self.timer.poll() {
            for idle in self
                .idle_peers
                .values_mut()
                .filter(|p| p.should_init_connection())
            {
                idle.last_connect_attempt = Instant::now();
                return Ok(Async::Ready(Some(
                    (idle.peer.addr, idle.peer.dest_port).into(),
                )));
            }
        }
        Ok(Async::NotReady)
    }
}

impl fmt::Display for Poller {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<Poller peers={}>", self.idle_peers.len())
    }
}
