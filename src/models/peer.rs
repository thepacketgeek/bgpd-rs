use std::cmp;
use std::convert::From;
use std::fmt;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use bgp_rs::{
    Capabilities, Identifier, Message, NLRIEncoding, Open, OpenParameter, PathAttribute, Update,
};
use chrono::Utc;
use log::{debug, warn};
use serde::{Deserialize, Serialize};

use crate::MessageResponse;
use crate::codec::capabilities_from_params;
use crate::models::{Community, CommunityList, Route, RouteState};
use crate::utils::{as_u32_be, asn_to_dotted, transform_u32_to_bytes};

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub enum PeerState {
    Connect,
    Active,
    Idle,
    OpenSent,
    OpenConfirm,
    Established,
}

impl fmt::Display for PeerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let word = match self {
            PeerState::Connect => "Connect",
            PeerState::Active => "Active",
            PeerState::Idle => "Idle",
            PeerState::OpenSent => "OpenSent",
            PeerState::OpenConfirm => "OpenConfirm",
            PeerState::Established => "Established",
        };
        write!(f, "{}", word)
    }
}

impl FromStr for PeerState {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Connect" => Ok(PeerState::Connect),
            "Active" => Ok(PeerState::Active),
            "Idle" => Ok(PeerState::Idle),
            "OpenSent" => Ok(PeerState::OpenSent),
            "OpenConfirm" => Ok(PeerState::OpenConfirm),
            "Established" => Ok(PeerState::Established),
            _ => Err(Error::from(ErrorKind::NotFound)),
        }
    }
}

#[derive(Debug)]
pub struct PeerIdentifier {
    pub router_id: Option<IpAddr>,
    pub asn: u32,
}

impl PeerIdentifier {
    pub fn new(router_id: Option<IpAddr>, asn: u32) -> PeerIdentifier {
        PeerIdentifier { router_id, asn }
    }
}

impl fmt::Display for PeerIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}/{}",
            self.router_id
                .unwrap_or_else(|| IpAddr::from(Ipv4Addr::new(0, 0, 0, 0))),
            asn_to_dotted(self.asn)
        )
    }
}

#[derive(Debug)]
pub struct Peer {
    pub addr: IpAddr,
    pub remote_id: PeerIdentifier,
    local_id: PeerIdentifier, // Server (local side) ID
    state: PeerState,
    passive: bool,
    pub hold_timer: u16,
}

impl Peer {
    pub fn new(
        addr: IpAddr,
        state: PeerState,
        remote_id: PeerIdentifier,
        local_id: PeerIdentifier,
        passive: bool,
        hold_timer: u16,
    ) -> Peer {
        Peer {
            addr,
            state,
            remote_id,
            local_id,
            passive,
            hold_timer,
        }
    }

    pub fn is_passive(&self) -> bool {
        self.passive
    }

    pub fn get_state(&self) -> PeerState {
        self.state
    }

    pub fn update_state(&mut self, new_state: PeerState) {
        let label = if self.remote_id.router_id.is_some() {
            format!("{}", self.remote_id)
        } else {
            format!("{}", self.addr)
        };
        debug!(
            "{} went from {} to {}",
            label,
            self.state.to_string(),
            new_state.to_string()
        );
        self.state = new_state;
    }

    pub fn revert_to_idle(&mut self) {
        self.update_state(PeerState::Idle);
        self.remote_id.router_id = None;
    }

    pub fn open_received(&mut self, open: Open) -> (Capabilities, u16) {
        let (capabilities, remote_asn) = capabilities_from_params(&open.parameters);
        let remote_id = PeerIdentifier::new(
            Some(IpAddr::from(transform_u32_to_bytes(open.identifier))),
            remote_asn.unwrap_or_else(|| u32::from(open.peer_asn)),
        );
        let hold_timer = cmp::min(open.hold_timer, self.hold_timer);
        debug!(
            "[{}] Received OPEN {} [w/ {} params]",
            remote_id,
            self.addr,
            open.parameters.len()
        );
        self.remote_id = remote_id;
        self.update_state(PeerState::OpenConfirm);
        (capabilities, hold_timer)
    }

