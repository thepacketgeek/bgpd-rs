use bgp_rs::{Identifier, NLRIEncoding, PathAttribute, Update, AFI, SAFI};

use crate::rib::Family;
use crate::session::SessionError;

pub fn parse_update(
    update: Update,
) -> Result<(Vec<PathAttribute>, Family, Vec<NLRIEncoding>), SessionError> {
    let attributes = update.attributes.clone();
    let mut family = Family::new(AFI::IPV4, SAFI::Unicast); // BGP4 default

    let nlri: Vec<NLRIEncoding> = if !update.announced_routes.is_empty() {
        update.announced_routes.clone()
    } else if let Some(mp_reach_nlri) = update.get(Identifier::MP_REACH_NLRI) {
        match mp_reach_nlri {
            PathAttribute::MP_REACH_NLRI(nlri) => {
                family = Family::new(nlri.afi, nlri.safi);
                if family.safi == SAFI::Unicast && nlri.next_hop.is_empty() {
                    return Err(SessionError::TransportError(String::from(
                        "Invalid Next-hop on MPReachNLRI",
                    )));
                }
                nlri.announced_routes.iter().cloned().collect::<Vec<_>>()
            }
            _ => unreachable!(),
        }
    } else {
        vec![]
    };

    Ok((attributes, family, nlri))
}
