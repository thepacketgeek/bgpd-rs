use std::io::Error;
use std::sync::Arc;

use log::trace;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};

use crate::config::ServerConfig;
use crate::rib::RIB;
use crate::session::{SessionManager, SessionUpdate};

pub struct Server {
    inner: Arc<State>,
}

pub struct State {
    pub(crate) sessions: Arc<Mutex<SessionManager>>,
    pub(crate) rib: Arc<Mutex<RIB>>,
}

impl Server {
    pub fn new(
        config: Arc<ServerConfig>,
        listener: TcpListener,
        config_rx: watch::Receiver<Arc<ServerConfig>>,
    ) -> Result<Self, Error> {
        let mut rib = RIB::new();
        for peer in config.peers.iter() {
            for route in peer.static_routes.iter() {
                let (family, attributes, nlri) = route.parse()?;
                rib.insert_from_config(family, attributes, nlri);
            }
        }
        let manager = SessionManager::new(config, listener, config_rx);

        Ok(Self {
            inner: Arc::new(State {
                sessions: Arc::new(Mutex::new(manager)),
                rib: Arc::new(Mutex::new(rib)),
            }),
        })
    }

    pub fn clone_state(&self) -> Arc<State> {
        self.inner.clone()
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Ok(update) = self
            .inner
            .sessions
            .lock()
            .await
            .get_update(self.inner.rib.clone())
            .await
        {
            trace!("Rib has {} entries", self.inner.rib.lock().await.len());
            match update {
                Some(SessionUpdate::Learned((router_id, update))) => {
                    trace!("Incoming update from {}: {:?}", router_id, update);
                    self.inner
                        .rib
                        .lock()
                        .await
                        .update_from_peer(router_id, update)?;
                }
                Some(SessionUpdate::Ended(peers)) => {
                    let mut rib = self.inner.rib.lock().await;
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
