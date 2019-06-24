use std::fmt;
use std::io::Error;
use std::time::Instant;

use bgp_rs::Message;
use futures::sync::mpsc;
use futures::{Async, Poll, Stream};
use log::{debug, trace, warn};
use tokio::prelude::*;

use crate::codec::MessageProtocol;
use crate::peer::{Peer, PeerState};

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
            self.protocol
                .start_send(Message::Open(self.peer.create_open()))
                .and_then(|_| self.protocol.poll_complete())?;
            self.peer.update_state(PeerState::Established);
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
