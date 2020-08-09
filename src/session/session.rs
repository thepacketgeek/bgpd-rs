use std::cmp;
use std::fmt;
use std::io;
use std::net::IpAddr;
use std::sync::Arc;

use bgp_rs::{
    Capabilities, Message, Notification, Open, OpenCapability, OpenParameter, RouteRefresh, Update,
};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use log::{debug, trace, warn};
use tokio;

use super::codec::MessageProtocol;
use super::{HoldTimer, MessageCounts};
use super::{SessionError, SessionState, SessionUpdate};
use crate::config::PeerConfig;
use crate::utils::{format_time_as_elapsed, get_message_type};

/// A `Session` is a stream for processing BGP messages and
/// handling peer timeouts
pub struct Session {
    pub(crate) addr: IpAddr,
    pub(crate) state: SessionState,
    pub(crate) router_id: IpAddr,
    pub(crate) config: Arc<PeerConfig>,
    pub(crate) protocol: MessageProtocol,
    pub(crate) connect_time: DateTime<Utc>,
    pub(crate) hold_timer: HoldTimer,
    pub(crate) counts: MessageCounts,
    pub(crate) capabilities: Capabilities,
}

impl Session {
    /// Build a newly created session from the peer config & BGP Message Stream
    pub fn new(config: Arc<PeerConfig>, protocol: MessageProtocol) -> Session {
        let hold_timer = config.hold_timer;
        let capabilities: Vec<OpenCapability> = vec![OpenCapability::FourByteASN(config.local_as)]
            .into_iter()
            .chain(config.families.iter().map(|f| f.to_open_param()))
            .collect();
        Session {
            addr: protocol
                .get_ref()
                .peer_addr()
                .expect("Stream has remote IP")
                .ip(),
            state: SessionState::Connect,
            router_id: protocol
                .get_ref()
                .peer_addr()
                .expect("Protocol has remote peer")
                .ip(),
            config,
            protocol,
            connect_time: Utc::now(),
            hold_timer: HoldTimer::new(hold_timer),
            counts: MessageCounts::new(),
            capabilities: Capabilities::from_parameters(vec![OpenParameter::Capabilities(
                capabilities,
            )]),
        }
    }

    /// Did the local side initiate the connection out (vs. receiving SYN from peer)
    ///   This is true if the remote port is the configured dest port
    ///   since a remote initiation would mean a random remote port
    pub fn is_locally_initiated(&self) -> bool {
        let remote_port = self
            .protocol
            .get_ref()
            .peer_addr()
            .expect("Getting remote addr")
            .port();
        remote_port == self.config.dest_port
    }

    pub fn update_state(&mut self, new_state: SessionState) {
        debug!(
            "{} went from {} to {}",
            self.addr,
            self.state.to_string(),
            new_state.to_string()
        );
        self.state = new_state;
    }

    pub fn update_config(&mut self, new_config: Arc<PeerConfig>) {
        debug!("Peer config for {} (active session) updated", self.addr);
        self.config = new_config;
    }

    /// Main function for making progress with the session
    /// Waits for either a new incoming message or a HoldTimer event
    pub async fn run(&mut self) -> Result<Option<SessionUpdate>, SessionError> {
        if !self.config.enabled {
            // Peer has been disabled, shutdown session
            return Err(SessionError::Deconfigured);
        }
        if self.state == SessionState::Connect && self.is_locally_initiated() {
            let open = self.create_open();
            if let Err(err) = self.send_message(Message::Open(open)).await {
                warn!("Error sending OPEN message: {}", err);
            }
            self.update_state(SessionState::OpenSent);
        }
        trace!("Hold time on {}: {}", self.addr, self.hold_timer);

        tokio::select! {
            message = self.protocol.next() => {
                match message {
                    // Framed stream is exhausted, remote side closed connection
                    None => {
                        Err(SessionError::Other(format!(
                            "Session ended with {}",
                            self.addr
                        )))
                    }
                    // Message received, update session and reply if required
                    Some(Ok(message)) => {
                        let message_type = get_message_type(&message);
                        trace!("[{}] Incoming: {}", self.addr, message_type);
                        self.counts.increment_received();
                        self.hold_timer.received();
                        let resp = self.process_message(message)?;
                        match resp {
                            MessageResponse::Reply(message) => {
                                self.send_message(message).await?;
                            }
                            MessageResponse::Update(update) => {
                                return Ok(Some(SessionUpdate::Learned((self.addr, update))));
                            }
                            MessageResponse::Refresh(refresh) => {
                                return Ok(Some(SessionUpdate::RouteRefresh(refresh)));
                            }
                            _ => (),
                        }
                        Ok(None)
                    }
                    // Error decoding message
                    Some(Err(err)) => {
                        Err(SessionError::CodecError(format!(
                            "Session ended with {}: {}",
                            self.addr, err
                        )))
                    }
                }
            },
            // Hold Timer
            keepalive = self.hold_timer.should_send_keepalive() => {
                match keepalive {
                    Err(err) => Err(err),
                    Ok(should_send) => {if should_send {
                        self.send_message(Message::KeepAlive).await?;
                    } Ok(None)}
                }
            },
        }
    }