    pub fn create_open(&self) -> Open {
        let router_id = match self.local_id.router_id {
            Some(IpAddr::V4(ipv4)) => ipv4,
            _ => unreachable!(),
        };
        Open {
            version: 4,
            peer_asn: self.local_id.asn as u16,
            hold_timer: self.hold_timer,
            identifier: as_u32_be(router_id.octets()),
            parameters: vec![
                OpenParameter {
                    // SAFI Ipv4 Unicast
                    param_type: 2,
                    param_length: 6,
                    value: vec![0x01, 0x04, 0x00, 0x01, 0x00, 0x01],
                },
                OpenParameter {
                    // SAFI IPv6 Unicast
                    param_type: 2,
                    param_length: 6,
                    value: vec![0x01, 0x04, 0x00, 0x02, 0x00, 0x01],
                },
                OpenParameter {
                    // 4-byte ASN
                    param_type: 2,
                    param_length: 6,
                    value: vec![0x41, 0x04, 0x00, 0x02, 0xfd, 0xe8],
                },
            ],
        }
    }

    pub fn create_update(&self, route: &Route) -> Update {
        let mut attributes: Vec<PathAttribute> = Vec::with_capacity(4);
        attributes.push(PathAttribute::ORIGIN(route.origin.clone()));
        attributes.push(PathAttribute::NEXT_HOP(route.next_hop));
        if let Some(med) = route.multi_exit_disc {
            attributes.push(PathAttribute::MULTI_EXIT_DISC(med));
        }
        if let Some(pref) = route.local_pref {
            attributes.push(PathAttribute::LOCAL_PREF(pref));
        }

        // TODO:
        // ASPath (adding this peer's ASN if eBGP)
        // COMMMUNITIES

        let announced_routes = vec![NLRIEncoding::IP(route.prefix.clone())];

        Update {
            withdrawn_routes: Vec::new(),
            attributes,
            announced_routes,
        }
    }

    pub fn process_message(&mut self, message: Message) -> Result<MessageResponse, Error> {
        let response = match message {
            Message::Open(open) => {
                let (capabilities, hold_timer) = self.open_received(open);
                MessageResponse::Open((self.create_open(), capabilities, hold_timer))
            }
            Message::KeepAlive => MessageResponse::Message(Message::KeepAlive),
            Message::Update(update) => {
                let router_id = self.remote_id.router_id.unwrap();
                if update.is_announcement() {
                    let announced_routes =
                        process_routes(router_id, &update, &update.announced_routes);
                    MessageResponse::LearnedRoutes(announced_routes)
                } else {
                    MessageResponse::Empty
                }
                // TODO
                // if update.is_withdrawal() {
                //     let withdrawn_routes = process_routes(router_id, update, update.withdrawn_routes);
                // }
            }
            Message::Notification(notification) => {
                warn!("{} NOTIFICATION: {}", self.remote_id, notification);
                MessageResponse::Empty
            }
            Message::RouteRefresh(_) => MessageResponse::Empty,
        };
        Ok(response)
    }
}

impl Default for Peer {
    fn default() -> Self {
        let ip = "0.0.0.0".parse().unwrap();
        Peer::new(
            ip,
            PeerState::Idle,
            PeerIdentifier::new(Some(ip), 0),
            PeerIdentifier::new(Some(ip), 0),
            false,
            0,
        )
    }
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Peer {} {} {}>",
            self.addr,
            self.remote_id,
            self.state.to_string(),
        )
    }
}

fn process_routes(router_id: IpAddr, update: &Update, routes: &Vec<NLRIEncoding>) -> Vec<Route> {
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

    let community_list = CommunityList(
        communities
            .into_iter()
            .chain(ext_communities.into_iter())
            .collect(),
    );

    routes
        .iter()
        .map(|route| match route {
            NLRIEncoding::IP(prefix) => Some(prefix),
            _ => None,
        })
        .filter(std::option::Option::is_some)
        .map(std::option::Option::unwrap)
        .map(|prefix| Route {
            peer: router_id,
            state: RouteState::Received(Utc::now()),
            prefix: prefix.clone(),
            next_hop,
            origin: origin.clone(),
            as_path: as_path.clone(),
            local_pref,
            multi_exit_disc,
            communities: community_list.clone(),
        })
        .collect()
}
