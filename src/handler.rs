use std::error::Error;
use std::sync::Arc;

use log::trace;
use tokio::net::TcpListener;
use tokio::sync::{watch, RwLock};

use crate::config::ServerConfig;
use crate::rib::RIB;
use crate::session::{SessionManager, SessionUpdate};
use crate::utils::{parse_flow_spec, parse_route_spec};

#[derive(Clone)]
pub struct Server {
    pub(crate) inner: Arc<State>,
}

pub struct State {
    pub(crate) sessions: Arc<RwLock<SessionManager>>,
    pub(crate) rib: Arc<RwLock<RIB>>,
}

impl Server {
    pub fn new(
        config: Arc<ServerConfig>,
        listener: TcpListener,
        config_rx: watch::Receiver<Arc<ServerConfig>>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut rib = RIB::new();
        for peer in config.peers.iter() {
            for route in peer.static_routes.iter() {
                let (family, attributes, nlri) = parse_route_spec(route)?;
                rib.insert_from_config(family, attributes, nlri);
            }
            for route in peer.static_flows.iter() {
                let (family, attributes, nlri) = parse_flow_spec(route)?;
                rib.insert_from_config(family, attributes, nlri);
            }
        }
        let manager = SessionManager::new(config, listener, config_rx);

        Ok(Self {
            inner: Arc::new(State {
                sessions: Arc::new(RwLock::new(manager)),
                rib: Arc::new(RwLock::new(rib)),
            }),
        })
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        while let Ok(update) = self
            .inner
            .sessions
            .write()
            .await
            .get_update(self.inner.rib.clone())
            .await
        {
            trace!("Rib has {} entries", self.inner.rib.read().await.len());
            match update {
                Some(SessionUpdate::Learned((router_id, update))) => {
                    trace!("Incoming update from {}: {:?}", router_id, update);
                    self.inner
                        .rib
                        .write()
                        .await
                        .update_from_peer(router_id, update)?;
                }
                Some(SessionUpdate::Ended(peers)) => {
                    let mut rib = self.inner.rib.write().await;
                    for peer in peers {
                        rib.remove_from_peer(peer);
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
}
