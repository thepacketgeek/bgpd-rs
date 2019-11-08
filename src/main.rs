use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::process;
use std::sync::Arc;

use env_logger::Builder;
use log::{debug, error, info, trace, LevelFilter};
use signal_hook::{iterator::Signals, SIGHUP};
use structopt::StructOpt;
use tokio::net::TcpListener;
use tokio::sync::watch;

use bgpd::api::serve;
use bgpd::config;
use bgpd::handler::Server;

#[derive(StructOpt, Debug)]
#[structopt(name = "bgpd", rename_all = "kebab-case")]
/// BGPd Server
pub struct Args {
    /// Path to BGP service config.toml
    config_path: String,
    /// Host address to use for BGP service
    #[structopt(short, long, default_value = "127.0.0.1")]
    address: IpAddr,
    /// Host port to use for BGP service
    #[structopt(short, long, default_value = "179")]
    port: u16,
    /// Host address to use for HTTP API
    #[structopt(long, default_value = "127.0.0.1")]
    http_addr: IpAddr,
    /// Host port to use for HTTP API
    #[structopt(long, default_value = "8080")]
    http_port: u16,
    /// Show debug logs (additive for trace logs)
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::from_args();

    let (bgpd_level, other_level) = match args.verbose {
        0 => (LevelFilter::Info, LevelFilter::Warn),
        1 => (LevelFilter::Debug, LevelFilter::Warn),
        2 => (LevelFilter::Trace, LevelFilter::Warn),
        3 | _ => (LevelFilter::Trace, LevelFilter::Trace),
    };
    Builder::new()
        .filter(Some("bgpd"), bgpd_level)
        .filter(None, other_level)
        .init();
    info!("Logging at levels {}/{}", bgpd_level, other_level);

    let config = Arc::new(config::from_file(&args.config_path)?);
    debug!("Found {} peers in {}", config.peers.len(), args.config_path);
    trace!("Using config: {:#?}", &config);
    let (config_tx, config_rx) = watch::channel(config.clone());
    config_tx.broadcast(config.clone())?;

    let socket = SocketAddr::from((args.address, args.port));
    let bgp_listener = TcpListener::bind(&socket).await?;
    let mut bgp_server = Server::new(config, bgp_listener, config_rx)?;
    // Setup JSON RPC Server
    let state = bgp_server.clone_state();
    let http_socket: SocketAddr = (args.http_addr, args.http_port).into();
    serve(http_socket, state).await;

    let signals = Signals::new(&[SIGHUP])?;
    std::thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received {}, reloading config", sig);
            config::from_file(&args.config_path)
                .map(|new_config| config_tx.broadcast(Arc::new(new_config)))
                .map_err(|err| error!("Error reloading config: {}", err))
                .ok();
        }
    });

    // Start BGP Daemon
    info!("Starting BGPd [pid {}] on {}...", process::id(), socket);
    bgp_server.run().await
}
