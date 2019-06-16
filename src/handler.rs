use std::collections::HashMap;
use std::io::Error;

use bgp_rs::Message;
use futures::future::{self, Either, Future};
use log::{debug, error, info, warn};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use crate::codec::{MessageCodec, MessageProtocol};
use crate::config::ServerConfig;
use crate::peer::{Peer, PeerIdentifier, PeerState, Session};

type Peers = HashMap<IpAddr, Peer>;

fn handle_new_connection(stream: TcpStream, peers: Arc<Mutex<Peers>>) {
    let messages = MessageProtocol::new(stream, MessageCodec::new());

    let connection = messages
        .into_future()
        // `into_future` doesn't have the right error type, so map the error to make it work.
        .map_err(|(e, _)| e)
        // Process the first received Open message
        .and_then(move |(open, protocol)| {
            let peer_addr = protocol.get_ref().peer_addr().unwrap().ip();
            if let Some(mut peer) = peers.lock().unwrap().remove(&peer_addr) {
                if let Some(Message::Open(open)) = open {
                    let updated_protocol = peer.open_received(open, protocol);
                    let new_session = Session::new(peer, updated_protocol);
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
            error!("connection error = {:?}", e);
        });
    tokio::spawn(connection);
}

pub fn serve(addr: IpAddr, port: u16, config: ServerConfig) -> Result<(), Error> {
    let socket = format!("{}:{}", addr, port);
    let listener = TcpListener::bind(&socket.parse().unwrap())?;
    let mut runtime = Runtime::new().unwrap();

    // Peers become attributes (owned by) a session when it begin
    let peers: Peers = config
        .peers
        .iter()
        .map(|p| {
            let peer = Peer::new(
                p.remote_ip,
                PeerState::Idle,
                PeerIdentifier::new(
                    p.router_id.unwrap_or(config.router_id),
                    p.local_as.unwrap_or(config.default_as),
                ), // local
            );
            (peer.addr, peer)
        })
        .collect();

    let peers: Arc<Mutex<Peers>> = Arc::new(Mutex::new(peers));
    let future_peers = peers.clone();

    let server = listener
        .incoming()
        .for_each(move |stream| {
            debug!(
                "Incoming new connection from {}",
                stream.peer_addr().unwrap()
            );
            handle_new_connection(stream, peers.clone());
            Ok(())
        })
        .map_err(|err| error!("Incoming connection failed: {}", err));

    info!("Starting BGP server on {}...", socket);
    runtime.spawn(server);

    let task = Interval::new(
        Instant::now() + Duration::from_secs(10), // Initial delay
        Duration::from_secs(15),  // Interval
    )
    .for_each(move |_| {
        if let Ok(peers) = future_peers.lock() {
            for (addr, peer) in peers.iter() {
                debug!("Caddr: {} peer: {}", addr, peer);
                // let socket = SocketAddr::new(addr.clone(), port);
                // let connection = TcpStream::connect(&socket).and_then(|sock| {
                //     let protocol = MessageProtocol::new(sock, MessageCodec::new());
                //     protocol.for_each(|message| {
                //         println!("Received message {:?}", message);
                //         Ok(())
                //     })
                // });
                // runtime.spawn(connection);
            }
        }

        Ok(())
    })
    .map_err(|e| panic!("interval errored; err={:?}", e));

    runtime.spawn(task);

    runtime.shutdown_on_idle().wait().unwrap();
    Ok(())
}
