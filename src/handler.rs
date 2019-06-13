use std::io::Error;

use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use bgp_rs::Message;
use futures::future::{self, Either, Future};
use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use crate::codec::{MessageCodec, MessageProtocol};
use crate::config::{PeerConfig, ServerConfig};
use crate::peer::{Peer, PeerIdentifier};

fn handle_new_connection(
    stream: TcpStream,
    config: Arc<Mutex<ServerConfig>>,
) {
    let messages = MessageProtocol::new(stream, MessageCodec::new());

    let connection = messages
        .into_future()
        // `into_future` doesn't have the right error type, so map the error to make it work.
        .map_err(|(e, _)| e)
        // Process the first received Open message
        .and_then(move |(open, protocol)| {
            if let Ok(config) = config.lock() {
                let peer_addr = protocol.get_ref().peer_addr().unwrap().ip();
                let peer_config: Option<&PeerConfig> =
                    config.peers.iter().find(|p| p.remote_ip == peer_addr);
                if let Some(peer_config) = peer_config {
                    if let Some(Message::Open(open)) = open {
                        let peer = Peer::from_open(
                            protocol,
                            PeerIdentifier::new(
                                peer_config.router_id.unwrap_or(config.router_id),
                                peer_config.local_as.unwrap_or(config.default_as),
                            ),
                            open,
                        );
                        return Either::B(peer);
                    }
                } else {
                    warn!("Unexpected connection from {}", peer_addr,);
                }
            }
            Either::A(future::ok(()))
        })
        .map_err(|e| {
            error!("connection error = {:?}", e);
        });
    tokio::spawn(connection);
}

pub fn serve(addr: IpAddr, port: u32, config: ServerConfig) -> Result<(), Error> {
    let socket = format!("{}:{}", addr, port);
    let listener = TcpListener::bind(&socket.parse().unwrap())?;
    let config = Arc::new(Mutex::new(config));

    let server = listener
        .incoming()
        .for_each(move |stream| {
            debug!(
                "Incoming new connection from {}",
                stream.peer_addr().unwrap()
            );
            handle_new_connection(stream, config.clone());
            Ok(())
        })
        .map_err(|err| error!("Incoming connection failed: {}", err));

    info!("Starting BGP server on {}...", socket);
    tokio::run(server);
    Ok(())
}
