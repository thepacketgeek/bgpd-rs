use std::convert::TryFrom;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use bgp_rs::{ASPath, NLRIEncoding, Origin, PathAttribute, Segment, SAFI};
use bgpd_rpc_lib::{AdvertiseRoute, Api, LearnedRoute, PeerDetail, PeerSummary};
use jsonrpsee;
use log::info;

use crate::handler::State;
use crate::rib::{Community, CommunityList, EntrySource, Family};

use super::peers::{peer_to_detail, peer_to_summary};
use super::routes::entry_to_route;
use crate::utils::{asn_from_dotted, prefix_from_string};

pub async fn serve(socket: SocketAddr, state: Arc<State>) {
    info!("Starting JSON-RPC server on {}...", socket);
    tokio::spawn(async move {
        let mut server = jsonrpsee::http_server(&socket).await.unwrap();

        while let Ok(request) = Api::next_request(&mut server).await {
            match request {
                Api::ShowPeers { respond } => {
                    let mut output: Vec<PeerSummary> = vec![];
                    let sessions = state.sessions.lock().await;
                    let peers = sessions.get_peer_configs();
                    let active_sessions = sessions.sessions.lock().await;
                    let rib = state.rib.lock().await;
                    let peer_info = peers
                        .into_iter()
                        .map(|config| {
                            let session = active_sessions.get(&config.remote_ip);
                            let pfx_rcvd = {
                                if let Some(session) = session {
                                    let routes = rib.get_routes_from_peer(session.peer.remote_ip);
                                    Some(routes.len() as u64)
                                } else {
                                    None
                                }
                            };
                            peer_to_summary(config, session, pfx_rcvd)
                        })
                        .collect::<Vec<PeerSummary>>();
                    output.extend(peer_info);
                    respond.ok(output).await;
                }
                Api::ShowPeerDetail { respond } => {
                    let mut output: Vec<PeerDetail> = vec![];
                    let sessions = state.sessions.lock().await;
                    let peers = sessions.get_peer_configs();
                    let active_sessions = sessions.sessions.lock().await;
                    let rib = state.rib.lock().await;
                    let peer_info = peers
                        .into_iter()
                        .map(|config| {
                            let session = active_sessions.get(&config.remote_ip);
                            let pfx_rcvd = {
                                if let Some(session) = session {
                                    let routes = rib.get_routes_from_peer(session.addr);
                                    Some(routes.len() as u64)
                                } else {
                                    None
                                }
                            };
                            peer_to_detail(config, session, pfx_rcvd)
                        })
                        .collect::<Vec<PeerDetail>>();
                    output.extend(peer_info);
                    respond.ok(output).await;
                }
                Api::ShowRoutesLearned { respond, from_peer } => {
                    let mut output: Vec<LearnedRoute> = vec![];
                    let rib = state.rib.lock().await;
                    let entries = if let Some(peer) = from_peer {
                        rib.get_routes_from_peer(peer)
                    } else {
                        rib.get_routes()
                    };
                    let routes: Vec<_> = entries
                        .into_iter()
                        .map(|entry| entry_to_route(entry))
                        .collect();
                    output.extend(routes);
                    respond.ok(output).await;
                }
                Api::ShowRoutesAdvertised { respond, to_peer } => {
                    let mut output: Vec<LearnedRoute> = vec![];
                    let sessions = state.sessions.lock().await;
                    let active_sessions = sessions.sessions.lock().await;
                    let routes: Vec<LearnedRoute> = active_sessions
                        .values()
                        .filter(|s| {
                            if let Some(addr) = to_peer {
                                s.addr == addr
                            } else {
                                true
                            }
                        })
                        .map(|s| {
                            s.routes
                                .advertised()
                                .into_iter()
                                .map(|entry| {
                                    let mut entry = entry_to_route(entry);
                                    entry.source = EntrySource::Peer(s.addr).to_string();
                                    entry
                                })
                                .collect::<Vec<_>>()
                        })
                        .flatten()
                        .collect();
                    output.extend(routes);
                    respond.ok(output).await;
                }
                Api::AdvertiseRoute { respond, route } => {
                    let response = match advertise_route_to_update(route) {
                        Ok(update) => {
                            let (attributes, family, nlri) = update;
                            let mut rib = state.rib.lock().await;
                            let entry = rib.insert_from_api(family, attributes, nlri);
                            Ok(entry_to_route(entry))
                        }
                        Err(err) => Err(err.to_string()),
                    };
                    respond.ok(response).await;
                }
            }
        }
    });
}

fn advertise_route_to_update(
    route: AdvertiseRoute,
) -> Result<(Vec<PathAttribute>, Family, NLRIEncoding), String> {
    let prefix = prefix_from_string(&route.prefix).map_err(|err| err.to_string())?;
    let mut attributes = vec![];
    let next_hop = route
        .next_hop
        .parse::<IpAddr>()
        .map_err(|err| err.to_string())?;
    attributes.push(PathAttribute::NEXT_HOP(next_hop));
    if let Some(origin) = route.origin {
        let origin = match origin.to_lowercase().as_str() {
            "igp" => Origin::IGP,
            "egp" => Origin::EGP,
            "?" | "incomplete" => Origin::INCOMPLETE,
            _ => return Err(format!("Not a valid origin: {}", origin)),
        };
        attributes.push(PathAttribute::ORIGIN(origin));
    }
    if let Some(local_pref) = route.local_pref {
        attributes.push(PathAttribute::LOCAL_PREF(local_pref));
    }
    if let Some(med) = route.multi_exit_disc {
        attributes.push(PathAttribute::MULTI_EXIT_DISC(med));
    }
    let as_path = {
        let mut asns: Vec<u32> = Vec::with_capacity(route.as_path.len());
        for asn in &route.as_path {
            asns.push(
                asn_from_dotted(asn).map_err(|err| format!("Error parsing ASN: {}", err.reason))?,
            );
        }
        ASPath {
            segments: vec![Segment::AS_SEQUENCE(asns)],
        }
    };
    attributes.push(PathAttribute::AS_PATH(as_path));
    let communities = {
        let mut comms: Vec<Community> = Vec::with_capacity(route.communities.len());
        for comm in route.communities {
            comms.push(
                Community::try_from(comm.as_str())
                    .map_err(|err| format!("Error parsing ASN: {}", err))?,
            );
        }
        CommunityList(comms)
    };
    let standard_communities = communities.standard();
    if !standard_communities.is_empty() {
        attributes.push(PathAttribute::COMMUNITY(standard_communities));
    }
    let extd_communities = communities.extended();
    if !extd_communities.is_empty() {
        attributes.push(PathAttribute::EXTENDED_COMMUNITIES(extd_communities));
    }

    Ok((
        attributes,
        Family::new(prefix.protocol, SAFI::Unicast),
        NLRIEncoding::IP(prefix),
    ))
}
