use std::collections::HashMap;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::codec::{MessageCodec, MessageProtocol};
use super::models::{Peer, PeerIdentifier, PeerState, PeerSummary};
use bgp_rs::Message;
use futures::future::{self, Either, Future};
use futures::sync::mpsc;
use futures::{Async, Poll, Stream};
use log::{debug, error, info, trace, warn};
use net2::TcpBuilder;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::reactor::Handle;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use crate::config::ServerConfig;
// use crate::db::DB;
use crate::models::Route;
use crate::session::{Session, SessionMessage};

type Peers = HashMap<IpAddr, Peer>;

type SessionTx = mpsc::UnboundedSender<SessionMessage>;
type SessionRx = mpsc::UnboundedReceiver<SessionMessage>;

pub struct Server {
    tcp_listener: TcpListener,
    idle_peers: Arc<Mutex<Peers>>,
    sessions: Arc<Mutex<HashMap<SocketAddr, SessionTx>>>,
    // learned_routes: Arc<Mutex<Vec<Route>>>,
    // advertised_routes: Arc<Mutex<Vec<Route>>>,
    // api_channel: Tx,
}

impl Server {
    pub fn from_config(socket: SocketAddr, config: ServerConfig) -> Result<Self, Error> {
        // Peers are owned by a session when it begins and
        // returned to idle_peers when the session drops
        let idle_peers: Peers = config
            .peers
            .iter()
            .map(|p| {
                let peer = Peer::new(
                    p.remote_ip,
                    PeerState::Idle,
                    PeerIdentifier::new(None, p.remote_as), // remote
                    PeerIdentifier::new(
                        Some(p.router_id.unwrap_or(config.router_id)),
                        p.local_as.unwrap_or(config.default_as),
                    ), // local
                    p.passive,
                    p.hold_timer,
                );
                (peer.addr, peer)
            })
            .collect();

        Ok(Self {
            tcp_listener: TcpListener::bind(&socket)?,
            idle_peers: Arc::new(Mutex::new(idle_peers)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            // learned_routes: Arc::new(Mutex::new(Vec::new())),
            // advertised_routes: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

impl Future for Server {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // TCP Listener task
        while let Ok(Async::Ready((stream, socket))) = self.tcp_listener.poll_accept() {
            debug!("Incoming new connection from {}", socket.ip());
            if let Some(mut peer) = self.idle_peers.lock().unwrap().remove(&socket.ip()) {
                peer.update_state(PeerState::OpenSent);

                let peer_insert = {
                    let peers = self.idle_peers.clone();
                    move |peer: Option<Peer>| {
                        if let Some(peer) = peer {
                            debug!("Adding {} back to idle peers", peer);
                            peers
                                .lock()
                                .map(|mut peers| {
                                    peers.insert(peer.addr, peer);
                                })
                                .ok();
                        }
                    }
                };

                let session = MessageProtocol::new(stream, MessageCodec::new())
                    .into_future()
                    .map_err(|(e, _)| e.into())
                    .and_then(move |(open, protocol)| {
                        if let Some(Message::Open(open)) = open {
                            let (updated_protocol, hold_timer) = peer.open_received(open, protocol);
                            let new_session = Session::new(peer, updated_protocol, hold_timer);
                            return Either::B(Some(new_session));
                        } else {
                            warn!("Invalid first packet received");
                        }
                        Either::A(future::ok(None))
                    })
                    .map(peer_insert)
                    .map_err(|e| error!("{}", e));
                tokio::spawn(session);
            } else {
                warn!("Unexpected connection from {}", socket.ip());
            }
        }
        Ok(Async::NotReady)
    }
}
