use std::net::IpAddr;
use std::sync::Arc;

use bgp_rs::{ASPath, Identifier, Origin, PathAttribute, Update, AFI, SAFI};

use crate::rib::{Community, CommunityList, Family, PathAttributes, StoredUpdate};

pub fn parse_update(update: Update) -> Vec<StoredUpdate> {
    let origin = update
        .get(Identifier::ORIGIN)
        .map(|attr| {
            if let PathAttribute::ORIGIN(origin) = attr {
                origin.clone()
            } else {
                unreachable!()
            }
        })
        .unwrap_or(Origin::INCOMPLETE);
    let next_hop = update
        .get(Identifier::NEXT_HOP)
        .map(|attr| {
            if let PathAttribute::NEXT_HOP(next_hop) = attr {
                Some(*next_hop)
            } else {
                None
            }
        })
        .unwrap_or(None);
    let as_path = update
        .get(Identifier::AS_PATH)
        .map(|attr| {
            if let PathAttribute::AS_PATH(as_path) = attr {
                as_path.clone()
            } else {
                unreachable!()
            }
        })
        .unwrap_or_else(|| ASPath { segments: vec![] });
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

    let community_list = CommunityList(
        communities
            .into_iter()
            .chain(ext_communities.into_iter())
            .collect(),
    );

    let mut attributes = Arc::new(PathAttributes {
        next_hop,
        origin,
        as_path,
        local_pref,
        multi_exit_disc,
        communities: community_list,
    });

    let announced_nlri: Vec<StoredUpdate> = update
        .announced_routes
        .iter()
        .map(|nlri| StoredUpdate {
            family: Family::new(AFI::IPV4, SAFI::Unicast), // BGP4 default
            attributes: attributes.clone(),
            nlri: nlri.clone(),
        })
        .collect();
    let mut mp_nlri: Vec<StoredUpdate> = vec![];
    if let Some(mp_reach_nlri) = update.get(Identifier::MP_REACH_NLRI) {
        if let PathAttribute::MP_REACH_NLRI(nlri) = mp_reach_nlri {
            if !nlri.next_hop.is_empty() {
                let next_hop = bytes_to_ipv6(nlri.next_hop.clone());
                attributes = Arc::new(PathAttributes {
                    next_hop: Some(next_hop),
                    ..Arc::try_unwrap(attributes).unwrap()
                });
            }
            mp_nlri.extend(
                nlri.announced_routes
                    .iter()
                    .map(|route| StoredUpdate {
                        family: Family::new(nlri.afi, nlri.safi),
                        attributes: attributes.clone(),
                        nlri: route.clone(),
                    })
                    .collect::<Vec<_>>(),
            );
        } else {
            unreachable!()
        }
    }
    announced_nlri.into_iter().chain(mp_nlri).collect()
}

/// Convert first 16 bytes (1 IPv6 address) to IpAddr
/// TODO: Handle multiple next hops
///       Can they be variable length?
fn bytes_to_ipv6(bytes: Vec<u8>) -> IpAddr {
    let mut buffer: [u8; 16] = [0; 16];
    buffer[..16].clone_from_slice(&bytes[..16]);
    IpAddr::from(buffer)
}
