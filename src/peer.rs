use std::fmt;
use std::io::Error;
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use bgp_rs::{Capabilities, Message, Open, OpenParameter};
use log::{debug, info, trace, warn};
use tokio::prelude::*;

use crate::codec::MessageProtocol;
use crate::utils::{as_u32_be, transform_u32_to_bytes};

pub enum PeerState {
    Connect,
    Active,
    Idle,
    OpenSent,
    OpenConfirm,
    Established,
}

impl std::string::ToString for PeerState {
    fn to_string(&self) -> String {
        match self {
            PeerState::Connect => "Connect".to_string(),
            PeerState::Active => "Active".to_string(),
            PeerState::Idle => "Idle".to_string(),
            PeerState::OpenSent => "OpenSent".to_string(),
            PeerState::OpenConfirm => "OpenConfirm".to_string(),
            PeerState::Established => "Established".to_string(),
        }
    }
}

pub struct PeerIdentifier {
    pub router_id: IpAddr,
    pub asn: u32,
}

impl PeerIdentifier {
    pub fn new(router_id: IpAddr, asn: u32) -> PeerIdentifier {
        PeerIdentifier { router_id, asn }
    }
}

impl fmt::Display for PeerIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{} | {}]", self.router_id, self.asn)
    }
}

#[allow(dead_code)]
pub struct Peer {
    // TCP Session
    addr: SocketAddr,
    messages: MessageProtocol,

    // BGP Config
    remote_id: PeerIdentifier,
    local_id: PeerIdentifier, // Server (local side) ID
    state: PeerState,
    capabilities: Capabilities,
    connect_time: Instant,
    last_message: Instant,
}

impl Peer {
    pub fn new(
        messages: MessageProtocol,
        state: PeerState,
        remote_id: PeerIdentifier,
        local_id: PeerIdentifier,
        capabilities: Capabilities,
    ) -> Peer {
        // Get the session socket address
        let addr = messages.get_ref().peer_addr().unwrap();

        info!("[{}] New Connection", addr);
        Peer {
            addr,
            messages,
            state,
            remote_id,
            local_id,
            capabilities,
            connect_time: Instant::now(),
            last_message: Instant::now(),
        }
    }

    pub fn from_open(messages: MessageProtocol, local_id: PeerIdentifier, open: Open) -> Peer {
        let peer_addr = messages.get_ref().peer_addr().unwrap();
        debug!(
            "[{}] Received OPEN [# params={}]",
            peer_addr.ip(),
            open.parameters.len()
        );
        let mut capabilities = Capabilities::default();
        for param in open.parameters.iter().filter(|p| p.param_type == 2) {
            if param.value[0] == 65 {
                trace!("EXTENDED_PATH_NLRI_SUPPORT {:?}", param);
                capabilities.EXTENDED_PATH_NLRI_SUPPORT = true;
            }
        }
        let remote_id = PeerIdentifier::new(
            IpAddr::from(transform_u32_to_bytes(open.identifier)),
            u32::from(open.peer_asn),
        );
        Peer::new(
            messages,
            PeerState::OpenConfirm,
            remote_id,
            local_id,
            capabilities,
        )
    }

    fn update_last_message(&mut self) {
        self.last_message = Instant::now();
    }

    pub fn process_message(&mut self, message: Message) -> Option<Message> {
        trace!("{}: {:?}", self.remote_id, message);
        match message {
            Message::KeepAlive => {
                self.update_last_message();
                Some(Message::KeepAlive)
            }
            Message::Update(_) => None,
            Message::Notification => None,
            Message::RouteRefresh(_) => None,
            _ => {
                warn!("{} Unexpected message {:?}", self.remote_id, message);
                None
            }
        }
    }

    fn create_open(&self) -> Open {
        let router_id = match self.local_id.router_id {
            IpAddr::V4(ipv4) => ipv4,
            _ => unreachable!(),
        };
        Open {
            version: 4,
            peer_asn: self.local_id.asn as u16,
            hold_timer: 180,
            identifier: as_u32_be(router_id.octets()),
            parameters: vec![
                OpenParameter {
                    // SAFI Ipv4 Unicast
                    param_type: 2,
                    param_length: 6,
                    value: vec![0x01, 0x04, 0x00, 0x01, 0x00, 0x01],
                },
                OpenParameter {
                    // SAFI IPv6 Unicast
                    param_type: 2,
                    param_length: 6,
                    value: vec![0x01, 0x04, 0x00, 0x02, 0x00, 0x01],
                },
                OpenParameter {
                    // 4-byte ASN
                    param_type: 2,
                    param_length: 6,
                    value: vec![0x41, 0x04, 0x00, 0x02, 0xfd, 0xe8],
                },
            ],
        }
    }
}

/// This is where a connected peer is managed.
///
/// A `Peer` is also a future representing completely processing the session.
///
/// When a `Peer` is created, the first line (representing the session's name)
/// has already been read. When the socket closes, the `Peer` future completes.
///
/// While processing, the session future implementation will:
///
/// 1) Receive messages on its message channel and write them to the socket.
/// 2) Receive messages from the socket and broadcast them to all peers.
///
impl Future for Peer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<(), Error> {
        trace!("Polling {}", self);

        // TODO: Respond with our OPEN Message
        if let PeerState::OpenConfirm = self.state {
            if self
                .messages
                .start_send(Message::Open(self.create_open()))
                .is_ok()
            {
                self.messages.poll_complete()?;
            }
            self.state = PeerState::Established;
        }

        // Read new messages from the socket
        while let Async::Ready(data) = self.messages.poll()? {
            if let Some(message) = data {
                debug!("[{}] Received message {:?}", self.addr, message);
                if let Some(resp) = self.process_message(message) {
                    if self.messages.start_send(resp).is_ok() {
                        self.messages.poll_complete()?;
                    }
                }
            } else {
                // TODO: Update peer state
                warn!("Peer disconnected: {}", self.addr);
                return Ok(Async::Ready(()));
            }
        }
        trace!("Finished polling {}", self);
        Ok(Async::NotReady)
    }
}

// impl Drop for Peer {
//     fn drop(&mut self) {
//         // TODO: Update Peer state
//     }
// }

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Peer {} {} socket={} uptime={} last_message={}>",
            self.remote_id,
            self.state.to_string(),
            self.addr,
            self.connect_time.elapsed().as_secs(),
            self.last_message.elapsed().as_secs(),
        )
    }
}
