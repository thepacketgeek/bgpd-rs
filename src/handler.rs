use std::collections::HashMap;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use super::codec::{MessageCodec, MessageProtocol};
use super::models::{Peer, PeerIdentifier, PeerState};
use futures::{future::Future, sync::mpsc, Async, Poll, Stream};
use log::{debug, error, warn};
use tokio::net::TcpListener;

use crate::config::ServerConfig;
use crate::models::Route;
use crate::poller::Poller;
use crate::session::{Session, SessionRoutes, SessionRx, SessionTx};

pub struct Server {
    inner: Arc<State>,
    tcp_listener: TcpListener,
    tx: SessionTx,
    rx: SessionRx,
}

pub struct State {
    pub(crate) idle_peers: Arc<Mutex<Poller>>,
    pub(crate) sessions: Arc<Mutex<HashMap<IpAddr, Session>>>,
    pub(crate) learned_routes: Arc<Mutex<Vec<Route>>>,
    pub(crate) pending_routes: Arc<Mutex<Vec<Route>>>,
    pub(crate) advertised_routes: Arc<Mutex<Vec<Route>>>,
}

impl Server {
    pub fn from_config(socket: SocketAddr, config: ServerConfig) -> Result<Self, Error> {
        // Peers are taken by a session when it begins and
        // returned to Poller when the session drops
        let mut poller = Poller::new(15);
        for peer_config in config.peers.iter() {
            let peer = Peer::new(
                peer_config.remote_ip,
                PeerState::Idle,
                PeerIdentifier::new(None, peer_config.remote_as), // remote
                PeerIdentifier::new(
                    Some(peer_config.router_id.unwrap_or(config.router_id)),
                    peer_config.local_as.unwrap_or(config.default_as),
                ), // local
                peer_config.passive,
                peer_config.hold_timer,
                peer_config.dest_port,
            );
            poller.add_peer(peer);
        }
        let (tx, rx) = mpsc::unbounded();

        Ok(Self {
            inner: Arc::new(State {
                idle_peers: Arc::new(Mutex::new(poller)),
                sessions: Arc::new(Mutex::new(HashMap::new())),
                learned_routes: Arc::new(Mutex::new(Vec::new())),
                advertised_routes: Arc::new(Mutex::new(Vec::new())),
                pending_routes: Arc::new(Mutex::new(Vec::new())),
            }),
            tcp_listener: TcpListener::bind(&socket)?,
            tx,
            rx,
        })
    }

    pub fn clone_state(&self) -> Arc<State> {
        self.inner.clone()
    }
}

impl Future for Server {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Process all pending incoming connections
        while let Ok(Async::Ready((stream, socket))) = self.tcp_listener.poll_accept() {
            debug!("Incoming new connection from {}", socket.ip());
            if let Some(peer) = self
                .inner
                .idle_peers
                .lock()
                .unwrap()
                .connect_peer(&socket.ip())
            {
                let protocol = MessageProtocol::new(stream, MessageCodec::new());
                let new_session = Session::new(peer, protocol, self.tx.clone());
                if let Ok(mut sessions) = self.inner.sessions.lock() {
                    sessions.insert(socket.ip(), new_session);
                }
            } else {
                warn!("Unexpected connection from {}", socket.ip());
            }
        }

        let mut ended_sessions: Vec<IpAddr> = vec![];
        if let Ok(mut sessions) = self.inner.sessions.lock() {
            for (addr, session) in sessions.iter_mut() {
                if let Err(session_err) = session.poll() {
                    warn!("Session ended with {}: {}", addr, session_err.reason);
                    ended_sessions.push(*addr);
                }

                if let Ok(mut routes) = self.inner.pending_routes.lock() {
                    if let Some(router_id) = session.peer.remote_id.router_id {
                        let mut pending: Vec<Route> = vec![];
                        // Until vec.drain_filter() hits stable...
                        let mut i = 0;
                        while i != routes.len() {
                            if routes[i].peer == router_id {
                                pending.push(routes.remove(i));
                            } else {
                                i += 1;
                            }
                        }
                        session.add_pending_routes(pending);
                    }
                }
            }
        }
        while let Some(session_addr) = ended_sessions.pop() {
            if let Ok(mut sessions) = self.inner.sessions.lock() {
                if let Some(mut session) = sessions.remove(&session_addr) {
                    let mut peer = session.reset_peer();
                    if let Ok(mut peers) = self.inner.idle_peers.lock() {
                        if let Ok(mut learned_routes) = self.inner.learned_routes.lock() {
                            learned_routes.retain(|r| r.peer != peer.remote_id.router_id.unwrap());
                        }
                        peer.revert_to_idle();
                        peers.add_peer(peer);
                    }
                }
            }
        }

        while let Ok(Async::Ready(Some(routes))) = self.rx.poll() {
            match routes {
                SessionRoutes::Learned(routes) => {
                    debug!("Incoming routes: {}", routes.len());
                    if let Err(err) = self
                        .inner
                        .learned_routes
                        .lock()
                        .map(|mut lr| lr.extend(routes))
                    {
                        error!("Error adding learned routes: {}", err);
                    }
                }
                SessionRoutes::Advertised(routes) => {
                    if let Err(err) = self
                        .inner
                        .advertised_routes
                        .lock()
                        .map(|mut lr| lr.extend(routes))
                    {
                        error!("Error adding advertised routes: {}", err);
                    }
                }
            }
        }

        Ok(Async::NotReady)
    }
}
