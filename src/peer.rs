use std::cmp;
use std::convert::From;
use std::fmt;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr};

use bgp_rs::{Message, Open, OpenParameter};
use log::{debug, trace, warn};

use crate::codec::{capabilities_from_params, MessageProtocol};
use crate::display::StatusRow;
use crate::utils::{as_u32_be, asn_to_dotted, transform_u32_to_bytes};

#[derive(Debug, Copy, Clone)]
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
            PeerState::Connect => "Connect",
            PeerState::Active => "Active",
            PeerState::Idle => "Idle",
            PeerState::OpenSent => "OpenSent",
            PeerState::OpenConfirm => "OpenConfirm",
            PeerState::Established => "Established",
        };
        write!(f, "{}", word)
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Peer {
    pub addr: IpAddr,
    pub remote_id: PeerIdentifier,
    local_id: PeerIdentifier, // Server (local side) ID
    state: PeerState,
    passive: bool,
    hold_timer: u16,
}

impl Peer {
    pub fn new(
        addr: IpAddr,
        state: PeerState,
        remote_id: PeerIdentifier,
        local_id: PeerIdentifier,
        passive: bool,
        hold_timer: u16,
    ) -> Peer {
        Peer {
            addr,
            state,
            remote_id,
            local_id,
            passive,
            hold_timer,
        }
    }

    pub fn is_passive(&self) -> bool {
        self.passive
    }

    pub fn get_state(&self) -> PeerState {
        self.state
    }

    pub fn update_state(&mut self, new_state: PeerState) {
        let label = if self.remote_id.router_id.is_some() {
            format!("{}", self.remote_id)
        } else {
            format!("{}", self.addr)
        };
        debug!(
            "{} went from {} to {}",
            label,
            self.state.to_string(),
            new_state.to_string()
        );
        self.state = new_state;
    }

    pub fn open_received(
        &mut self,
        open: Open,
        mut protocol: MessageProtocol,
    ) -> (MessageProtocol, u16) {
        let peer_addr = protocol.get_ref().peer_addr().unwrap();
        let (capabilities, remote_asn) = capabilities_from_params(&open.parameters);
        let remote_id = PeerIdentifier::new(
            Some(IpAddr::from(transform_u32_to_bytes(open.identifier))),
            remote_asn.unwrap_or_else(|| u32::from(open.peer_asn)),
        );
        let hold_timer = cmp::min(open.hold_timer, self.hold_timer);
        debug!(
            "[{}] Received OPEN {} [w/ {} params]",
            remote_id,
            peer_addr.ip(),
            open.parameters.len()
        );
        self.remote_id = remote_id;
        protocol.codec_mut().set_capabilities(capabilities);
        self.update_state(PeerState::OpenConfirm);
        (protocol, hold_timer)
    }

    pub fn create_open(&self) -> Open {
        let router_id = match self.local_id.router_id {
            Some(IpAddr::V4(ipv4)) => ipv4,
            _ => unreachable!(),
        };
        Open {
            version: 4,
            peer_asn: self.local_id.asn as u16,
            hold_timer: self.hold_timer,
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
        trace!("{}: {:?}", self.remote_id, message);
        let response = match message {
            Message::KeepAlive => Some(Message::KeepAlive),
            Message::Update(_) => None,
            Message::Notification(notification) => {
                warn!("{} NOTIFICATION: {}", self.remote_id, notification);
                None
            }
            Message::RouteRefresh(_) => None,
            _ => {
                warn!("{} Unexpected message {:?}", self.remote_id, message);
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
            0,
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

impl From<&Peer> for StatusRow {
    fn from(peer: &Peer) -> Self {
        StatusRow {
            neighbor: peer.addr,
            asn: peer.remote_id.asn,
            msg_received: 0,
            msg_sent: 0,
            connect_time: None,
            state: peer.state,
            prefixes_received: 0,
        }
    }
}

#[derive(Debug)]
pub struct MessageCounts {
    received: u64,
    sent: u64,
}

impl MessageCounts {
    pub fn new() -> Self {
        MessageCounts {
            received: 0,
            sent: 0,
        }
    }

    pub fn received(&self) -> u64 {
        self.received
    }
    pub fn increment_received(&mut self) {
        self.received += 1;
    }

    pub fn sent(&self) -> u64 {
        self.sent
    }
    pub fn increment_sent(&mut self) {
        self.sent += 1;
    }
}
