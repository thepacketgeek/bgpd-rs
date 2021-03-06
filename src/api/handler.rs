use std::net::SocketAddr;
use std::sync::Arc;

use bgpd_rpc_lib::{Api, LearnedRoute, PeerDetail, PeerSummary};
use jsonrpsee::{raw::RawServer, transport::http::HttpTransportServer};
use log::info;

use crate::handler::State;
use crate::rib::EntrySource;

use super::peers::{peer_to_detail, peer_to_summary};
use super::routes::entry_to_route;
use crate::utils::{get_host_address, parse_flow_spec, parse_route_spec};

pub async fn serve(socket: SocketAddr, state: Arc<State>) {
    info!("Starting JSON-RPC server on {}...", socket);
    tokio::spawn(async move {
        let mut server = {
            let transport_server = HttpTransportServer::bind(&socket).await.unwrap();
            RawServer::new(transport_server)
        };

        while let Ok(request) = Api::next_request(&mut server).await {
            match request {
                Api::ShowPeers { respond } => {
                    let mut output: Vec<PeerSummary> = vec![];
                    let sessions = state.sessions.read().await;
                    let configs = sessions.get_peer_configs();
                    let active_sessions = sessions.sessions.read().await;
                    let rib = state.rib.read().await;
                    // Summary for any non-idle sessions
                    let session_summaries: Vec<PeerSummary> = active_sessions
                        .iter()
                        .map(|(addr, session)| {
                            let pfx_rcvd = rib.get_routes_from_peer(*addr).len() as u64;
                            peer_to_summary(session.config.clone(), Some(session), Some(pfx_rcvd))
                        })
                        .collect();
                    output.extend(session_summaries);
                    // Summaries for idle peer/network configs
                    let idle_summaries = configs
                        .into_iter()
                        .filter_map(|config| {
                            if let Some(remote_ip) = get_host_address(&config.remote_ip) {
                                // Don't duplicate session summaries
                                if active_sessions.get(&remote_ip).is_some() {
                                    return None;
                                }
                            }
                            Some(peer_to_summary(config, None, None))
                        })
                        .collect::<Vec<PeerSummary>>();
                    output.extend(idle_summaries);
                    respond.ok(output).await;
                }
                Api::ShowPeerDetail { respond } => {
                    let mut output: Vec<PeerDetail> = vec![];
                    let sessions = state.sessions.read().await;
                    let configs = sessions.get_peer_configs();
                    let active_sessions = sessions.sessions.read().await;
                    let rib = state.rib.read().await;
                    // Detail for any non-idle sessions
                    let session_details: Vec<PeerDetail> = active_sessions
                        .iter()
                        .map(|(addr, session)| {
                            let pfx_rcvd = rib.get_routes_from_peer(*addr).len() as u64;
                            peer_to_detail(session.config.clone(), Some(session), Some(pfx_rcvd))
                        })
                        .collect();
                    output.extend(session_details);
                    // Details for idle peer/network configs
                    let idle_details: Vec<PeerDetail> = configs
                        .into_iter()
                        .filter_map(|config| {
                            if let Some(remote_ip) = get_host_address(&config.remote_ip) {
                                // Don't duplicate session details
                                if active_sessions.get(&remote_ip).is_some() {
                                    return None;
                                }
                            }
                            Some(peer_to_detail(config, None, None))
                        })
                        .collect();
                    output.extend(idle_details);
                    respond.ok(output).await;
                }
                Api::ShowRoutesLearned { respond, from_peer } => {
                    let mut output: Vec<LearnedRoute> = vec![];
                    let entries = {
                        let rib = state.rib.read().await;
                        if let Some(peer) = from_peer {
                            let sessions = state.sessions.read().await;
                            let active_sessions = sessions.sessions.read().await;
                            active_sessions
                                .keys()
                                // Find peers contained in the `from_peer` prefix
                                .filter(|addr| peer.contains(**addr))
                                // Collect routes for each matching peer
                                .map(|p| rib.get_routes_from_peer(*p))
                                .flatten()
                                .collect::<Vec<_>>()
                        } else {
                            rib.get_routes()
                        }
                    };
                    let routes: Vec<_> = entries.into_iter().map(entry_to_route).collect();
                    output.extend(routes);
                    respond.ok(output).await;
                }
                Api::ShowRoutesAdvertised { respond, to_peer } => {
                    let mut output: Vec<LearnedRoute> = vec![];
                    let sessions = state.sessions.read().await;
                    let active_sessions = sessions.sessions.read().await;
                    let routes: Vec<LearnedRoute> = active_sessions
                        .values()
                        .filter(|s| match to_peer {
                            Some(prefix) => prefix.contains(s.addr),
                            _ => true,
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
                            let mut rib = state.rib.write().await;
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
                            let mut rib = state.rib.write().await;
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
