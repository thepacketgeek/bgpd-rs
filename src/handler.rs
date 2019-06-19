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
use crate::peer::{Channel, Peer, PeerIdentifier, PeerState, Session, Tx};

type Peers = HashMap<IpAddr, Peer>;

/// Receives a TcpStream from either an incoming connection or active polling,
/// and processes the OPEN message for the correct peer (if configured)
fn handle_new_connection(stream: TcpStream, peers: Arc<Mutex<Peers>>, channel: Tx) {
    let connection = MessageProtocol::new(stream, MessageCodec::new())
        .into_future()
        .map_err(|(e, _)| e)
        .and_then(move |(open, protocol)| {
            let peer_addr = protocol.get_ref().peer_addr().unwrap().ip();
            if let Some(mut peer) = peers.lock().unwrap().remove(&peer_addr) {
                peer.update_state(PeerState::OpenSent);
                if let Some(Message::Open(open)) = open {
                    let updated_protocol = peer.open_received(open, protocol);
                    let new_session = Session::new(peer, updated_protocol, channel);
                    return Either::B(new_session);
                } else {
                    warn!("Invalid first packet received");
                    return Either::A(future::ok(()));
                }
            } else {
                warn!("Unexpected connection from {}", peer_addr,);
            }
            Either::A(future::ok(()))
        })
        .map_err(|e| {
            error!("connection error = {}", e);
        });
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
    let socket = format!("{}:{}", addr, port);
    let listener = TcpListener::bind(&socket.parse().unwrap())?;
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
                        .filter(|(_, p)| p.is_passive() == false)
                        .map(|(_, p)| p.addr)
                        .collect()
                })
                .unwrap_or(vec![]);
            for peer_addr in idle_peers {
                connect_to_peer(peer_addr, addr, port, peers.clone(), sender.clone());
            }
            Ok(())
        })
        .map_err(|e| error!("Error executing interval: {:?}", e))
    };
    runtime.spawn(connect);

    // When sessions are dropped, they will send peers back over the
    // channel. Poll the channel and add back to the idle Peers container.
    let peer_poller = {
        let peers = peers.clone();
        channel
            .into_future()
            .and_then(move |peer| {
                debug!("Adding {} back to idle peers", peer);
                peers
                    .lock()
                    .map(|mut peers| {
                        peers.insert(peer.addr, peer);
                    })
                    .ok();
                future::ok(())
            })
            .map(|_| ())
            .map_err(|_| ())
    };
    runtime.spawn(peer_poller);

    runtime.shutdown_on_idle().wait().unwrap();
    Ok(())
}
