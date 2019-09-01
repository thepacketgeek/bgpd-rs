use std::collections::HashMap;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::codec::{MessageCodec, MessageProtocol};
use super::models::{Peer, PeerIdentifier, PeerState};
use bgp_rs::Message;
use futures::future::{self, Either, Future};
use futures::sync::mpsc;
use futures::{Async, Poll, Stream};
use log::{debug, error, info, trace, warn};
use net2::TcpBuilder;
use tokio::net::{TcpListener, TcpStream};

use crate::config::ServerConfig;
use crate::models::Route;
use crate::session::{Session, SessionRoutes, SessionRx};

type Peers = HashMap<IpAddr, Peer>;

type ActiveSession = (Session, SessionRx);

pub struct Server {
    inner: Arc<State>,
    tcp_listener: TcpListener,
}

pub struct State {
    pub(crate) idle_peers: Arc<Mutex<Peers>>,
    pub(crate) sessions: Arc<Mutex<HashMap<IpAddr, ActiveSession>>>,
    pub(crate) learned_routes: Arc<Mutex<Vec<Route>>>,
    pub(crate) pending_routes: Arc<Mutex<Vec<Route>>>,
    pub(crate) advertised_routes: Arc<Mutex<Vec<Route>>>,
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
            inner: Arc::new(State {
                idle_peers: Arc::new(Mutex::new(idle_peers)),
                sessions: Arc::new(Mutex::new(HashMap::new())),
                learned_routes: Arc::new(Mutex::new(Vec::new())),
                advertised_routes: Arc::new(Mutex::new(Vec::new())),
                pending_routes: Arc::new(Mutex::new(Vec::new())),
            }),
            tcp_listener: TcpListener::bind(&socket)?,
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
            if let Some(peer) = self.inner.idle_peers.lock().unwrap().remove(&socket.ip()) {
                let (tx, rx) = mpsc::unbounded();
                let protocol = MessageProtocol::new(stream, MessageCodec::new());
                let new_session = Session::new(peer, protocol, tx);
                if let Ok(mut sessions) = self.inner.sessions.lock() {
                    sessions.insert(socket.ip(), (new_session, rx));
                }
            } else {
                warn!("Unexpected connection from {}", socket.ip());
            }
        }

        if let Ok(mut sessions) = self.inner.sessions.lock() {
            for (addr, (session, rx)) in sessions.iter_mut() {
                match session.poll() {
                    Ok(Async::Ready(routes)) => {
                        eprintln!("Received {} routes", routes.len());
                    }
                    Err(session_err) => {
                        warn!("Session ended with {}: {}", addr, session_err.reason);
                        if let Ok(mut sessions) = self.inner.sessions.lock() {
                            if let Some((mut session, _)) = sessions.remove(&addr) {
                                let mut peer = session.reset_peer();
                                if let Ok(mut peers) = self.inner.idle_peers.lock() {
                                    peer.revert_to_idle();
                                    peers.insert(peer.addr, peer);
                                }
                            }
                        }
                    }
                    _ => (),
                }
                while let Ok(Async::Ready(Some(routes))) = rx.poll() {
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

        eprintln!("--");
        Ok(Async::NotReady)
    }
}
