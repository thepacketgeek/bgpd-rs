use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::Hasher;
use std::net::IpAddr;

use bgp_rs::{ASPath, Identifier, Origin, PathAttribute, AFI, SAFI};
use itertools::Itertools;

use crate::rib::{Community, CommunityList};
use crate::utils::bytes_to_ipv6;

#[derive(Debug)]
pub struct PathAttributeCache(HashMap<u64, PathAttributeGroup>);

impl PathAttributeCache {
    pub fn with_capacity(size: usize) -> Self {
        Self(HashMap::with_capacity(size))
    }

    pub fn insert(&mut self, attrs: Vec<PathAttribute>) -> u64 {
        let group = PathAttributeGroup::from_attributes(attrs);
        let hash = group.hash();
        self.0.insert(hash, group);
        hash
    }

    pub fn get(&self, key: u64) -> Option<&PathAttributeGroup> {
        self.0.get(&key)
    }

    /// Cleanup a PathAttributeGroup with no more associated entries
    pub(super) fn remove(&mut self, key: u64) {
        self.0.remove(&key);
    }
}

#[derive(Debug)]
pub struct PathAttributeGroup(HashMap<Identifier, PathAttribute>);

impl PathAttributeGroup {
    pub fn from_attributes(attributes: Vec<PathAttribute>) -> Self {
        Self(
            attributes
                .into_iter()
                .map(|attr| (attr.id(), attr))
                .collect(),
        )
    }

    pub fn get(&self, identifier: Identifier) -> Option<&PathAttribute> {
        self.0.get(&identifier)
    }

    /// Hash contained PathAttributes using the encoded bytes
    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut bytes: Vec<u8> = Vec::with_capacity(8 * self.0.len());
        for attr in self
            .0
            .values()
            // Sort by identifier ID for consistent hashing
            .sorted_by(|a, b| Ord::cmp(&(a.id() as u8), &(b.id() as u8)))
        {
            attr.encode(&mut bytes).expect("Can't hash PathAttribute");
            hasher.write(&bytes);
        }
        hasher.finish()
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

impl PathAttributes {
    pub fn from_group(group: &PathAttributeGroup) -> Self {
        let origin = group
            .get(Identifier::ORIGIN)
            .map(|attr| match attr {
                PathAttribute::ORIGIN(origin) => origin.clone(),
                _ => unreachable!(),
            })
            .unwrap_or(Origin::INCOMPLETE);
        let next_hop = group
            // Check for IPv6 first in MPReachNLRI
            .get(Identifier::MP_REACH_NLRI)
            .map(|attr| match attr {
                PathAttribute::MP_REACH_NLRI(nlri) => {
                    if (nlri.afi, nlri.safi) == (AFI::IPV6, SAFI::Unicast) {
                        Some(bytes_to_ipv6(&nlri.next_hop))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            // Fallback to IPv4:Unicast
            .unwrap_or_else(|| {
                group.get(Identifier::NEXT_HOP).map(|attr| match attr {
                    PathAttribute::NEXT_HOP(next_hop) => *next_hop,
                    _ => unreachable!(),
                })
            });
        let as_path = group
            .get(Identifier::AS_PATH)
            .map(|attr| match attr {
                PathAttribute::AS_PATH(as_path) => as_path.clone(),
                _ => unreachable!(),
            })
            .unwrap_or_else(|| ASPath { segments: vec![] });
        let local_pref = group
            .get(Identifier::LOCAL_PREF)
            .map(|attr| match attr {
                PathAttribute::LOCAL_PREF(local_pref) => Some(*local_pref),
                _ => unreachable!(),
            })
            .unwrap_or(None);
        let multi_exit_disc = group
            .get(Identifier::MULTI_EXIT_DISC)
            .map(|attr| match attr {
                PathAttribute::MULTI_EXIT_DISC(metric) => Some(*metric),
                _ => unreachable!(),
            })
            .unwrap_or(None);
        let communities = group
            .get(Identifier::COMMUNITY)
            .map(|attr| match attr {
                PathAttribute::COMMUNITY(communities) => communities
                    .iter()
                    .map(|c| Community::STANDARD(*c))
                    .collect::<Vec<Community>>(),
                _ => unreachable!(),
            })
            .unwrap_or_else(|| vec![]);

        let ext_communities = group
            .get(Identifier::EXTENDED_COMMUNITIES)
            .map(|attr| match attr {
                PathAttribute::EXTENDED_COMMUNITIES(communities) => communities
                    .iter()
                    .map(|c| Community::EXTENDED(*c))
                    .collect::<Vec<Community>>(),
                _ => unreachable!(),
            })
            .unwrap_or_else(|| vec![]);

        let community_list = CommunityList(
            communities
                .into_iter()
                .chain(ext_communities.into_iter())
                .collect(),
        );

        PathAttributes {
            next_hop,
            origin,
            as_path,
            local_pref,
            multi_exit_disc,
            communities: community_list,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bgp_rs::Origin;

    #[test]
    fn test_insert_group() {
        let mut cache = PathAttributeCache::with_capacity(2);
        let attrs = vec![
            PathAttribute::ORIGIN(Origin::IGP),
            PathAttribute::NEXT_HOP("1.1.1.1".parse().unwrap()),
        ];
        let attrs_clone = attrs.clone();
        cache.insert(attrs);
        cache.insert(attrs_clone);
        dbg!(&cache);
        assert_eq!(cache.0.len(), 1);
    }
}
