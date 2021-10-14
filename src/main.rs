use std::error::Error;
use std::process;
use std::sync::Arc;

use env_logger::Builder;
use log::{debug, error, info, trace, LevelFilter};
use signal_hook::{iterator::Signals, SIGHUP};
use structopt::StructOpt;
use tokio::net::TcpListener;
use tokio::sync::watch;

use bgpd_rs::cli;
use bgpd_rs::config;
use bgpd_rs::handler::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Args::from_args();

    let (bgpd_level, other_level) = match args.verbose {
        0 => (LevelFilter::Info, LevelFilter::Warn),
        1 => (LevelFilter::Debug, LevelFilter::Warn),
        2 => (LevelFilter::Trace, LevelFilter::Warn),
        _ => (LevelFilter::Trace, LevelFilter::Trace),
    };
    Builder::new()
        .filter(Some("bgpd"), bgpd_level)
        .filter(None, other_level)
        .init();
    info!("Logging at levels {}/{}", bgpd_level, other_level);

    match args.cmd {
        cli::Command::Run(opts) => {
            let config = Arc::new(config::from_file(&opts.config_path)?);
            debug!("Found {} peers in {}", config.peers.len(), opts.config_path);
            trace!("Using config: {:#?}", &config);
            let (config_tx, config_rx) = watch::channel(config.clone());
            config_tx.send(config.clone())?;

            let bgp_listener = TcpListener::bind(&config.bgp_socket).await?;
            let mut bgp_server = Server::new(config.clone(), bgp_listener, config_rx)?;
            // Setup JSON RPC Server
            let _api_handle = bgp_server
                .serve_rpc_api(args.api.unwrap_or(config.api_socket))
                .await?;

            let signals = Signals::new(&[SIGHUP])?;
            std::thread::spawn(move || {
                for sig in signals.forever() {
                    info!("Received {}, reloading config", sig);
                    config::from_file(&opts.config_path)
                        .map(|new_config| config_tx.send(Arc::new(new_config)))
                        .map_err(|err| error!("Error reloading config: {}", err))
                        .ok();
                }
            });

            // Start BGP Daemon
            info!(
                "Starting BGPd [pid {}] on {}...",
                process::id(),
                config.bgp_socket
            );
            bgp_server.run().await?;
        }
        _ => cli::query_bgpd(&args).await,
    }
    Ok(())
}
