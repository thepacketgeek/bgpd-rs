use std::fmt;
use std::io::{Error, ErrorKind};
use std::net::IpAddr;
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
}

impl Session {
    pub fn new(peer: Peer, protocol: MessageProtocol) -> Session {
        Session {
            peer: Box::new(peer),
            protocol,
            connect_time: Instant::now(),
            last_message: Instant::now(),
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

impl std::string::ToString for PeerState {
    fn to_string(&self) -> String {
        match self {
            PeerState::Connect { .. } => "Connect".to_string(),
            PeerState::Active => "Active".to_string(),
            PeerState::Idle => "Idle".to_string(),
            PeerState::OpenSent { .. } => "OpenSent".to_string(),
            PeerState::OpenConfirm { .. } => "OpenConfirm".to_string(),
            PeerState::Established { .. } => "Established".to_string(),
        }
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
            "[{} | {}]",
            self.router_id
                .unwrap_or("0.0.0.0".parse::<IpAddr>().unwrap()),
            asn_to_dotted(self.asn)
        )
    }
}

#[allow(dead_code)]
pub struct Peer {
    pub addr: IpAddr,
    remote_id: PeerIdentifier,
    local_id: PeerIdentifier, // Server (local side) ID
    state: PeerState,
}

impl Peer {
    pub fn new(
        addr: IpAddr,
        state: PeerState,
        remote_id: PeerIdentifier,
        local_id: PeerIdentifier,
    ) -> Peer {
        Peer {
            addr,
            state,
            remote_id,
            local_id,
        }
    }

    pub fn open_received(&mut self, open: Open, mut protocol: MessageProtocol) -> MessageProtocol {
        let peer_addr = protocol.get_ref().peer_addr().unwrap();
        let (capabilities, remote_asn) = capabilities_from_params(&open.parameters);
        let remote_id = PeerIdentifier::new(
            Some(IpAddr::from(transform_u32_to_bytes(open.identifier))),
            remote_asn.unwrap_or(u32::from(open.peer_asn)),
        );
        debug!(
            "[{}] Received OPEN {} [w/ {} params]",
            peer_addr.ip(),
            remote_id,
            open.parameters.len()
        );
        self.remote_id = remote_id;
        protocol.codec_mut().set_capabilities(capabilities);
        self.state = PeerState::OpenConfirm;
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
                return Ok(Async::Ready(()));
            }
        }
        trace!("Finished polling {}", self);
        Ok(Async::NotReady)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // TODO: Update peer state, add back to Peers for polling
        warn!("Session ended with {}", self.peer);
    }
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<Peer {} {} {}>", self.addr, self.remote_id, self.state.to_string(),)
    }
}
