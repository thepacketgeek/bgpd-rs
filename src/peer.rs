use futures::sync::mpsc;
use std::fmt;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

use bgp_rs::{Message, Open, OpenParameter};
use log::{debug, trace, warn};
use tokio::prelude::*;

use crate::codec::{capabilities_from_params, MessageProtocol};
use crate::utils::{as_u32_be, asn_to_dotted, transform_u32_to_bytes};

pub struct Session {
    peer: Box<Peer>,
    protocol: MessageProtocol,
    connect_time: Instant,
    last_message: Instant,
    channel: Tx,
}

impl Session {
    pub fn new(peer: Peer, protocol: MessageProtocol, channel: Tx) -> Session {
        Session {
            peer: Box::new(peer),
            protocol,
            connect_time: Instant::now(),
            last_message: Instant::now(),
            channel,
        }
    }

    fn update_last_message(&mut self) {
        self.last_message = Instant::now();
    }
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Session {} uptime={} last_message={}>",
            self.protocol.get_ref().peer_addr().unwrap(),
            self.connect_time.elapsed().as_secs(),
            self.last_message.elapsed().as_secs(),
        )
    }
}

pub enum PeerState {
    Connect,
    Active,
    Idle,
    OpenSent,
    OpenConfirm,
    Established,
}

impl fmt::Display for PeerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let word = match self {
            PeerState::Connect { .. } => "Connect",
            PeerState::Active => "Active",
            PeerState::Idle => "Idle",
            PeerState::OpenSent { .. } => "OpenSent",
            PeerState::OpenConfirm { .. } => "OpenConfirm",
            PeerState::Established { .. } => "Established",
        };
        write!(f, "{}", word)
    }
}

pub struct PeerIdentifier {
    pub router_id: Option<IpAddr>,
    pub asn: u32,
}

impl PeerIdentifier {
    pub fn new(router_id: Option<IpAddr>, asn: u32) -> PeerIdentifier {
        PeerIdentifier { router_id, asn }
    }
}

impl fmt::Display for PeerIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}/{}",
            self.router_id
                .unwrap_or_else(|| IpAddr::from(Ipv4Addr::new(0, 0, 0, 0))),
            asn_to_dotted(self.asn)
        )
    }
}

pub struct Peer {
    pub addr: IpAddr,
    remote_id: PeerIdentifier,
    local_id: PeerIdentifier, // Server (local side) ID
    state: PeerState,
    passive: bool,
}

impl Peer {
    pub fn new(
        addr: IpAddr,
        state: PeerState,
        remote_id: PeerIdentifier,
        local_id: PeerIdentifier,
        passive: bool,
    ) -> Peer {
        Peer {
            addr,
            state,
            remote_id,
            local_id,
            passive,
        }
    }

    pub fn is_passive(&self) -> bool {
        self.passive
    }

    pub fn update_state(&mut self, new_state: PeerState) {
        debug!(
            "{} went from {} to {}",
            self.addr,
            self.state.to_string(),
            new_state.to_string()
        );
        self.state = new_state;
    }

    pub fn open_received(&mut self, open: Open, mut protocol: MessageProtocol) -> MessageProtocol {
        let peer_addr = protocol.get_ref().peer_addr().unwrap();
        let (capabilities, remote_asn) = capabilities_from_params(&open.parameters);
        let remote_id = PeerIdentifier::new(
            Some(IpAddr::from(transform_u32_to_bytes(open.identifier))),
            remote_asn.unwrap_or_else(|| u32::from(open.peer_asn)),
        );
        debug!(
            "[{}] Received OPEN {} [w/ {} params]",
            peer_addr.ip(),
            remote_id,
            open.parameters.len()
        );
        self.remote_id = remote_id;
        protocol.codec_mut().set_capabilities(capabilities);
        self.update_state(PeerState::OpenConfirm);
        protocol
    }

    fn create_open(&self) -> Open {
        let router_id = match self.local_id.router_id {
            Some(IpAddr::V4(ipv4)) => ipv4,
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

    pub fn process_message(&mut self, message: Message) -> Result<Option<Message>, Error> {
        trace!("{}: {:?}", self.addr, message);
        let response = match message {
            Message::KeepAlive => Some(Message::KeepAlive),
            Message::Update(_) => None,
            Message::Notification => None,
            Message::RouteRefresh(_) => None,
            _ => {
                warn!("{} Unexpected message {:?}", self.addr, message);
                return Err(Error::from(ErrorKind::InvalidInput));
            }
        };
        Ok(response)
    }
}

impl Default for Peer {
    fn default() -> Self {
        let ip = "0.0.0.0".parse().unwrap();
        Peer::new(
            ip,
            PeerState::Idle,
            PeerIdentifier::new(Some(ip), 0),
            PeerIdentifier::new(Some(ip), 0),
            false,
        )
    }
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Peer {} {} {}>",
            self.addr,
            self.remote_id,
            self.state.to_string(),
        )
    }
}

/// This is where a connected peer is managed.
///
/// A `Session` is also a future representing completely processing the session.
///
/// When a `Session` is created, the first line (representing the session's name)
/// has already been read. When the socket closes, the `Session` future completes.
///
///
/// TODO: Session polls, updating it's peer status
impl Future for Session {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<(), Error> {
        trace!("Polling {}", self);

        if let PeerState::OpenConfirm = self.peer.state {
            self.protocol
                .start_send(Message::Open(self.peer.create_open()))
                .and_then(|_| self.protocol.poll_complete())?;
            self.peer.state = PeerState::Established;
        }

        // Read new messages from the socket
        while let Async::Ready(data) = self.protocol.poll()? {
            if let Some(message) = data {
                debug!("[{}] Received message {:?}", self.peer.addr, message);
                self.peer
                    .process_message(message)
                    .and_then(|resp| {
                        if let Some(data) = resp {
                            self.protocol.start_send(data).ok();
                        }
                        Ok(())
                    })
                    .and_then(|_| self.protocol.poll_complete())?;
                self.update_last_message();
            } else {
                warn!("Session ended with {}", self.peer);
                // Before the Session is dropped, need to send the peer
                // back to the peers HashMap.
                // Create a default peer to replace self.peer
                let mut peer = std::mem::replace(&mut self.peer, Box::new(Peer::default()));
                peer.update_state(PeerState::Idle);
                self.channel
                    .start_send(*peer)
                    .and_then(|_| self.channel.poll_complete())
                    .ok();
                return Ok(Async::Ready(()));
            }
        }
        trace!("Finished polling {}", self);
        Ok(Async::NotReady)
    }
}

pub type Tx = mpsc::UnboundedSender<Peer>;
pub type Rx = mpsc::UnboundedReceiver<Peer>;

pub struct Channel {
    receiver: Rx,
    sender: Tx,
}

impl Channel {
    pub fn new() -> Channel {
        let (sender, receiver) = mpsc::unbounded();
        Channel { receiver, sender }
    }

    pub fn add_sender(&self) -> Tx {
        self.sender.clone()
    }
}

impl Future for Channel {
    type Item = Peer;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Error> {
        // Check for peers returned from ended Session
        if let Async::Ready(Some(peer)) = self.receiver.poll().unwrap() {
            return Ok(Async::Ready(peer));
        }
        Ok(Async::NotReady)
    }
}
