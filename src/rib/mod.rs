pub mod community;
pub mod families;
mod parse;
pub mod session;

pub use community::{Community, CommunityList};
pub use families::{Families, Family};

use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use bgp_rs::{ASPath, NLRIEncoding, Origin, Update};
use chrono::{DateTime, Utc};
use log::trace;

use crate::utils::format_time_as_elapsed;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum EntrySource {
    Api,
    Config,
    Peer(IpAddr),
}

impl fmt::Display for EntrySource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use EntrySource::*;
        let display = match self {
            Api => "API".to_string(),
            Config => "Config".to_string(),
            Peer(addr) => addr.to_string(),
        };
        write!(f, "{}", display)
    }
}

#[derive(Debug)]
pub struct Entry {
    // Time received
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) update: StoredUpdate,
    pub(crate) source: EntrySource,
}

impl Entry {
    pub fn new(update: StoredUpdate, source: EntrySource) -> Self {
        Self {
            timestamp: Utc::now(),
            update,
            source,
        }
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Entry source={} age={}>",
            self.source,
            format_time_as_elapsed(self.timestamp),
        )
    }
}

#[derive(Debug)]
pub struct RIB {
    entries: Vec<Arc<Entry>>,
}

impl RIB {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(64),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get_routes(&self) -> Vec<Arc<Entry>> {
        self.entries.iter().map(|e| e.clone()).collect()
    }

    pub fn get_routes_from_peer(&self, peer: IpAddr) -> Vec<Arc<Entry>> {
        self.entries
            .iter()
            .filter(|e| e.source == EntrySource::Peer(peer))
            .map(|e| e.clone())
            .collect()
    }

    pub fn get_routes_for_peer(&self, peer: IpAddr) -> Vec<Arc<Entry>> {
        // TODO: accept some kind of policy object to determine which routes
        //       a peer should receive. for now, just broadcast all that weren't
        //       learned from the peer
        self.entries
            .iter()
            .filter(|e| e.source != EntrySource::Peer(peer))
            .map(|e| e.clone())
            .collect()
    }

    pub fn insert_from_peer(&mut self, peer: IpAddr, update: Update) {
        // TODO, delete from RIB
        // if !update.withdrawn_routes.is_empty() {
        // }
        // if !update.announced_routes.is_empty() {
        let updates = parse::parse_update(update);
        self.entries.extend(
            updates
                .into_iter()
                .map(|u| Arc::new(Entry::new(u, EntrySource::Peer(peer)))),
        );
        // }
    }
    pub fn insert_from_api(&mut self, update: StoredUpdate) -> Arc<Entry> {
        let entry = Arc::new(Entry::new(update, EntrySource::Api));
        self.entries.push(entry.clone());
        entry
    }
    pub fn insert_from_config(&mut self, update: StoredUpdate) -> Arc<Entry> {
        let entry = Arc::new(Entry::new(update, EntrySource::Config));
        self.entries.push(entry.clone());
        entry
    }

    pub fn remove_from_peer(&mut self, peer: IpAddr) {
        let count = self
            .entries
            .drain_filter(|e| e.source == EntrySource::Peer(peer))
            .count();
        trace!("Removed {} routes from RIB for {}", count, peer);
    }
}

#[derive(Debug)]
pub struct PathAttributes {
    pub next_hop: Option<IpAddr>,
    pub origin: Origin,
    pub as_path: ASPath,
    pub local_pref: Option<u32>,
    pub multi_exit_disc: Option<u32>,
    pub communities: CommunityList,
}

#[derive(Debug)]
pub struct StoredUpdate {
    pub family: Family,
    // TODO: Hash/Cache these (for space savings & grouping of PAs across NLRI)
    pub attributes: Arc<PathAttributes>,
    pub nlri: NLRIEncoding,
}
