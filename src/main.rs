use std::io::Result;
use std::net::{IpAddr, SocketAddr};

use env_logger::Builder;
use futures::future::Future;
use log::{debug, info, LevelFilter};
use structopt::StructOpt;
use tokio::runtime::Runtime;
use tower_web::ServiceBuilder;

use bgpd::{Server, ServerConfig, API};

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

fn main() -> Result<()> {
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

    let config = ServerConfig::from_file(&args.config_path)?;
    debug!("Found {} peers in {}", config.peers.len(), args.config_path);

    let mut runtime = Runtime::new().unwrap();
    let socket = SocketAddr::from((args.address, args.port));
    let server = Server::from_config(socket, config)?;
    info!("Starting BGP server on {}...", socket);
    runtime.spawn(server);

    // ctrlc::set_handler(move || {
    //     info!("Stopping BGPd...");
    //     // Remove DB
    //     // std::fs::remove_file("/tmp/bgpd.sqlite3").expect("Error deleting DB");
    //     // std::process::exit(0);
    // })
    // .expect("Error setting Ctrl-C handler");

    let http_socket = (args.http_addr, args.http_port).into();
    ServiceBuilder::new()
        .resource(API)
        .run(&http_socket)
        .unwrap();
    runtime.shutdown_on_idle().wait().unwrap();
    Ok(())
}
