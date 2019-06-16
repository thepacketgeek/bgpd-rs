#![allow(unused_imports)]

use std::io::Result;
use std::net::IpAddr;

use clap::{App, Arg};
use env_logger::Builder;
use log::{debug, info, LevelFilter};

use bgpd::{serve, ServerConfig};

fn main() -> Result<()> {
    let matches = App::new("bgpd")
        .version("0.1")
        .author("Mat W. <mat@thepacketgeek.com>")
        .about("BGP Server")
        .arg(
            Arg::with_name("configpath")
                .short("c")
                .long("config-path")
                .help("Path to bgpd server config")
                .takes_value(true)
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name("address")
                .short("a")
                .long("address")
                .help("IP Address to listen on")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true)
                .help("TCP Port to listen on"),
        )
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of logging verbosity"),
        )
        .get_matches();

    let config_path: String = matches
        .value_of("configpath")
        .unwrap()
        .parse()
        .expect("Must specify a valid config path");

    let addr: IpAddr = matches
        .value_of("address")
        .unwrap_or("127.0.0.1")
        .parse()
        .expect("Must specify a valid IP Address");
    let port: u16 = matches
        .value_of("port")
        .unwrap_or("179")
        .parse()
        .expect("Port must be an integer");

    let (bgpd_level, other_level) = match matches.occurrences_of("v") {
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

    let config = ServerConfig::from_file(&config_path)?;
    debug!("Found {} peers in {}", config.peers.len(), config_path);

    // TODO, setup Server like this
    // let server = Server(config);
    // server.serve(addr, port)?;
    serve(addr, port, config)?;

    Ok(())
}
