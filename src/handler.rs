use std::io::{self, Error};
use std::sync::Arc;

use bgp_rs::{ASPath, NLRIEncoding, Origin, SAFI};
use log::trace;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};

use crate::config::ServerConfig;
use crate::rib::{CommunityList, Family, PathAttributes, StoredUpdate, RIB};
use crate::session::{SessionManager, SessionUpdate};
use crate::utils::prefix_from_string;

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
                let prefix = prefix_from_string(&route.prefix)
                    .map_err(|err| Error::new(io::ErrorKind::Other, err.to_string()))?;
                let update = StoredUpdate {
                    family: Family::new(prefix.protocol, SAFI::Unicast),
                    attributes: Arc::new(PathAttributes {
                        next_hop: Some(route.next_hop),
                        origin: Origin::INCOMPLETE,
                        as_path: ASPath { segments: vec![] },
                        local_pref: None,
                        multi_exit_disc: None,
                        communities: CommunityList(vec![]),
                    }),
                    nlri: NLRIEncoding::IP(prefix),
                };
                rib.insert_from_config(update);
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
            match update {
                Some(SessionUpdate::Learned((router_id, update))) => {
                    trace!("Incoming update from {}: {:?}", router_id, update);
                    self.inner
                        .rib
                        .lock()
                        .await
                        .insert_from_peer(router_id, update);
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
