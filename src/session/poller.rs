use std::collections::HashMap;
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::FutureExt;
use futures::{pin_mut, select};
use log::{debug, trace, warn};
use net2::TcpBuilder;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::timer::DelayQueue;
use tokio_net::driver::Handle;

use crate::config::PeerConfig;

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

    pub fn is_enabled(&self) -> bool {
        self.0.enabled
    }

    pub fn is_passive(&self) -> bool {
        self.0.passive
    }

    async fn connect(
        &self,
        source_addr: SocketAddr,
    ) -> Result<(TcpStream, Arc<PeerConfig>), io::Error> {
        let peer_addr = SocketAddr::new(self.0.remote_ip, self.0.dest_port);
        let builder = match peer_addr {
            SocketAddr::V4(_) => TcpBuilder::new_v4().unwrap(),
            SocketAddr::V6(_) => TcpBuilder::new_v6().unwrap(),
        };
        builder.reuse_address(true)?;
        builder.bind(source_addr)?;
        let handle = &Handle::default();
        let connect = TcpStream::connect_std(builder.to_tcp_stream().unwrap(), &peer_addr, handle)
            .timeout(Duration::from_millis(TCP_INIT_TIMEOUT_MS.into()));

        match connect.await? {
            Ok(stream) => Ok((stream, self.0.clone())),
            Err(err) => Err(err),
        }
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
    idle_peers: HashMap<IpAddr, IdlePeer>,
    tcp_listener: TcpListener,
    rx: PollerRx,
    interval: Duration,
    delay_queue: DelayQueue<IpAddr>,
}

impl Poller {
    pub fn new(listener: TcpListener, interval: u32 /* seconds */, rx: PollerRx) -> Self {
        let mut delay_queue = DelayQueue::with_capacity(4);
        // Add an empty IP in a year so delay_queue is never empty
        delay_queue.insert_at(
            IpAddr::from(Ipv4Addr::new(0, 0, 0, 0)),
            Instant::now() + Duration::from_secs(31536000),
        );
        Self {
            idle_peers: HashMap::new(),
            tcp_listener: listener,
            interval: Duration::from_secs(interval.into()),
            delay_queue,
            rx,
        }
    }

    pub fn upsert_peer(&mut self, config: Arc<PeerConfig>) {
        let addr = config.remote_ip;

        if let Some(_existing_peer) = self
            .idle_peers
            .insert(config.remote_ip, IdlePeer::new(config))
        {
            debug!("Peer config for {} updated", addr);
        }
        self.delay_queue.insert(addr, self.interval);
    }

    pub async fn get_connection(
        &mut self,
    ) -> Result<Option<(TcpStream, Arc<PeerConfig>)>, io::Error> {
        let local_outbound_addr = self.tcp_listener.local_addr().expect("Has local address");
        let listener = self
            .tcp_listener
            .accept()
            .timeout(Duration::from_millis(TCP_INIT_TIMEOUT_MS.into()))
            .fuse();

        // TODO: If DelayQueue.is_empty(), CPU spikes to 100%
        //       Look into returning a stream::pending() and remove
        //       insert() call in `new()`
        let initializer = self.delay_queue.next().fuse();
        let rescheduled_peers = self.rx.recv().fuse();
        pin_mut!(listener, initializer, rescheduled_peers);
        select! {
            incoming = listener => {
                if let Ok(Ok((stream, socket))) = incoming {
                    let should_connect = match self.idle_peers.get(&socket.ip()) {
                        Some(peer) => peer.is_enabled(),
                        None => false,
                    };
                    if should_connect {
                        let peer = self.idle_peers.remove(&socket.ip()).expect("Idle peer exists");
                        debug!("Incoming new connection from {}", socket.ip());
                        return Ok(Some((stream, peer.get_config())));
                    } else {
                        warn!(
                            "Unexpected connection from {}: Not a configured peer",
                            socket.ip(),
                        );
                    }
                }
                Ok(None)
            },
            outgoing = initializer => {
                if let Some(Ok(peer)) = outgoing {
                    let addr = peer.into_inner();
                    trace!("Poller outbound triggered for {}", addr);
                    // Peer may not be present if an incoming connection
                    // was established simultaneously
                    if let Some(mut peer) = self.idle_peers.get(&addr) {
                        if peer.is_enabled() && !peer.is_passive() {
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
            peer = rescheduled_peers => {
                if let Some(config) = peer {
                    let addr = config.remote_ip;
                    self.idle_peers
                        .insert(config.remote_ip, IdlePeer::new(config));
                    self.delay_queue.insert(addr, self.interval);
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
