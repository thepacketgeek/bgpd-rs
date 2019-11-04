use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use bgp_rs::{ASPath, NLRIEncoding, Origin, SAFI};
use bgpd_rpc_lib::{AdvertiseRoute, Api, LearnedRoute, PeerDetail, PeerSummary};
use jsonrpsee;
use log::info;

use crate::handler::State;
use crate::rib::{CommunityList, EntrySource, Family, PathAttributes, StoredUpdate};

use super::peers::{peer_to_detail, peer_to_summary};
use super::routes::entry_to_route;
use crate::utils::prefix_from_string;

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
                    // TODO: How to get/store the actual sent info
                    //       like ASPath, etc (after policy)
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
                                    // TODO: Process advertised routes differently
                                    //       so we can report actual advertised info
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
                            let mut rib = state.rib.lock().await;
                            let entry = rib.insert_from_api(update);
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

fn advertise_route_to_update(route: AdvertiseRoute) -> Result<StoredUpdate, String> {
    let prefix = prefix_from_string(&route.prefix).map_err(|err| err.to_string())?;
    let next_hop = route
        .next_hop
        .parse::<IpAddr>()
        .map_err(|err| err.to_string())?;
    let origin = route
        .origin
        .map(|origin| match origin.to_lowercase().as_str() {
            "igp" => Origin::IGP,
            "egp" => Origin::EGP,
            "?" | "incomplete" => Origin::INCOMPLETE,
            _ => Origin::INCOMPLETE,
            // TODO: Raise on invalid Origin
            // _ => Err(format!(
            //     "Invalid Origin '{}'. Must be one of: IGP, EGP, ?",
            //     origin
            // )),
        })
        .unwrap_or(Origin::INCOMPLETE);
    let as_path = ASPath { segments: vec![] };
    //     segments: route.as_path.split(" ").iter().collect(),
    // };

    let communities = CommunityList(vec![]);

    let attributes = Arc::new(PathAttributes {
        next_hop: Some(next_hop),
        origin,
        as_path,
        local_pref: route.local_pref,
        multi_exit_disc: route.multi_exit_disc,
        communities,
    });

    Ok(StoredUpdate {
        family: Family::new(prefix.protocol, SAFI::Unicast),
        attributes,
        nlri: NLRIEncoding::IP(prefix),
    })
}
