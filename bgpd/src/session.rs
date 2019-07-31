use std::fmt;
use std::io::Error;

use bgp_rs::Message;
use bgpd_lib::codec::MessageProtocol;
use bgpd_lib::db::{PeerStatus, RouteDB};
use bgpd_lib::peer::{MessageCounts, Peer, PeerState};
use bgpd_lib::utils::format_elapsed_time;
use chrono::{DateTime, Duration, Utc};
use futures::{Async, Poll, Stream};
use log::{debug, error, trace, warn};
use tokio::prelude::*;

struct HoldTimer {
    hold_timer: u16,
    last_update: DateTime<Utc>,
}

impl HoldTimer {
    fn new(hold_timer: u16) -> HoldTimer {
        HoldTimer {
            hold_timer,
            last_update: Utc::now(),
        }
    }

    // Calculate the interval for sending keepalives
    fn get_keepalive_timer(&self) -> Duration {
        Duration::seconds((self.hold_timer / 3).into())
    }

    // Calculate remaining hold time available
    // Counts down from self.hold_timer to 0
    // Will never be less than 0, at which the peer hold time has expired
    fn get_hold_time(&self) -> Duration {
        let hold_time = Duration::seconds(self.hold_timer.into());
        if Utc::now().signed_duration_since(self.last_update) > hold_time {
            Duration::seconds(0)
        } else {
            hold_time - Utc::now().signed_duration_since(self.last_update)
        }
    }

    // Calculate if Keepalive message should be sent
    // Returns true when:
    //    Hold time remaining is less than 2/3 of the total hold_timer
    //    which is 2x the Keepalive timer
    fn should_send_keepalive(&self) -> bool {
        self.get_hold_time().num_seconds() < (2 * self.get_keepalive_timer().num_seconds())
    }

    fn received_update(&mut self) {
        self.last_update = Utc::now();
    }
}

impl Future for HoldTimer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Error> {
        if self.should_send_keepalive() {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl fmt::Display for HoldTimer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_elapsed_time(self.get_hold_time()))
    }
}

pub struct Session {
    peer: Box<Peer>,
    protocol: MessageProtocol,
    connect_time: DateTime<Utc>,
    hold_timer: HoldTimer,
    counts: MessageCounts,
}

impl Session {
    pub fn new(peer: Peer, protocol: MessageProtocol, hold_timer: u16) -> Session {
        let mut counts = MessageCounts::new();
        // Somewhat hacky, but this counts for
        counts.increment_received();
        Session {
            peer: Box::new(peer),
            protocol,
            connect_time: Utc::now(),
            hold_timer: HoldTimer::new(hold_timer),
            counts,
        }
    }

    // Send a message, and flush the send buffer afterwards
    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        debug!("[{}] Outgoing: {:?}", self.peer.addr, message);
        self.protocol
            .start_send(message)
            .and_then(|_| self.protocol.poll_complete())?;
        self.counts.increment_sent();
        Ok(())
    }

    fn update_peer_status(&self) {
        RouteDB::new()
            .and_then(|db| {
                let status = PeerStatus {
                    neighbor: self.peer.addr,
                    router_id: self.peer.remote_id.router_id,
                    asn: self.peer.remote_id.asn,
                    msg_received: Some(self.counts.received()),
                    msg_sent: Some(self.counts.sent()),
                    connect_time: Some(self.connect_time),
                    state: self.peer.get_state(),
                };
                db.update_peer(&status)
            })
            .map_err(|err| error!("{:?}", err))
            .ok();
    }

    // Prep the peer to send back to the Idle Peers HashMap
    // In order to do that, reset the Session's Peer with an empty Peer struct
    fn reset_peer(&mut self) -> Peer {
        RouteDB::new()
            .and_then(|db| db.remove_routes_for_peer(self.peer.remote_id.router_id.unwrap()))
            .ok();
        let mut peer = std::mem::replace(&mut self.peer, Box::new(Peer::default()));
        peer.update_state(PeerState::Idle);
        *peer
    }
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Session {} uptime={} hold_time={}>",
            self.protocol.get_ref().peer_addr().unwrap(),
            format_elapsed_time(Utc::now().signed_duration_since(self.connect_time)),
            self.hold_timer,
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
    type Item = Peer;
    type Error = SessionError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        trace!("Polling {}", self);

        if let PeerState::OpenConfirm = self.peer.get_state() {
            self.send_message(Message::Open(self.peer.create_open()))
                .map_err(|e| SessionError {
                    error: e,
                    peer: Some(self.reset_peer()),
                })
                .ok();
            self.peer.update_state(PeerState::Established);
        }

        // Read new messages from the socket
        while let Ok(Async::Ready(data)) = self.protocol.poll() {
            if let Some(message) = data {
                debug!("[{}] Incoming: {:?}", self.peer.addr, message);
                self.counts.increment_received();
                self.peer
                    .process_message(message)
                    .and_then(|resp| {
                        if let Some(data) = resp {
                            self.send_message(data)?;
                        }
                        Ok(())
                    })
                    .map_err(|e| SessionError {
                        error: e,
                        peer: Some(self.reset_peer()),
                    })
                    .ok();
                self.hold_timer.received_update();
            } else {
                warn!(
                    "Session ended with {} [{}]",
                    self.peer.remote_id.router_id.unwrap(),
                    self.peer.addr
                );
                // Before the Session is dropped, send peer back to idle peers
                let peer = self.reset_peer();
                return Ok(Async::Ready(peer));
            }
        }

        // Check for hold time expiration (send keepalive if not expired)
        while let Ok(Async::Ready(_)) = self.hold_timer.poll() {
            if self.hold_timer.get_hold_time() == Duration::seconds(0) {
                trace!("Hold Time Expired [{}]: {}", self.hold_timer, self.peer);
                let peer = self.reset_peer();
                return Ok(Async::Ready(peer));
            } else if self.hold_timer.should_send_keepalive() {
                self.send_message(Message::KeepAlive)
                    .map_err(|e| SessionError {
                        error: e,
                        peer: Some(self.reset_peer()),
                    })
                    .ok();
            }
        }

        self.update_peer_status();
        trace!("Finished polling {}", self);
        Ok(Async::NotReady)
    }
}

#[derive(Debug)]
pub struct SessionError {
    pub error: Error,
    pub peer: Option<Peer>,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let peer = if let Some(peer) = &self.peer {
            format!("{}", peer)
        } else {
            "No Peer".to_string()
        };
        write!(f, "Session Error: {:?}: {}", self.error, peer)
    }
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        SessionError { error, peer: None }
    }
}
