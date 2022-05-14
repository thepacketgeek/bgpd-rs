mod codec;
mod hold_timer;
mod manager;
mod message_counts;
mod poller;
mod lib;

use std::convert::From;
use std::error;
use std::fmt;
use std::io;
use std::net::IpAddr;

use hold_timer::HoldTimer;
pub use manager::SessionManager;
use message_counts::MessageCounts;
use poller::{Poller, PollerTx};
pub use lib::Session;

use bgp_rs::Update;

#[derive(Debug)]
pub enum SessionUpdate {
    // Update received from a peer (PeerIP, Update)
    Learned((IpAddr, Update)),
    // Sessions are ended, clear RIB for these peers
    Ended(Vec<IpAddr>),
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SessionState {
    Connect,
    Active,
    Idle,
    OpenSent,
    OpenConfirm,
    Established,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let word = match self {
            SessionState::Connect => "Connect",
            SessionState::Active => "Active",
            SessionState::Idle => "Idle",
            SessionState::OpenSent => "OpenSent",
            SessionState::OpenConfirm => "OpenConfirm",
            SessionState::Established => "Established",
        };
        write!(f, "{}", word)
    }
}

#[derive(Debug)]
pub enum SessionError {
    /// Peer De-configured
    Deconfigured,
    /// Received an unexpected ASN. [received, expected]
    OpenAsnMismatch(u32, u32),
    /// Finite State Machine error, unexpected transition [minor_err_codes]
    FiniteStateMachine(u8),
    /// Hold time expired. [interval]
    HoldTimeExpired(u16),
    /// Something happened in transport. [reason]
    TransportError(String),
    /// Some other issue happened. [reason]
    Other(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Session Error: ")?;
        use SessionError::*;
        match self {
            Deconfigured => write!(f, "Peer De-configured")?,
            OpenAsnMismatch(r, e) => {
                write!(f, "Open ASN Mismatch (received={}, expected={})", r, e)?;
            }
            HoldTimeExpired(h) => write!(f, "Hold time expired after {} seconds", h)?,
            FiniteStateMachine(minor) => write!(f, "Finite State Machine err [{}]", minor)?,
            TransportError(r) => write!(f, "Transport error [{}]", r)?,
            Other(r) => write!(f, "{}", r)?,
        }
        Ok(())
    }
}

impl From<io::Error> for SessionError {
    fn from(error: io::Error) -> Self {
        SessionError::TransportError(error.to_string())
    }
}

impl error::Error for SessionError {
    fn description(&self) -> &str {
        "Session Error"
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}
