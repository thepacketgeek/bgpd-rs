use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;

use futures::future::FutureExt;
use futures::{pin_mut, select};
use ipnetwork::IpNetwork;
use log::{debug, info, warn};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch, Mutex};

use super::codec::{MessageCodec, MessageProtocol};
use super::{Poller, PollerTx, Session, SessionError, SessionUpdate};
use crate::config::{PeerConfig, ServerConfig};
use crate::rib::RIB;

pub struct SessionManager {
    pub(crate) idle_peers: Poller,
    // Active Sessions                  remote_ip: session
    pub(crate) sessions: Arc<Mutex<HashMap<IpAddr, Session>>>,
    config: Arc<ServerConfig>,
    poller_tx: PollerTx,
    config_watch: watch::Receiver<Arc<ServerConfig>>,
}

impl SessionManager {
    pub fn new(
        config: Arc<ServerConfig>,
        listener: TcpListener,
        config_watch: watch::Receiver<Arc<ServerConfig>>,
    ) -> Self {
        let (poller_tx, poller_rx) = mpsc::unbounded_channel();
        let mut poller = Poller::new(listener, config.poll_interval.into(), poller_rx);
        for peer_config in config.peers.iter() {
            poller.upsert_config(peer_config.clone());
        }

        Self {
            idle_peers: poller,
            sessions: Arc::new(Mutex::new(HashMap::with_capacity(config.peers.len()))),
            config,
            poller_tx,
            config_watch,
        }
    }

    pub fn get_peer_configs(&self) -> Vec<Arc<PeerConfig>> {
        self.config.peers.to_vec()
    }

    pub async fn get_update(
        &mut self,
        rib: Arc<Mutex<RIB>>,
    ) -> Result<Option<SessionUpdate>, Box<dyn Error>> {
        let sessions_clone = Arc::clone(&self.sessions);
        let receive_new_sessions = self.idle_peers.get_connection().fuse();
        let config_updates = self.config_watch.recv().fuse();

        // TODO: Figure out how to select_all over sessions
        // let active_sessions = {
        //     let mut sessions = self.sessions.lock().await;
        //     let futs: Vec<_> = sessions
        //         .values_mut()
        //         .map(|sess| Box::pin(sess.run()))
        //         .collect();
        //     select_all(futs).fuse()
        // };
        {
            // Store sessions that have ended (remote_ip, router_id)
            let mut ended_sessions: Vec<IpAddr> = Vec::new();
            let mut sessions = self.sessions.lock().await;
            for (remote_ip, session) in sessions.iter_mut() {
                let routes = rib.lock().await.get_routes_for_peer(session.addr);
                session.routes.insert_routes(routes);

                match session.run().await {
                    Ok(update) => {
                        if update.is_some() {
                            return Ok(update);
                        }
                    }
                    Err(err) => {
                        match err {
                            SessionError::Deconfigured => {
                                session.notify(6, 3).await?;
                            }
                            SessionError::HoldTimeExpired(_) => {
                                session.notify(4, 0).await?;
                            }
                            SessionError::FiniteStateMachine(minor) => {
                                session.notify(5, minor).await?;
                            }
                            SessionError::OpenAsnMismatch(_, _) => {
                                session.notify(3, 2).await?;
                            }
                            _ => (),
                        }
                        warn!("{}", err);
                        self.poller_tx.send(session.config.clone()).unwrap();
                        ended_sessions.push(*remote_ip);
                    }
                }
            }
            // Remove ended sessions and alert handler for RIB removal
            if !ended_sessions.is_empty() {
                for remote_ip in &ended_sessions {
                    sessions.remove(&remote_ip);
                }
                return Ok(Some(SessionUpdate::Ended(ended_sessions)));
            }
        }

        pin_mut!(receive_new_sessions, config_updates);
        select! {
            new_connection = receive_new_sessions => {
                if let Ok(Some((stream, peer_config))) = new_connection {
                    let mut sessions = sessions_clone.lock().await;
                    let remote_ip = stream.peer_addr().expect("Stream has remote peer").ip();
                    if sessions.contains_key(&remote_ip) {
                        warn!(
                            "Unexpected connection from {}: Already have an existing session",
                            remote_ip,
                        );
                        return Ok(None);
                    }
                    let protocol = MessageProtocol::new(stream, MessageCodec::new());
                    let new_session = Session::new(Arc::clone(&peer_config), protocol);
                    info!("New session started: {}", remote_ip);
                    sessions.insert(remote_ip, new_session);
                }
                Ok(None)
            },
            update = config_updates => {
                if let Some(new_config) = update {
                    self.config = new_config.clone();
                    let configs_by_network: HashMap<IpNetwork, Arc<PeerConfig>> = new_config
                        .peers
                        .iter()
                        .map(|p| (p.remote_ip, p.clone()))
                        .collect();
                    { // Current Sessions lock scope
                        let mut current_sessions = self.sessions.lock().await;
                        let removed_peers = find_removed_peers(&mut current_sessions, &configs_by_network);

                        debug!(
                            "Received config [{} peer configs, {} removed peer configs]",
                            configs_by_network.len(),
                            removed_peers.len()
                        );

                        for removed_ip in removed_peers {
                            warn!("Session ended with {}, peer de-configured", removed_ip);
                            let mut session = current_sessions.remove(&removed_ip).expect("Active session");
                            session.notify(6 /* Cease */, 3/* Deconfigured */).await?;
                        }
                    }

                    for (_, new_config) in configs_by_network {
                        self.poller_tx.send(new_config.clone())?;
                    }
                }
                Ok(None)
            }
        }
    }
}

fn find_removed_peers(
    sessions: &mut HashMap<IpAddr, Session>,
    configs: &HashMap<IpNetwork, Arc<PeerConfig>>,
) -> Vec<IpAddr> {
    sessions
        .iter_mut()
        .filter_map(|(addr, current_session)| {
            if let Some(network) = configs.keys().find(|n| n.contains(*addr)) {
                let config = configs.get(network).expect("Network has config");
                current_session.update_config(config.clone());
                None
            } else {
                Some(*addr)
            }
        })
        .collect()
}
