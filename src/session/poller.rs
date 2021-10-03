use std::collections::HashMap;
use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use futures::StreamExt;
use ipnetwork::IpNetwork;
use log::{debug, trace, warn};
use net2::TcpBuilder;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tokio_util::time::DelayQueue;
use tokio::{
    self,
    net::{TcpListener, TcpSocket, TcpStream},
};

use crate::config::PeerConfig;
use crate::utils::get_host_address;

const TCP_INIT_TIMEOUT_MS: u16 = 1000;

pub type PollerTx = mpsc::UnboundedSender<Arc<PeerConfig>>;
pub type PollerRx = mpsc::UnboundedReceiver<Arc<PeerConfig>>;

#[derive(Debug)]
pub struct IdlePeer(Arc<PeerConfig>);

impl IdlePeer {
    pub fn new(config: Arc<PeerConfig>) -> Self {
        Self(config)
    }

    pub fn get_config(&self) -> Arc<PeerConfig> {
        Arc::clone(&self.0)
    }

    async fn connect(
        &self,
        source_addr: SocketAddr,
    ) -> Result<(TcpStream, Arc<PeerConfig>), io::Error> {
        if let Some(remote_ip) = get_host_address(&self.0.remote_ip) {
            let peer_addr = SocketAddr::new(remote_ip, self.0.dest_port);
            let builder = match peer_addr {
                SocketAddr::V4(_) => TcpBuilder::new_v4()?,
                SocketAddr::V6(_) => TcpBuilder::new_v6()?,
            };
            builder.reuse_address(true)?;
            builder.bind(source_addr)?;
            let s = TcpSocket::from_std_stream(builder.to_tcp_stream()?);
            let connect = s.connect(peer_addr);
            return match timeout(Duration::from_millis(TCP_INIT_TIMEOUT_MS.into()), connect).await?
            {
                Ok(stream) => Ok((stream, self.0.clone())),
                Err(err) => Err(err),
            };
        }
        unreachable!();
    }
}

impl fmt::Display for IdlePeer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<IdlePeer {}>", self.0.remote_ip)
    }
}

/// Stores Idle peers and checks every interval if there are peers that the Handler
/// can attempt to connect to
pub struct Poller {
    idle_peers: HashMap<IpNetwork, IdlePeer>,
    tcp_listener: TcpListener,
    rx: PollerRx,
    interval: Duration,
    delay_queue: DelayQueue<IpAddr>,
}

impl Poller {
    pub fn new(listener: TcpListener, interval: u32 /* seconds */, rx: PollerRx) -> Self {
        Self {
            idle_peers: HashMap::new(),
            tcp_listener: listener,
            interval: Duration::from_secs(interval.into()),
            delay_queue: DelayQueue::with_capacity(4),
            rx,
        }
    }

    pub fn upsert_config(&mut self, config: Arc<PeerConfig>) {
        let network = config.remote_ip;

        if self
            .idle_peers
            .insert(config.remote_ip, IdlePeer::new(config))
            .is_some()
        {
            debug!("Peer config for {} updated", network);
        } else if let Some(remote_ip) = get_host_address(&network) {
            // Add to outgoing connection queue if there was no existing config
            // and if it's a single host
            self.delay_queue.insert(remote_ip, self.interval);
        }
    }

    pub fn replace_configs(&mut self, configs: Vec<Arc<PeerConfig>>) {
        self.delay_queue.clear();
        self.idle_peers.clear();
        for config in configs.into_iter() {
            self.upsert_config(config);
        }
    }

    pub async fn get_connection(
        &mut self,
    ) -> Result<Option<(TcpStream, Arc<PeerConfig>)>, io::Error> {
        let local_outbound_addr = self.tcp_listener.local_addr().expect("Has local address");
        let listener = timeout(
            Duration::from_millis(TCP_INIT_TIMEOUT_MS.into()),
            self.tcp_listener.accept(),
        );

        tokio::select! {
            incoming = listener => {
                if let Ok(Ok((stream, socket))) = incoming {
                    if let Some(config) = get_config_for_peer(&self.idle_peers, socket.ip()) {
                        if config.enabled {
                            let config = if get_host_address(&config.remote_ip).is_some() {
                                // Only remove from idle peers if this a for a single peer
                                self.idle_peers.remove(&config.remote_ip)
                                    .expect("Idle peer exists")
                                    .get_config()
                            } else {
                                self.idle_peers.get(&config.remote_ip)
                                    .expect("Idle peer exists")
                                    .get_config()

                            };
                            debug!("Incoming new connection from {}", socket.ip());
                            return Ok(Some((stream, config)));
                        }
                    } else {
                        warn!(
                            "Unexpected connection from {}: Not a configured peer",
                            socket.ip(),
                        );
                    }
                }
                Ok(None)
            },
            // If DelayQueue.is_empty() and is polled, CPU spikes to 100%
            outgoing = self.delay_queue.next(), if !self.delay_queue.is_empty() => {
                if let Some(Ok(peer)) = outgoing {
                    let addr = peer.into_inner();
                    trace!("Poller outbound triggered for {}", addr);
                    // Peer may not be present if an incoming connection
                    // was established simultaneously
                    if let Some(config) = get_config_for_peer(&self.idle_peers, addr) {
                        if config.enabled && !config.passive {
                            let peer = self.idle_peers.remove(&config.remote_ip).expect("Idle peer exists");
                            match peer.connect(SocketAddr::new(local_outbound_addr.ip(), 0u16)).await {
                                Ok(connection) => return Ok(Some(connection)),
                                Err(err) => {
                                    warn!("Error polling {}: {}", addr, err);
                                    self.delay_queue.insert(addr, self.interval);
                                }
                            }
                        }
                    }
                }
                Ok(None)
            },
            peer = self.rx.recv() => {
                if let Some(config) = peer {
                    let network = config.remote_ip;
                    self.idle_peers
                        .insert(config.remote_ip, IdlePeer::new(config));
                    if let Some(addr) = get_host_address(&network) {
                        self.delay_queue.insert(addr, self.interval);
                    }
                }
                Ok(None)
            }
        }
    }
}

impl fmt::Display for Poller {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<Poller peers={}>", self.idle_peers.len())
    }
}

fn get_config_for_peer(
    idle_peers: &HashMap<IpNetwork, IdlePeer>,
    peer: IpAddr,
) -> Option<Arc<PeerConfig>> {
    if let Some(network) = idle_peers.keys().find(|n| n.contains(peer)) {
        idle_peers.get(&network).map(|c| c.get_config())
    } else {
        None
    }
}
