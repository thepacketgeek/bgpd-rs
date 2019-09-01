use std::fmt;
use std::io::Error;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use futures::{Async, Poll};
use tokio::prelude::*;

use crate::MessageProtocol;
use crate::models::Peer;
use crate::utils::{format_elapsed_time, get_elapsed_time};


struct PeerStatus {
    peer: Peer,
    last_connect_attempt: Instant,
}

impl PeerStatus {
    fn new(peer: Peer) -> Self {
        PeerStatus {peer, last_connect_attempt: Instant::now()}
    }

    pub fn should_init_connection(&self, interval: Duration) -> bool {
        self.last_connect_attempt.elapsed() > interval
    }
}

impl Future for PeerStatus {
    type Item = IpAddr;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Error> {
        if !self.peer.is_passive() && self.should_init_connection() {
            Ok(Async::Ready(self.peer.addr)
        } else {
            Ok(Async::NotReady)
        }
    }
}

pub struct SessionStarter {
    interval: Duration,
    peers: Vec<PeerStatus>,
    socket: TcpStream,
}

impl SessionStarter {
    pub fn new(socket: TcpStream, interval: u32 /* seconds */) -> Self {
        Self {
            interval: Duration::from_secs(interval),
            peers: vec![],
            socket,
        }
    }

    pub fn add_peer(&mut self, peer: Peer) {
        self.peers.push(PeerStatus::new(peer));
    }
}

impl Future for SessionStarter {
    type Item = MessageProtocol;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Error> {
        for peer_state in self.peers {
            if let Ok(Async::Ready(addr)) = peer_state.poll() {
                let socket = self.builder.
                TcpStream::connect_std(
                    socket,
                    &SocketAddr::new(addr, peer_state.peer.dest_port),
                    &Handle::default(),
                )
                .timeout(Duration::from_secs(2))
            }
        }
    }
}

impl fmt::Display for HoldTimer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_elapsed_time(self.get_hold_time()))
    }
}