    pub fn process_message(&mut self, message: Message) -> Result<MessageResponse, SessionError> {
        match message {
            Message::Open(open) => {
                let (capabilities, hold_timer) = self.receive_open(open)?;
                self.capabilities = capabilities;
                self.hold_timer = HoldTimer::new(hold_timer);
                match &self.state {
                    // Remote initiated, reply with OPEN
                    SessionState::Connect => {
                        self.update_state(SessionState::OpenConfirm);
                        Ok(MessageResponse::Reply(Message::Open(self.create_open())))
                    }
                    SessionState::OpenSent => {
                        self.update_state(SessionState::OpenConfirm);
                        Ok(MessageResponse::Reply(Message::KeepAlive))
                    }
                    _ => {
                        return Err(SessionError::FiniteStateMachine(fsm_err_for_state(
                            self.state,
                        )));
                    }
                }
            }
            Message::KeepAlive => match self.state {
                SessionState::OpenConfirm => {
                    self.update_state(SessionState::Established);
                    Ok(MessageResponse::Reply(Message::KeepAlive))
                }
                _ => Ok(MessageResponse::Empty),
            },
            Message::Update(update) => Ok(MessageResponse::Update(update)),
            Message::Notification(notification) => {
                warn!("{} NOTIFICATION: {}", self.addr, notification.to_string());
                Ok(MessageResponse::Empty)
            }
            Message::RouteRefresh(refresh) => Ok(MessageResponse::Refresh(refresh)),
        }
    }

    // Send a message, and flush the send buffer afterwards
    pub async fn send_message(&mut self, message: Message) -> Result<(), io::Error> {
        let message_type = get_message_type(&message);
        trace!("[{}] Outgoing: {}", self.addr, message_type);
        self.protocol.send(message).await?;
        self.counts.increment_sent();
        self.hold_timer.sent();
        Ok(())
    }

    pub async fn notify(&mut self, maj: u8, min: u8) -> Result<(), io::Error> {
        let notif = Notification {
            major_err_code: maj,
            minor_err_code: min,
            data: vec![],
        };
        self.send_message(Message::Notification(notif)).await
    }

    pub fn receive_open(
        &mut self,
        received_open: Open,
    ) -> Result<(Capabilities, u16), SessionError> {
        let router_id = IpAddr::from(received_open.identifier.to_be_bytes());
        let remote_asn = asn_from_open(&received_open);
        if remote_asn != self.config.remote_as {
            return Err(SessionError::OpenAsnMismatch(
                remote_asn,
                self.config.remote_as,
            ));
        }
        let hold_timer = cmp::min(received_open.hold_timer, self.config.hold_timer);
        debug!(
            "[{}] Received OPEN [w/ {} params]",
            self.addr,
            received_open.parameters.len()
        );
        self.router_id = router_id;
        let received_capabilities = Capabilities::from_parameters(received_open.parameters);
        let common_capabilities = common_capabilities(&self.capabilities, &received_capabilities)?;
        Ok((common_capabilities, hold_timer))
    }

    pub fn create_open(&self) -> Open {
        let router_id = match self.config.local_router_id {
            IpAddr::V4(ipv4) => ipv4,
            _ => unreachable!(),
        };
        let families: Vec<_> = self
            .config
            .families
            .iter()
            .map(|family| family.to_open_param())
            .collect();
        let mut capabilities: Vec<OpenCapability> =
            Vec::with_capacity(self.config.families.len() + 1);
        capabilities.extend(families);
        capabilities.push(OpenCapability::FourByteASN(self.config.local_as));
        let two_byte_asn = if self.config.local_as < 65535 {
            self.config.local_as as u16
        } else {
            // AS-TRANS: RFC 6793 [4.2.3.9]
            23456
        };
        Open {
            version: 4,
            peer_asn: two_byte_asn,
            hold_timer: self.hold_timer.hold_timer,
            identifier: u32::from_be_bytes(router_id.octets()),
            parameters: vec![OpenParameter::Capabilities(capabilities)],
        }
    }
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Session {} uptime={} hold_time={}>",
            self.addr,
            format_time_as_elapsed(self.connect_time),
            self.hold_timer,
        )
    }
}

