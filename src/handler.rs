use std::collections::HashMap;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bgp_rs::Message;
use futures::future::{self, Either, Future};
use log::{debug, error, info, trace, warn};
use net2::TcpBuilder;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::reactor::Handle;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use crate::codec::{MessageCodec, MessageProtocol};
use crate::config::ServerConfig;
use crate::display::{StatusRow, StatusTable};
use crate::peer::{Peer, PeerIdentifier, PeerState};
use crate::session::{Channel, Session, Tx};

type Peers = HashMap<IpAddr, Peer>;

/// Receives a TcpStream from either an incoming connection or active polling,
/// and processes the OPEN message for the correct peer (if configured)
fn handle_new_connection(stream: TcpStream, peers: Arc<Mutex<Peers>>, channel: Tx) {
    let peer_insert = {
        let peers = peers.clone();
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

    let connection = MessageProtocol::new(stream, MessageCodec::new())
        .into_future()
        .map_err(|(e, _)| e.into())
        .and_then(move |(open, protocol)| {
            let peer_addr = protocol.get_ref().peer_addr().unwrap().ip();
            if let Some(mut peer) = peers.lock().unwrap().remove(&peer_addr) {
                peer.update_state(PeerState::OpenSent);
                if let Some(Message::Open(open)) = open {
                    let (updated_protocol, hold_timer) = peer.open_received(open, protocol);
                    let new_session = Session::new(peer, updated_protocol, channel, hold_timer);
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

fn connect_to_peer(
    peer: IpAddr,
    source_addr: IpAddr,
    dest_port: u16,
    peers: Arc<Mutex<Peers>>,
    channel: Tx,
) {
    if let Ok(mut peers) = peers.lock() {
        if let Some(peer) = peers.get_mut(&peer) {
            peer.update_state(PeerState::Active);
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
                }
            }
            handle_new_connection(stream, peers.clone(), channel);
            Ok(())
        })
        .map_err(handle_error)
    };
    tokio::spawn(connect);
}

pub fn serve(addr: IpAddr, port: u16, config: ServerConfig) -> Result<(), Error> {
    let socket = SocketAddr::from((addr, port));
    let listener = TcpListener::bind(&socket)?;
    let mut runtime = Runtime::new().unwrap();

    let channel = Channel::new();

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
            (peer.addr, peer)
        })
        .collect();

    let peers: Arc<Mutex<Peers>> = Arc::new(Mutex::new(peers));

    // TCP Listener task
    let server = {
        let peers = peers.clone();
        let sender = channel.add_sender();
        listener
            .incoming()
            .for_each(move |stream: TcpStream| {
                debug!(
                    "Incoming new connection from {}",
                    stream.peer_addr().unwrap()
                );
                handle_new_connection(stream, peers.clone(), sender.clone());
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
        let sender = channel.add_sender();
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
                connect_to_peer(peer_addr, addr, port, peers.clone(), sender.clone());
            }
            Ok(())
        })
        .map_err(|e| error!("Error executing interval: {:?}", e))
    };
    runtime.spawn(connect);

    let session_status: Arc<Mutex<HashMap<IpAddr, StatusRow>>> =
        Arc::new(Mutex::new(HashMap::new()));
    // Poll the channel for Session status updates
    let session_poller = {
        let session_status = session_status.clone();
        channel.receiver.for_each(move |session| {
            // debug!("Got row from {} back to idle peers", peer);
            session_status
                .lock()
                .map(|mut sessions| {
                    if let Some(row) = session.status {
                        sessions.insert(session.addr, row);
                    } else {
                        sessions.remove(&session.addr);
                    }
                })
                .ok();
            future::ok(())
        })
    };
    runtime.spawn(session_poller);

    // TCP Connections outbound
    // Attempt to connect to configured & idle peers
    let peers_path = config.path_for_peers();
    let output = {
        let peers = peers.clone();
        let session_status = session_status.clone();
        Interval::new(Instant::now(), Duration::from_secs(1))
            .for_each(move |_| {
                let mut table = StatusTable::new();
                for (_, session) in session_status.lock().unwrap().iter() {
                    table.add_row(session);
                }
                for (_, peer) in peers.lock().unwrap().iter() {
                    table.add_row(&StatusRow::from(peer));
                }
                table.write(&peers_path);
                Ok(())
            })
            .map_err(|e| error!("Error writing session info: {:?}", e))
    };
    runtime.spawn(output);

    ctrlc::set_handler(move || {
        info!("Stopping BGP server...");
        StatusTable::new().write(&config.path_for_peers());
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    runtime.shutdown_on_idle().wait().unwrap();
    Ok(())
}
