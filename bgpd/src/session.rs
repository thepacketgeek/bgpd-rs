use std::fmt;
use std::io::{Error, ErrorKind};
use std::net::IpAddr;

use bgp_rs::{Identifier, Message, NLRIEncoding, PathAttribute};
use bgpd_lib::utils::{format_elapsed_time, format_time_as_elapsed, get_elapsed_time};
use chrono::{DateTime, Duration, Utc};
use futures::{Async, Poll, Stream};
use log::{error, trace, warn};
use tokio::prelude::*;

use crate::codec::MessageProtocol;
use crate::db::DB;
use crate::models::{Community, CommunityList, MessageCounts, Peer, PeerState, PeerSummary, Route};

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
        if get_elapsed_time(self.last_update) > hold_time {
            Duration::seconds(0)
        } else {
            hold_time - get_elapsed_time(self.last_update)
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
        trace!("[{}] Outgoing: {:?}", self.peer.addr, message);
        self.protocol
            .start_send(message)
            .and_then(|_| self.protocol.poll_complete())?;
        self.counts.increment_sent();
        Ok(())
    }

    fn update_peer_summary(&self) {
        DB::new()
            .and_then(|db| {
                let summary = PeerSummary {
                    neighbor: self.peer.addr,
                    router_id: self.peer.remote_id.router_id,
                    asn: self.peer.remote_id.asn,
                    msg_received: Some(self.counts.received()),
                    msg_sent: Some(self.counts.sent()),
                    connect_time: Some(self.connect_time),
                    state: self.peer.get_state(),
                    prefixes_received: None,
                };
                db.update_peer(&summary)
            })
            .map_err(|err| error!("{:?}", err))
            .ok();
    }

    // Prep the peer to send back to the Idle Peers HashMap
    // In order to do that, reset the Session's Peer with an empty Peer struct
    fn reset_peer(&mut self) -> Peer {
        DB::new()
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
            format_time_as_elapsed(self.connect_time),
            self.hold_timer,
        )
    }
}

/// This is where a connected peer is managed.
///
/// A `Session` is also a future for processing BGP messages and
/// handling peer timeouts
///
/// When a `Session` is created, the first OPEN message has already been read.
/// When the socket closes, the `Session` completes and the Peer struct
/// is returned back to the Idle Peers (for future connections)
///
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
                trace!("[{}] Incoming: {:?}", self.peer.addr, message);
                self.counts.increment_received();
                process_message(&mut self.peer, message)
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

        self.update_peer_summary();
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

pub fn process_message(peer: &mut Peer, message: Message) -> Result<Option<Message>, Error> {
    trace!("{}: {:?}", peer.remote_id, message);
    let response = match message {
        Message::KeepAlive => Some(Message::KeepAlive),
        Message::Update(update) => {
            if update.is_announcement() {
                let origin = update
                    .get(Identifier::ORIGIN)
                    .map(|attr| {
                        if let PathAttribute::ORIGIN(origin) = attr {
                            origin.clone()
                        } else {
                            unreachable!()
                        }
                    })
                    .expect("ORIGIN must be present in Update");
                let next_hop = update
                    .get(Identifier::NEXT_HOP)
                    .map(|attr| {
                        if let PathAttribute::NEXT_HOP(next_hop) = attr {
                            *next_hop
                        } else {
                            unreachable!()
                        }
                    })
                    .expect("NEXT_HOP must be present in Update");
                let as_path = update
                    .get(Identifier::AS_PATH)
                    .map(|attr| {
                        if let PathAttribute::AS_PATH(as_path) = attr {
                            as_path.clone()
                        } else {
                            unreachable!()
                        }
                    })
                    .expect("AS_PATH must be present in Update");
                let local_pref = update
                    .get(Identifier::LOCAL_PREF)
                    .map(|attr| {
                        if let PathAttribute::LOCAL_PREF(local_pref) = attr {
                            Some(*local_pref)
                        } else {
                            unreachable!()
                        }
                    })
                    .unwrap_or(None);
                let multi_exit_disc = update
                    .get(Identifier::MULTI_EXIT_DISC)
                    .map(|attr| {
                        if let PathAttribute::MULTI_EXIT_DISC(metric) = attr {
                            Some(*metric)
                        } else {
                            unreachable!()
                        }
                    })
                    .unwrap_or(None);
                let communities = update
                    .get(Identifier::COMMUNITY)
                    .map(|attr| {
                        if let PathAttribute::COMMUNITY(communities) = attr {
                            communities
                                .iter()
                                .map(|c| Community::STANDARD(*c))
                                .collect::<Vec<Community>>()
                        } else {
                            unreachable!()
                        }
                    })
                    .unwrap_or_else(|| vec![]);

                let ext_communities = update
                    .get(Identifier::EXTENDED_COMMUNITIES)
                    .map(|attr| {
                        if let PathAttribute::EXTENDED_COMMUNITIES(communities) = attr {
                            communities
                                .iter()
                                .map(|c| Community::EXTENDED(*c))
                                .collect::<Vec<Community>>()
                        } else {
                            unreachable!()
                        }
                    })
                    .unwrap_or_else(|| vec![]);

                let community_list = CommunityList::new(
                    communities
                        .into_iter()
                        .chain(ext_communities.into_iter())
                        .collect(),
                );

                let routes: Vec<Route> = update
                    .announced_routes
                    .iter()
                    .map(|route| match route {
                        NLRIEncoding::IP(prefix) => Some(prefix),
                        _ => None,
                    })
                    .filter(std::option::Option::is_some)
                    .map(std::option::Option::unwrap)
                    .map(|prefix| {
                        let addr = IpAddr::from(prefix);
                        Route {
                            received_from: peer.remote_id.router_id.unwrap(),
                            received_at: Utc::now(),
                            prefix: addr,
                            next_hop,
                            origin: origin.clone(),
                            as_path: as_path.clone(),
                            local_pref,
                            multi_exit_disc,
                            communities: community_list.clone(),
                        }
                    })
                    .collect();
                DB::new().and_then(|db| db.insert_routes(routes)).ok();
            }
            if update.is_withdrawal() {
                DB::new()
                    .and_then(|db| {
                        db.remove_prefixes_from_peer(
                            peer.remote_id.router_id.unwrap(),
                            &update.withdrawn_routes,
                        )
                    })
                    .ok();
            }
            None
        }
        Message::Notification(notification) => {
            warn!("{} NOTIFICATION: {}", peer.remote_id, notification);
            None
        }
        Message::RouteRefresh(_) => None,
        _ => {
            warn!("{} Unexpected message {:?}", peer.remote_id, message);
            return Err(Error::from(ErrorKind::InvalidInput));
        }
    };
    Ok(response)
}
