use std::net::SocketAddr;

use ipnetwork::IpNetwork;
use jsonrpsee::{
    core::{Error, RpcResult},
    http_server::{HttpServerBuilder, HttpServerHandle},
};
use log::info;

use super::peers::{peer_to_detail, peer_to_summary};
use super::routes::entry_to_route;
use super::rpc::{ApiServer, FlowSpec, LearnedRoute, PeerDetail, PeerSummary, RouteSpec};
use crate::handler::Server;
use crate::rib::EntrySource;
use crate::utils::{get_host_address, parse_flow_spec, parse_route_spec};

#[async_trait::async_trait]
impl ApiServer for Server {
    async fn show_peers(&self) -> RpcResult<Vec<PeerSummary>> {
        let mut output: Vec<PeerSummary> = vec![];
        let sessions = self.inner.sessions.read().await;
        let configs = sessions.get_peer_configs();
        let active_sessions = sessions.sessions.read().await;
        let rib = self.inner.rib.read().await;
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
        Ok(output)
    }

    async fn show_peer_detail(&self) -> RpcResult<Vec<PeerDetail>> {
        let mut output: Vec<PeerDetail> = vec![];
        let sessions = self.inner.sessions.read().await;
        let configs = sessions.get_peer_configs();
        let active_sessions = sessions.sessions.read().await;
        let rib = self.inner.rib.read().await;
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
        Ok(output)
    }

    async fn show_routes_learned(
        &self,
        from_peer: Option<IpNetwork>,
    ) -> RpcResult<Vec<LearnedRoute>> {
        let mut output: Vec<LearnedRoute> = vec![];
        let entries = {
            let rib = self.inner.rib.read().await;
            if let Some(peer) = from_peer {
                let sessions = self.inner.sessions.read().await;
                let active_sessions = sessions.sessions.read().await;
                active_sessions
                    .keys()
                    // Find peers contained in the `from_peer` prefix
                    .filter(|addr| peer.contains(**addr))
                    // Collect routes for each matching peer
                    .flat_map(|p| rib.get_routes_from_peer(*p))
                    .collect::<Vec<_>>()
            } else {
                rib.get_routes()
            }
        };
        let routes: Vec<_> = entries.into_iter().map(entry_to_route).collect();
        output.extend(routes);
        Ok(output)
    }

    async fn show_routes_advertised(
        &self,
        to_peer: Option<IpNetwork>,
    ) -> RpcResult<Vec<LearnedRoute>> {
        let mut output: Vec<LearnedRoute> = vec![];
        let sessions = self.inner.sessions.read().await;
        let active_sessions = sessions.sessions.read().await;
        let routes: Vec<LearnedRoute> = active_sessions
            .values()
            .filter(|s| match to_peer {
                Some(prefix) => prefix.contains(s.addr),
                _ => true,
            })
            .flat_map(|s| {
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
            .collect();
        output.extend(routes);
        Ok(output)
    }

    async fn advertise_route(&self, route: RouteSpec) -> RpcResult<LearnedRoute> {
        let update = parse_route_spec(&route).map_err(|e| Error::Custom(e.to_string()))?;
        let (family, attributes, nlri) = update;
        let mut rib = self.inner.rib.write().await;
        let entry = rib.insert_from_api(family, attributes, nlri);
        Ok(entry_to_route(entry))
    }

    async fn advertise_flow(&self, flow: FlowSpec) -> RpcResult<LearnedRoute> {
        let update = parse_flow_spec(&flow).map_err(|e| Error::Custom(e.to_string()))?;
        let (family, attributes, nlri) = update;
        let mut rib = self.inner.rib.write().await;
        let entry = rib.insert_from_api(family, attributes, nlri);
        Ok(entry_to_route(entry))
    }
}

impl Server {
    pub async fn serve_rpc_api(&self, socket: SocketAddr) -> RpcResult<HttpServerHandle> {
        let server = self.clone();
        info!("Starting JSON-RPC server on {}...", socket);
        let handle = HttpServerBuilder::default()
            .build(socket)
            .await?
            .start(server.into_rpc())?;
        Ok(handle)
    }
}
