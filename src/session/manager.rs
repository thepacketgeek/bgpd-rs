use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;

use bgp_rs::{Message, Notification};
use futures::future::FutureExt;
use futures::{pin_mut, select};
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
            poller.upsert_peer(peer_config.clone());
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
                                let notif = Notification {
                                    major_err_code: 6,
                                    minor_err_code: 3,
                                    data: vec![],
                                };
                                session.send_message(Message::Notification(notif)).await?;
                            }
                            SessionError::HoldTimeExpired(_) => {
                                let notif = Notification {
                                    major_err_code: 4,
                                    minor_err_code: 0,
                                    data: vec![],
                                };
                                session.send_message(Message::Notification(notif)).await?;
                            }
                            SessionError::FiniteStateMachine(minor) => {
                                let notif = Notification {
                                    major_err_code: 5,
                                    minor_err_code: minor,
                                    data: vec![],
                                };
                                session.send_message(Message::Notification(notif)).await?;
                            }
                            SessionError::OpenAsnMismatch(_, _) => {
                                let notif = Notification {
                                    major_err_code: 3,
                                    minor_err_code: 2,
                                    data: vec![],
                                };
                                session.send_message(Message::Notification(notif)).await?;
                            }
                            _ => (),
                        }
                        warn!("{}", err);
                        self.poller_tx.send(session.peer.clone()).unwrap();
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
                    let remote_ip = peer_config.remote_ip;
                    if sessions.contains_key(&remote_ip) {
                        warn!(
                            "Unexpected connection from {}: Already have an existing session",
                            remote_ip,
                        );
                        return Ok(None);
                    }
                    let protocol = MessageProtocol::new(stream, MessageCodec::new());
                    let new_session = Session::new(Arc::clone(&peer_config), protocol);
                    sessions.insert(remote_ip, new_session);
                    info!("New session started: {}", peer_config.remote_ip);
                }
                Ok(None)
            },
            update = config_updates => {
                if let Some(new_config) = update {
                    self.config = new_config.clone();
                    let mut current_sessions = self.sessions.lock().await;
                    for peer_config in &new_config.peers {
                        if let Some(active_session) = current_sessions.get_mut(&peer_config.remote_ip) {
                            active_session.update_config(peer_config.clone());
                        }
                    }
                    let config_peers: HashMap<IpAddr, Arc<PeerConfig>> = new_config
                        .peers
                        .iter()
                        .map(|p| (p.remote_ip, p.clone()))
                        .collect();
                    let added_peers: Vec<_> = config_peers
                        .iter()
                        .filter(|(ip, _)| !current_sessions.contains_key(&ip))
                        .collect();
                    let removed_peers: Vec<_> = current_sessions
                        .iter()
                        .filter(|(ip, _)| !config_peers.contains_key(&ip))
                        .map(|(ip, _)| ip.clone())
                        .collect();

                    debug!(
                        "Received config [{} added peers, {} removed peers]",
                        added_peers.len(),
                        removed_peers.len()
                    );

                    for removed_ip in removed_peers {
                        warn!("Session ended with {}, peer de-configured", removed_ip);
                        let mut session = current_sessions.remove(&removed_ip).expect("Active session");
                        let notif = Notification {
                            major_err_code: 6, // Cease
                            minor_err_code: 3, // Deconfigured
                            data: vec![],
                        };
                        session.send_message(Message::Notification(notif)).await?;
                    }

                    for (_, new_peer) in added_peers {
                        self.poller_tx.send(new_peer.clone()).unwrap();
                    }
                }
                Ok(None)
            }
        }
    }
}

// async fn init_connection(
//     sessions: Arc<Mutex<HashMap<IpAddr, Session>>>,
//     connection: Result<(TcpStream, PeerConfig)>,
// ) {

// }
