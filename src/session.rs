use std::fmt;
use std::io::Error;

use bgp_rs::Message;
use chrono::{DateTime, Duration, Utc};
use futures::sync::mpsc;
use futures::{Async, Poll, Stream};
use log::trace;
use tokio::prelude::*;

use crate::api::PeerSummary;
use crate::codec::MessageProtocol;
use crate::models::{
    HoldTimer, MessageCounts, MessageResponse, Peer, PeerState, Route, RouteState,
};
use crate::utils::format_time_as_elapsed;

pub type SessionTx = mpsc::UnboundedSender<SessionRoutes>;
pub type SessionRx = mpsc::UnboundedReceiver<SessionRoutes>;

pub enum SessionRoutes {
    Learned(Vec<Route>),
    Advertised(Vec<Route>),
}

pub struct Session {
    pub(crate) peer: Box<Peer>,
    protocol: MessageProtocol,
    tx: SessionTx,
    connect_time: DateTime<Utc>,
    hold_timer: HoldTimer,
    counts: MessageCounts,
    pending_routes: Vec<Route>,
}

impl Session {
    pub fn new(peer: Peer, protocol: MessageProtocol, tx: SessionTx) -> Session {
        let hold_timer = peer.hold_timer;
        Session {
            peer: Box::new(peer),
            protocol,
            tx,
            connect_time: Utc::now(),
            hold_timer: HoldTimer::new(hold_timer),
            counts: MessageCounts::new(),
            pending_routes: vec![],
        }
    }

    pub fn get_summary(&self) -> PeerSummary {
        PeerSummary {
            peer: self.peer.addr,
            router_id: self.peer.remote_id.router_id,
            asn: self.peer.remote_id.asn,
            msg_received: Some(self.counts.received()),
            msg_sent: Some(self.counts.sent()),
            connect_time: Some(self.connect_time.timestamp()),
            uptime: Some(format_time_as_elapsed(self.connect_time)),
            state: self.peer.get_state().to_string(),
            prefixes_received: None,
        }
    }

    pub fn add_pending_routes(&mut self, routes: Vec<Route>) {
        self.pending_routes.extend(routes);
    }

    // Send a message, and flush the send buffer afterwards
    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        trace!("[{}] Outgoing: {:?}", self.peer.addr, message);
        self.protocol
            .start_send(message)
            .and_then(|_| self.protocol.poll_complete())?;
        self.counts.increment_sent();
        Ok(())
    }

    // Prep the peer to send back to the Idle Peers HashMap
    // In order to do that, reset the Session's Peer with an empty Peer struct
    pub fn reset_peer(&mut self) -> Peer {
        let peer = std::mem::replace(&mut self.peer, Box::new(Peer::default()));
        *peer
    }
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Session {} uptime={} hold_time={}>",
            self.protocol.get_ref().peer_addr().unwrap(),
            format_time_as_elapsed(self.connect_time),
            self.hold_timer,
        )
    }
}

/// This is where a connected peer is managed.
///
/// A `Session` is also a stream for processing BGP messages and
/// handling peer timeouts
///
/// When a `Session` is created, the first OPEN message has already been read.
/// When the socket closes, the `Session` completes and the Peer struct
/// is returned back to the Idle Peers (for future connections)
///
impl Future for Session {
    type Item = Vec<Route>; // Learned routes
    type Error = SessionError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        trace!("Polling {}", self);

        // Read new messages from the socket
        while let Ok(Async::Ready(data)) = self.protocol.poll() {
            if let Some(message) = data {
                trace!("[{}] Incoming: {:?}", self.peer.addr, message);
                self.counts.increment_received();
                let resp = self.peer.process_message(message)?;
                match resp {
                    MessageResponse::Open((open, caps, hold_timer)) => {
                        self.peer.update_state(PeerState::OpenConfirm);
                        self.protocol.codec_mut().set_capabilities(caps);
                        self.hold_timer = HoldTimer::new(hold_timer);
                        self.send_message(Message::Open(open))
                            .map_err(|e| SessionError {
                                reason: e.to_string(),
                            })
                            .ok();
                        self.peer.update_state(PeerState::Established);
                    }
                    MessageResponse::Message(message) => {
                        self.send_message(message)?;
                    }
                    MessageResponse::LearnedRoutes(routes) => {
                        self.tx
                            .unbounded_send(SessionRoutes::Learned(routes))
                            .unwrap();
                        // TODO: Make this a stream instead of using a channel?
                        // return Ok(Async::Ready(routes));
                        ()
                    }
                    _ => (),
                }
                self.hold_timer.received_update();
            } else {
                // Before the Session is dropped, send peer back to idle peers
                return Err(SessionError {
                    reason: format!(
                        "Session ended with {} [{}]",
                        self.peer.remote_id.router_id.unwrap(),
                        self.peer.addr
                    ),
                });
            }
        }

        if !self.pending_routes.is_empty() {
            let mut advertised: Vec<Route> = vec![];
            while let Some(mut route) = self.pending_routes.pop() {
                trace!("Sending route for {} to {}", route.prefix, self.peer.addr);
                self.send_message(Message::Update(self.peer.create_update(&route)))?;
                route.state = RouteState::Advertised(Utc::now());
                advertised.push(route);
            }
            self.tx
                .unbounded_send(SessionRoutes::Advertised(advertised))
                .unwrap();
        }

        // Check for hold time expiration (send keepalive if not expired)
        while let Ok(Async::Ready(_)) = self.hold_timer.poll() {
            if self.hold_timer.get_hold_time() == Duration::seconds(0) {
                return Err(SessionError {
                    reason: format!("Hold Time Expired [{}]: {}", self.hold_timer, self.peer),
                });
            } else if self.hold_timer.should_send_keepalive() {
                self.send_message(Message::KeepAlive)
                    .map_err(|e| SessionError {
                        reason: e.to_string(),
                    })
                    .ok();
            }
        }

        trace!("Finished polling {}", self);
        Ok(Async::NotReady)
    }
}

#[derive(Debug)]
pub struct SessionError {
    pub reason: String,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Session Error: {}", self.reason)
    }
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        SessionError {
            reason: error.to_string(),
        }
    }
}
