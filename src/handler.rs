use std::collections::HashMap;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::codec::{MessageCodec, MessageProtocol};
use super::models::{Peer, PeerIdentifier, PeerState, PeerSummary};
use bgp_rs::Message;
use futures::future::{self, Either, Future};
use log::{debug, error, info, trace, warn};
use net2::TcpBuilder;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::reactor::Handle;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use crate::config::ServerConfig;
use crate::db::DB;
use crate::session::Session;

type Peers = HashMap<IpAddr, Peer>;

fn update_peer(peer: &Peer) -> Result<(), String> {
    DB::new()
        .and_then(|db| {
            db.update_peer(&PeerSummary::new(
                peer.addr,
                peer.remote_id.asn,
                peer.get_state(),
            ))
        })
        .map_err(|err| format!("{}", err))
}

/// Receives a TcpStream from either an incoming connection or active polling,
/// and processes the OPEN message for the correct peer (if configured)
fn handle_new_connection(stream: TcpStream, peers: Arc<Mutex<Peers>>) {
    let peer_insert = {
        let peers = peers.clone();
        move |peer: Option<Peer>| {
            if let Some(peer) = peer {
                debug!("Adding {} back to idle peers", peer);
                update_peer(&peer).ok();
                peers
                    .lock()
                    .map(|mut peers| {
                        peers.insert(peer.addr, peer);
                    })
                    .ok();
            }
        }
    };

    let connection = MessageProtocol::new(stream, MessageCodec::new())
        .into_future()
        .map_err(|(e, _)| e.into())
        .and_then(move |(open, protocol)| {
            let peer_addr = protocol.get_ref().peer_addr().unwrap().ip();
            if let Some(mut peer) = peers.lock().unwrap().remove(&peer_addr) {
                peer.update_state(PeerState::OpenSent);
                if let Some(Message::Open(open)) = open {
                    let (updated_protocol, hold_timer) = peer.open_received(open, protocol);
                    let new_session = Session::new(peer, updated_protocol, hold_timer);
                    return Either::B(Some(new_session));
                } else {
                    warn!("Invalid first packet received");
                }
            } else {
                warn!("Unexpected connection from {}", peer_addr,);
            }
            Either::A(future::ok(None))
        })
        .map(peer_insert)
        .map_err(|e| error!("{}", e));
    tokio::spawn(connection);
}

fn connect_to_peer(peer: IpAddr, source_addr: IpAddr, dest_port: u16, peers: Arc<Mutex<Peers>>) {
    if let Ok(mut peers) = peers.lock() {
        if let Some(peer) = peers.get_mut(&peer) {
            peer.update_state(PeerState::Active);
            update_peer(&peer).ok();
        }
    }
    let builder = match peer {
        IpAddr::V4(_) => TcpBuilder::new_v4().unwrap(),
        IpAddr::V6(_) => TcpBuilder::new_v6().unwrap(),
    };
    let socket = builder
        .reuse_address(true)
        .and_then(|b| b.bind(&SocketAddr::new(source_addr, 0)))
        .and_then(TcpBuilder::to_tcp_stream)
        .unwrap();

    let connect = {
        let handle_error = {
            let peers = peers.clone();
            move |err| {
                trace!("Initiating BGP Session with {} failed: {:?}", peer, err);
                if let Ok(mut peers) = peers.lock() {
                    if let Some(peer) = peers.get_mut(&peer) {
                        peer.update_state(PeerState::Idle);
                        update_peer(&peer).ok();
                    }
                }
            }
        };

        TcpStream::connect_std(
            socket,
            &SocketAddr::new(peer, dest_port),
            &Handle::default(),
        )
        .timeout(Duration::from_secs(2))
        .and_then(move |stream| {
            trace!(
                "Attempting connection to peer: {} [from {}]",
                peer,
                stream.local_addr().unwrap(),
            );
            if let Ok(mut peers) = peers.lock() {
                if let Some(peer) = peers.get_mut(&peer) {
                    peer.update_state(PeerState::Connect);
                    update_peer(&peer).ok();
                }
            }
            handle_new_connection(stream, peers.clone());
            Ok(())
        })
        .map_err(handle_error)
    };
    tokio::spawn(connect);
}

pub fn serve(
    addr: IpAddr,
    port: u16,
    config: ServerConfig,
    mut runtime: Runtime,
) -> Result<Runtime, Error> {
    let socket = SocketAddr::from((addr, port));
    let listener = TcpListener::bind(&socket)?;

    // Peers are owned by a session when it begins
    // to be returned via Channel when the session drops
    let peers: Peers = config
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
            update_peer(&peer).ok();
            (peer.addr, peer)
        })
        .collect();

    let peers: Arc<Mutex<Peers>> = Arc::new(Mutex::new(peers));

    // TCP Listener task
    let server = {
        let peers = peers.clone();
        listener
            .incoming()
            .for_each(move |stream: TcpStream| {
                debug!(
                    "Incoming new connection from {}",
                    stream.peer_addr().unwrap()
                );
                handle_new_connection(stream, peers.clone());
                Ok(())
            })
            .map_err(|err| error!("Incoming connection failed: {}", err))
    };
    info!("Starting BGP server on {}...", socket);
    runtime.spawn(server);

    // TCP Connections outbound
    // Attempt to connect to configured & idle peers
    let connect = {
        let peers = peers.clone();
        Interval::new(
            Instant::now() + Duration::from_secs(10), // Initial delay
            Duration::from_secs(15),                  // Interval
        )
        .for_each(move |_| {
            let idle_peers: Vec<IpAddr> = peers
                .lock()
                .map(|peers| {
                    peers
                        .iter()
                        // Non-passive peers only
                        .filter(|(_, p)| !p.is_passive())
                        // Peers accessible over same IP version this server is bound to
                        .filter(|(_, p)| addr.is_ipv4() == p.addr.is_ipv4())
                        .map(|(_, p)| p.addr)
                        .collect()
                })
                .unwrap_or_else(|_| vec![]);
            for peer_addr in idle_peers {
                connect_to_peer(peer_addr, addr, port, peers.clone());
            }
            Ok(())
        })
        .map_err(|e| error!("Error executing interval: {:?}", e))
    };
    runtime.spawn(connect);

    ctrlc::set_handler(move || {
        info!("Stopping BGP server...");
        // Remove DB
        std::fs::remove_file("/tmp/bgpd.sqlite3").expect("Error deleting DB");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    Ok(runtime)
}