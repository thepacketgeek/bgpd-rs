use std::net::SocketAddr;
use std::sync::Arc;

use bgpd_rpc_lib::{Api, LearnedRoute, PeerDetail, PeerSummary};
use jsonrpsee;
use log::info;

use crate::handler::State;
use crate::rib::EntrySource;

use super::peers::{peer_to_detail, peer_to_summary};
use super::routes::entry_to_route;
use crate::utils::{parse_flow_spec, parse_route_spec};

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
                    let response = match parse_route_spec(&route) {
                        Ok(update) => {
                            let (family, attributes, nlri) = update;
                            let mut rib = state.rib.lock().await;
                            let entry = rib.insert_from_api(family, attributes, nlri);
                            Ok(entry_to_route(entry))
                        }
                        Err(err) => Err(err.to_string()),
                    };
                    respond.ok(response).await;
                }
                Api::AdvertiseFlow { respond, flow } => {
                    let response = match parse_flow_spec(&flow) {
                        Ok(update) => {
                            let (family, attributes, nlri) = update;
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
