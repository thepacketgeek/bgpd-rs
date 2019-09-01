use std::fmt;
use std::io::{Error, ErrorKind};

use bgp_rs::{Identifier, Message, NLRIEncoding, PathAttribute};
use chrono::{DateTime, Duration, Utc};
use futures::sync::mpsc;
use futures::{Async, Poll, Stream};
use log::{error, trace, warn};
use tokio::prelude::*;

use crate::codec::MessageProtocol;
use crate::models::{
    Community, CommunityList, HoldTimer, MessageCounts, MessageResponse, Peer, PeerState,
    PeerSummary, Route, RouteState,
};
use crate::utils::format_time_as_elapsed;

pub type SessionTx = mpsc::UnboundedSender<Vec<Route>>;
pub type SessionRx = mpsc::UnboundedReceiver<Vec<Route>>;

pub enum SessionMessage {
    LearnedRoutes(Route),
    AdvertisedRoute(Route),
}

pub struct Session {
    peer: Box<Peer>,
    protocol: MessageProtocol,
    tx: SessionTx,
    connect_time: DateTime<Utc>,
    hold_timer: HoldTimer,
    counts: MessageCounts,
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
        }
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

    // fn update_peer_summary(&self) {
    //     DB::new()
    //         .and_then(|db| {
    //             let summary = PeerSummary {
    //                 neighbor: self.peer.addr,
    //                 router_id: self.peer.remote_id.router_id,
    //                 asn: self.peer.remote_id.asn,
    //                 msg_received: Some(self.counts.received()),
    //                 msg_sent: Some(self.counts.sent()),
    //                 connect_time: Some(self.connect_time),
    //                 state: self.peer.get_state(),
    //                 prefixes_received: None,
    //             };
    //             db.update_peer(&summary)
    //         })
    //         .map_err(|err| error!("{:?}", err))
    //         .ok();
    // }

    // Prep the peer to send back to the Idle Peers HashMap
    // In order to do that, reset the Session's Peer with an empty Peer struct
    fn reset_peer(&mut self) -> Peer {
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
                                peer: Some(self.reset_peer()),
                            })
                            .ok();
                        self.peer.update_state(PeerState::Established);
                    }
                    MessageResponse::Message(message) => {
                        self.send_message(message)?;
                    }
                    MessageResponse::LearnedRoutes(routes) => {
                        self.tx.unbounded_send(routes).unwrap();
                        // return Ok(Async::Ready(routes));
                        ()
                    }
                    _ => (),
                }
                // .map_err(|e| SessionError {
                //     reason: e.to_string(),
                //     peer: Some(self.reset_peer()),
                // })
                // .ok();
                self.hold_timer.received_update();
            } else {
                // Before the Session is dropped, send peer back to idle peers
                return Err(SessionError {
                    reason: format!(
                        "Session ended with {} [{}]",
                        self.peer.remote_id.router_id.unwrap(),
                        self.peer.addr
                    ),
                    peer: Some(self.reset_peer()),
                });
            }
        }

        // Check for routes that need to be advertised
        // TODO: How to have newly added routes trigger this future to run & advertise
        //       versus current behavior of waiting for a packet or hold_time
        // if let Ok(db) = DB::new() {
        //     let res = db
        //         .get_pending_routes_for_peer(self.peer.remote_id.router_id.unwrap())
        //         .map(|routes| {
        //             // TODO: Group routes into common attributes and send in groups
        //             for mut route in routes {
        //                 // TODO: Modify route for next hop, Origin, Etc...

        //                 trace!("Sending route for {} to {}", route.prefix, self.peer.addr);
        //                 let res = self
        //                     .send_message(Message::Update(self.peer.create_update(&route)))
        //                     .and_then(|_| {
        //                         route.state = RouteState::Advertised(Utc::now());
        //                         db.update_route(&route)
        //                             .map_err(|err| Error::new(ErrorKind::BrokenPipe, err))
        //                     });
        //                 if let Err(err) = res {
        //                     warn!(
        //                         "Error advertising route to {}: {}",
        //                         self.peer.remote_id.router_id.unwrap(),
        //                         err
        //                     );
        //                 }
        //             }
        //         });
        //     if let Err(err) = res {
        //         warn!(
        //             "Error getting pending routes for {}: {}",
        //             self.peer.remote_id.router_id.unwrap(),
        //             err
        //         );
        //     }
        // }

        // Check for hold time expiration (send keepalive if not expired)
        while let Ok(Async::Ready(_)) = self.hold_timer.poll() {
            if self.hold_timer.get_hold_time() == Duration::seconds(0) {
                return Err(SessionError {
                    reason: format!("Hold Time Expired [{}]: {}", self.hold_timer, self.peer),
                    peer: Some(self.reset_peer()),
                });
            } else if self.hold_timer.should_send_keepalive() {
                self.send_message(Message::KeepAlive)
                    .map_err(|e| SessionError {
                        reason: e.to_string(),
                        peer: Some(self.reset_peer()),
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
    pub peer: Option<Peer>,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let peer = if let Some(peer) = &self.peer {
            format!("{}", peer)
        } else {
            "No Peer".to_string()
        };
        write!(f, "Session Error: {}: {}", self.reason, peer)
    }
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        SessionError {
            reason: error.to_string(),
            peer: None,
        }
    }
}
