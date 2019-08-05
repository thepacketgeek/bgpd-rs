#![allow(unused_imports)]

use std::io::Result;
use std::net::IpAddr;

use env_logger::Builder;
use futures::future::Future;
use hyper::service::service_fn;
use hyper::Server;
use log::{debug, info, LevelFilter};
use structopt::StructOpt;
use tokio::runtime::Runtime;

use super::{handle_api_request, serve, ServerConfig};

#[derive(StructOpt, Debug)]
#[structopt(name = "bgpd", rename_all = "kebab-case")]
/// BGPd Server
struct Args {
    /// Path to bgpd server config
    config_path: String,
    #[structopt(short, long, default_value = "127.0.0.1")]
    /// Path to bgpd server config
    address: IpAddr,
    #[structopt(short, long, default_value = "179")]
    /// Path to bgpd server config
    port: u16,
    #[structopt(short, parse(from_occurrences))]
    /// Path to bgpd server config
    verbose: u8,
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

    let addr = ([127, 0, 0, 1], 8080).into();

    let server = Server::bind(&addr)
        .serve(|| service_fn(handle_api_request))
        .map_err(|e| eprintln!("server error: {}", e));

    runtime.spawn(server);
    println!("HERE!");
    let mut runtime = serve(args.address, args.port, config, runtime)?;

    runtime.shutdown_on_idle().wait().unwrap();

    Ok(())
}