#[derive(Debug)]
pub enum MessageResponse {
    Open((Open, Vec<OpenCapability>, u16)),
    Reply(Message),
    Update(Update),
    Refresh(RouteRefresh),
    Empty,
}

fn fsm_err_for_state(state: SessionState) -> u8 {
    use SessionState::*;
    match state {
        OpenSent => 1,
        OpenConfirm => 2,
        Established => 3,
        _ => 0,
    }
}

/// Check 4-byte ASN first, fallback to 2-byte
fn asn_from_open(open: &Open) -> u32 {
    open.parameters
        .iter()
        .map(|p| match p {
            OpenParameter::Capabilities(caps) => caps.clone(),
            _ => vec![],
        })
        .flatten()
        .map(|c| match c {
            OpenCapability::FourByteASN(asn) => Some(asn),
            _ => None,
        })
        .find(|c| c.is_some())
        .unwrap_or_else(|| Some(u32::from(open.peer_asn)))
        .unwrap()
}

/// Work out the common set of capabilities between peer config and the received peer's capabilities
pub fn common_capabilities(
    a: &Capabilities,
    b: &Capabilities,
) -> Result<Capabilities, SessionError> {
    // And (manually) build an intersection between the two
    let mut negotiated = Capabilities::default();

    negotiated.MP_BGP_SUPPORT = a
        .MP_BGP_SUPPORT
        .intersection(&b.MP_BGP_SUPPORT)
        .copied()
        .collect();
    negotiated.ROUTE_REFRESH_SUPPORT = a.ROUTE_REFRESH_SUPPORT & b.ROUTE_REFRESH_SUPPORT;
    negotiated.OUTBOUND_ROUTE_FILTERING_SUPPORT = a
        .OUTBOUND_ROUTE_FILTERING_SUPPORT
        .intersection(&b.OUTBOUND_ROUTE_FILTERING_SUPPORT)
        .copied()
        .collect();

    // Attempt at a HashMap intersection. We can be a bit lax here because this isn't a real BGP implementation
    // so we can not care too much about the values for now.
    negotiated.EXTENDED_NEXT_HOP_ENCODING = a
        .EXTENDED_NEXT_HOP_ENCODING
        .iter()
        // .filter(|((afi, safi), _)| b.EXTENDED_NEXT_HOP_ENCODING.contains_key(&(*afi, *safi)))
        .map(|((afi, safi), nexthop)| ((*afi, *safi), *nexthop))
        .collect();

    negotiated.BGPSEC_SUPPORT = a.BGPSEC_SUPPORT & b.BGPSEC_SUPPORT;

    negotiated.MULTIPLE_LABELS_SUPPORT = a
        .MULTIPLE_LABELS_SUPPORT
        .iter()
        .filter(|((afi, safi), _)| b.MULTIPLE_LABELS_SUPPORT.contains_key(&(*afi, *safi)))
        .map(|((afi, safi), val)| ((*afi, *safi), *val))
        .collect();

    negotiated.GRACEFUL_RESTART_SUPPORT = a
        .GRACEFUL_RESTART_SUPPORT
        .intersection(&b.GRACEFUL_RESTART_SUPPORT)
        .copied()
        .collect();
    negotiated.FOUR_OCTET_ASN_SUPPORT = a.FOUR_OCTET_ASN_SUPPORT & b.FOUR_OCTET_ASN_SUPPORT;

    negotiated.ADD_PATH_SUPPORT = a
        .ADD_PATH_SUPPORT
        .iter()
        .filter(|((afi, safi), _)| b.ADD_PATH_SUPPORT.contains_key(&(*afi, *safi)))
        .map(|((afi, safi), val)| ((*afi, *safi), *val))
        .collect();
    negotiated.EXTENDED_PATH_NLRI_SUPPORT = !negotiated.ADD_PATH_SUPPORT.is_empty();

    negotiated.ENHANCED_ROUTE_REFRESH_SUPPORT =
        a.ENHANCED_ROUTE_REFRESH_SUPPORT & b.ENHANCED_ROUTE_REFRESH_SUPPORT;
    negotiated.LONG_LIVED_GRACEFUL_RESTART =
        a.LONG_LIVED_GRACEFUL_RESTART & b.LONG_LIVED_GRACEFUL_RESTART;

    Ok(negotiated)
}
