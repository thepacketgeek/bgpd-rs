use std::fmt;
use std::io::Error;
use std::time::{Duration, Instant};

use bgp_rs::Message;
use futures::sync::mpsc;
use futures::{Async, Poll, Stream};
use log::{debug, trace, warn};
use tokio::prelude::*;

use crate::codec::MessageProtocol;
use crate::peer::{Peer, PeerState};
use crate::utils::format_elapsed_time;

pub type Tx = mpsc::UnboundedSender<Peer>;
pub type Rx = mpsc::UnboundedReceiver<Peer>;

pub struct Channel {
    pub receiver: Rx,
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
            Ok(Async::Ready(peer))
        } else {
            Ok(Async::NotReady)
        }
    }
}

struct HoldTimer {
    hold_timer: u16,
    last_update: Instant,
}

impl HoldTimer {
    fn new(hold_timer: u16) -> HoldTimer {
        HoldTimer {
            hold_timer,
            last_update: Instant::now(),
        }
    }

    // Calculate the interval for sending keepalives
    fn get_keepalive_timer(&self) -> Duration {
        Duration::from_secs((self.hold_timer / 3).into())
    }

    // Calculate remaining hold time available
    // Counts down from self.hold_timer to 0
    // Will never be less than 0, at which the peer hold time has expired
    fn get_hold_time(&self) -> Duration {
        let hold_time = Duration::from_secs(self.hold_timer.into());
        if self.last_update.elapsed() > hold_time {
            Duration::from_secs(0)
        } else {
            hold_time - self.last_update.elapsed()
        }
    }

    // Calculate if Keepalive message should be sent
    // Returns true when:
    //    Hold time remaining is less than 2/3 of the total hold_timer
    //    which is 2x the Keepalive timer
    fn should_send_keepalive(&self) -> bool {
        self.get_hold_time() < (2 * self.get_keepalive_timer())
    }

    fn received_update(&mut self) {
        self.last_update = Instant::now();
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
    connect_time: Instant,
    channel: Tx,
    hold_timer: HoldTimer,
}

impl Session {
    pub fn new(peer: Peer, protocol: MessageProtocol, channel: Tx, hold_timer: u16) -> Session {
        Session {
            peer: Box::new(peer),
            protocol,
            connect_time: Instant::now(),
            channel,
            hold_timer: HoldTimer::new(hold_timer),
        }
    }

    // Send a message, and flush the send buffer afterwards
    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        debug!("[{}] Outgoing: {:?}", self.peer.addr, message);
        self.protocol
            .start_send(message)
            .and_then(|_| self.protocol.poll_complete())?;
        Ok(())
    }

    // Send the peer back to the Idle Peers HashMap via the send channel
    // In order to do that, replace the session peer with an empty Peer struct
    fn replace_peer(&mut self) {
        let mut peer = std::mem::replace(&mut self.peer, Box::new(Peer::default()));
        peer.update_state(PeerState::Idle);
        self.channel
            .start_send(*peer)
            .and_then(|_| self.channel.poll_complete())
            .ok();
    }
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Session {} uptime={} hold_time={}>",
            self.protocol.get_ref().peer_addr().unwrap(),
            format_elapsed_time(self.connect_time.elapsed()),
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
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<(), Error> {
        trace!("Polling {}", self);

        if let PeerState::OpenConfirm = self.peer.get_state() {
            self.send_message(Message::Open(self.peer.create_open()))?;
            self.peer.update_state(PeerState::Established);
        }

        // Read new messages from the socket
        while let Async::Ready(data) = self.protocol.poll()? {
            if let Some(message) = data {
                debug!("[{}] Incoming: {:?}", self.peer.addr, message);
                self.peer.process_message(message).and_then(|resp| {
                    if let Some(data) = resp {
                        self.send_message(data)?;
                    }
                    Ok(())
                })?;
                self.hold_timer.received_update();
            } else {
                warn!("Session ended with {}", self.peer);
                // Before the Session is dropped, send peer back to idle peers
                self.replace_peer();
                return Ok(Async::Ready(()));
            }
        }

        // Check for hold time expiration (send keepalive if not expired)
        while let Async::Ready(_) = self.hold_timer.poll()? {
            if self.hold_timer.get_hold_time() == Duration::from_secs(0) {
                trace!("Hold Time Expired [{}]: {}", self.hold_timer, self.peer);
                self.replace_peer();
                return Ok(Async::Ready(()));
            } else if self.hold_timer.should_send_keepalive() {
                self.send_message(Message::KeepAlive)?;
            }
        }

        trace!("Finished polling {}", self);
        Ok(Async::NotReady)
    }
}
