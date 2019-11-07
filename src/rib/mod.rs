mod attributes;
pub mod community;
mod export;
pub mod families;
mod parse;
pub mod session;

use attributes::PathAttributeCache;
pub use attributes::{PathAttributeGroup, PathAttributes};
pub use community::{Community, CommunityList};
pub use export::{ExportEntry, ExportedUpdate};
pub use families::{Families, Family};

use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use bgp_rs::{Identifier, NLRIEncoding, PathAttribute, Update};
use chrono::{DateTime, Utc};
use log::debug;

use crate::session::SessionError;

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

/// RIB-internal storage of attrs and NLRI info
#[derive(Debug)]
struct RibEntry {
    family: Family,
    source: EntrySource,
    timestamp: DateTime<Utc>,
    nlri: NLRIEncoding,
}

/// Routing-information Base
/// Contains all received NLRI information with associated Path Attributes
/// and provides an API to query:
///   - routes learned (from a peer, config, or API)
///   - routes that should be advertised to a peer
#[derive(Debug)]
pub struct RIB {
    /// Learned Rib entries, keyed by the PathAttributeGroup hash
    entries: HashMap<u64, Vec<RibEntry>>,
    /// Cache for grouping and storing common PathAttributes amongst NLRI
    cache: PathAttributeCache,
}

impl RIB {
    pub fn new() -> Self {
        Self {
            entries: HashMap::with_capacity(64),
            cache: PathAttributeCache::with_capacity(64),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    pub fn get_routes(&self) -> Vec<Arc<ExportEntry>> {
        self.entries
            .iter()
            .map(|(group_key, entries)| {
                let attributes = {
                    let group = self.cache.get(&group_key).expect("Cached PAs exist");
                    Arc::new(PathAttributes::from_group(&group))
                };
                entries
                    .iter()
                    .map(|e| Arc::new((e, attributes.clone()).into()))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect()
    }

    pub fn get_routes_from_peer(&self, peer: IpAddr) -> Vec<Arc<ExportEntry>> {
        self.entries
            .iter()
            .map(|(group_key, entries)| entries.iter().map(|e| (group_key, e)).collect::<Vec<_>>())
            .flatten()
            .filter(|(_, e)| e.source == EntrySource::Peer(peer))
            .map(|(group_key, e)| {
                let attributes = {
                    let group = self.cache.get(&group_key).expect("Cached PAs exist");
                    Arc::new(PathAttributes::from_group(&group))
                };
                Arc::new((e, attributes.clone()).into())
            })
            .collect()
    }

    pub fn get_routes_for_peer(&self, peer: IpAddr) -> Vec<Arc<ExportEntry>> {
        // TODO: accept some kind of policy object to determine which routes
        //       a peer should receive. for now, just broadcast all that weren't
        //       learned from the peer
        self.entries
            .iter()
            .map(|(group_key, entries)| entries.iter().map(|e| (group_key, e)).collect::<Vec<_>>())
            .flatten()
            .filter(|(_, e)| e.source != EntrySource::Peer(peer))
            .map(|(group_key, e)| {
                let attributes = {
                    let group = self.cache.get(&group_key).expect("Cached PAs exist");
                    Arc::new(PathAttributes::from_group(&group))
                };
                Arc::new((e, attributes.clone()).into())
            })
            .collect()
    }

    pub fn update_from_peer(&mut self, peer: IpAddr, update: Update) -> Result<(), SessionError> {
        let mp_withdraws: Vec<&NLRIEncoding> = update
            .get(Identifier::MP_UNREACH_NLRI)
            .map(|attr| match attr {
                PathAttribute::MP_UNREACH_NLRI(nlri) => nlri.withdrawn_routes.iter().collect(),
                _ => unreachable!(),
            })
            .unwrap_or_else(|| vec![]);
        let withdraws: Vec<&NLRIEncoding> = mp_withdraws
            .into_iter()
            .chain(update.withdrawn_routes.iter())
            .collect();
        if !withdraws.is_empty() {
            self.withdraw_peer_nlri(peer, withdraws);
        }
        let (attributes, family, nlri) = parse::parse_update(update)?;
        let group_key = self.cache.insert(attributes);
        let entry = self
            .entries
            .entry(group_key)
            .or_insert(Vec::with_capacity(nlri.len()));
        entry.extend(nlri.into_iter().map(|nlri| RibEntry {
            source: EntrySource::Peer(peer),
            family,
            timestamp: Utc::now(),
            nlri,
        }));
        Ok(())
    }

    pub fn insert_from_api(
        &mut self,
        family: Family,
        attributes: Vec<PathAttribute>,
        nlri: NLRIEncoding,
    ) -> Arc<ExportEntry> {
        let group_key = self.cache.insert(attributes);
        let entry = self
            .entries
            .entry(group_key)
            .or_insert(Vec::with_capacity(1));
        entry.push(RibEntry {
            source: EntrySource::Config,
            family,
            timestamp: Utc::now(),
            nlri,
        });
        let e = entry.last().expect("Pushed entry exists");
        let attributes = {
            let group = self.cache.get(&group_key).expect("Cached PAs exist");
            Arc::new(PathAttributes::from_group(&group))
        };
        Arc::new((e, attributes.clone()).into())
    }

    pub fn insert_from_config(
        &mut self,
        family: Family,
        attributes: Vec<PathAttribute>,
        nlri: NLRIEncoding,
    ) {
        let group_key = self.cache.insert(attributes);
        let entry = self
            .entries
            .entry(group_key)
            .or_insert(Vec::with_capacity(1));
        entry.push(RibEntry {
            source: EntrySource::Config,
            family,
            timestamp: Utc::now(),
            nlri,
        });
    }

    pub fn remove_from_peer(&mut self, peer: IpAddr) {
        let total: usize = self
            .entries
            .values_mut()
            .map(|entries| {
                entries
                    .drain_filter(|e| e.source == EntrySource::Peer(peer))
                    .count()
            })
            .sum();
        self.cleanup();
        debug!("Removed {} routes from RIB for {}", total, peer);
    }

    pub fn withdraw_peer_nlri(&mut self, peer: IpAddr, withdrawn: Vec<&NLRIEncoding>) {
        // TODO: Optimize this, possibly with an index of IP -> PA Group mapping?
        let mut total = 0usize;
        for nlri in withdrawn {
            total += self
                .entries
                .values_mut()
                .map(|entries| {
                    entries
                        .drain_filter(|e| e.source == EntrySource::Peer(peer) && &e.nlri == nlri)
                        .count()
                })
                .sum::<usize>();
        }
        self.cleanup();
        debug!("Withdrew {} routes for {}", total, peer);
    }

    /// Maintentance cleanup of PathAttributeGroups
    ///   - May be due to sessions ending, withdrawn routes, etc..
    fn cleanup(&mut self) {
        let mut empty_groups: Vec<u64> = vec![];
        self.entries.retain(|&k, v| {
            if v.is_empty() {
                empty_groups.push(k);
                false
            } else {
                true
            }
        });
        for empty in empty_groups {
            self.cache.remove(&empty);
        }
    }
}
