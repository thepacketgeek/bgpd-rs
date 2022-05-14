//! # BGPd CLI
//!
//! The CLI provided for running the BGPd daemon can also be used to interact with a running instance of BGPd.
//! It uses the default endpoint for the BGPd HTTP API (localhost:8080),
//! but you can point to BGPd running remotely using the `--host` and `--port` options.
//!
//! ## Features
//! - [x] CLI interface for viewing peer status and details
//! - [x] View learned routes (with source)
//! - [x] View advertised routes
//! - [x] Advertise IPv4/IPv6 Unicast routes (More attribute support coming soon)
//! - [x] Advertise IPv4/IPv6 Flowspec flows
//! - [ ] Filter learned/advertised routes (prefix, peer, attributes, ...)
//! - [ ] Enable/disable Peers
//!
//!
//! # Show Commands
//! Use the `bgpd` CLI for viewing peer & route information:
//!
//! ## Neighbors
//!
//! Peer summary:
//! ```sh
//! $ cargo run -- show neighbors
//!  Neighbor     Router ID    AS     MsgRcvd  MsgSent  Uptime    State        PfxRcd
//! ----------------------------------------------------------------------------------
//!  127.0.0.2    2.2.2.2      100    76       70       00:11:27  Established  4
//!  *127.0.0.3                65000                              Disabled
//!  172.16.20.2  172.16.20.2  65000  29       28       00:11:33  Established  2
//! ```
//!  > Tip: Use the `watch` command for keeping this view up-to-date
//!
//! Peer Detail:
//! ```sh
//! $ bgpd show neighbors detail
//! BGP neighbor is 127.0.0.3,  remote AS 65000, local AS 65000
//!   *Peer is Disabled
//!   Neighbor capabilities:
//!     IPv4 Unicast
//!     IPv4 Flowspec
//!     IPv6 Unicast
//!     IPv6 Flowspec
//!
//!
//! BGP neighbor is 172.16.20.2,  remote AS 65000, local AS 65000
//!   BGP version 4,  remote router-id 172.16.20.2
//!     Local address: 172.16.20.90:55687
//!     Remote address: 172.16.20.2:179
//!   BGP state = Established, up for 00:11:59
//!   Hold time is 90 (00:01:18), keepalive interval is 30
//!     Last read 00:00:03, last write 00:00:11
//!   Neighbor capabilities:
//!     Address family IPv6 Unicast
//!     Address family IPv4 Unicast
//!
//!   Message Statistics:
//!                       Sent      Received
//!     Total             30        29
//! ```
//!
//! ## Routes
//!
//! Learned routes:
//! ```sh
//! $ bgpd show routes learned
//! IPv4 / Unicast
//!  Received From  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities           Age
//! --------------------------------------------------------------------------------------------------------------------------------
//!  Config         9.9.9.0/24      172.16.20.90  00:08:00  Incomplete                                                     00:08:00
//!  172.16.20.2    172.16.20.0/24  172.16.20.2   00:07:54  IGP         100                                                00:07:54
//!  127.0.0.2      2.100.0.0/24    127.0.0.2     00:07:46  IGP                     500     100      target:65000:1.1.1.1  00:07:46
//!  127.0.0.2      2.200.0.0/24    127.0.0.2     00:07:46  IGP                             100 200                        00:07:46
//!
//! IPv6 / Flowspec
//!  Received From  Prefix                                          Next Hop  Age       Origin  Local Pref  Metric  AS Path  Communities     Age
//! --------------------------------------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      Dst: 3001:99:b::10/128, Src: 3001:99:a::10/128            00:00:39  IGP     100                          redirect:6:302  00:00:39
//!
//! IPv6 / Unicast
//!  Received From  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------------------
//!  Config         3001:404:a::/64      3001:1::1           00:08:00  Incomplete                                            00:08:00
//!  Config         3001:404:b::/64      3001:1::1           00:08:00  Incomplete                                            00:08:00
//!  172.16.20.2    3001:172:16:20::/64  ::ffff:172.16.20.2  00:07:54  IGP         100                                       00:07:54
//!  127.0.0.2      2621:a:10::/64       3001:1::1           00:07:46  IGP                             600 650               00:07:46
//!  127.0.0.2      2621:a:1337::/64     3001:1::1           00:07:46  IGP                     404     100                   00:07:46
//! ```
//!
//! Learned routes can be filtered with a peer IP Addr or prefix:
//! ```sh
//! $ bgpd show routes learned 172.16.20.2
//! IPv4 / Unicast
//!  Received From  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities           Age
//! --------------------------------------------------------------------------------------------------------------------------------
//!  172.16.20.2    172.16.20.0/24  172.16.20.2   00:07:54  IGP         100                                                00:07:54
//!
//! IPv6 / Unicast
//!  Received From  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------------------
//!  172.16.20.2    3001:172:16:20::/64  ::ffff:172.16.20.2  00:07:54  IGP         100                                       00:07:54
//! ```
//!
//! ```sh
//! $ bgpd show routes learned 127.0.0.0/24
//! IPv4 / Unicast
//!  Received From  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities           Age
//! --------------------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      2.100.0.0/24    127.0.0.2     00:07:46  IGP                     500     100      target:65000:1.1.1.1  00:07:46
//!  127.0.0.2      2.200.0.0/24    127.0.0.2     00:07:46  IGP                             100 200                        00:07:46
//!
//! IPv6 / Flowspec
//!  Received From  Prefix                                          Next Hop  Age       Origin  Local Pref  Metric  AS Path  Communities     Age
//! --------------------------------------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      Dst: 3001:99:b::10/128, Src: 3001:99:a::10/128            00:00:39  IGP     100                          redirect:6:302  00:00:39
//!
//! IPv6 / Unicast
//!  Received From  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      2621:a:10::/64       3001:1::1           00:07:46  IGP                             600 650               00:07:46
//!  127.0.0.2      2621:a:1337::/64     3001:1::1           00:07:46  IGP                     404     100                   00:07:46
//! ```
//!
//! Advertised routes:
//! ```sh
//! $ bgpd show routes advertised
//! IPv4 / Unicast
//!  Advertised To  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! -----------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      172.16.20.0/24  172.16.20.2   00:08:01  IGP         100                                       00:08:01
//!  172.16.20.2    9.9.9.0/24      172.16.20.90  00:08:06  Incomplete                                            00:08:06
//!
//! IPv6 / Unicast
//!  Advertised To  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      3001:172:16:20::/64  ::ffff:172.16.20.2  00:08:01  IGP         100                                       00:08:01
//!  172.16.20.2    3001:404:a::/64      3001:1::1           00:08:06  Incomplete                                            00:08:06
//!  172.16.20.2    3001:404:b::/64      3001:1::1           00:08:06  Incomplete                                            00:08:06
//! ```
//!
//! ## Advertise
//!
//! ### Unicast
//! IPv4 Unicast
//! ```sh
//! $ bgpd advertise route 10.10.10.0/24 172.16.20.90 --local-pref 500
//! Added route to RIB for announcement:
//!  Received From  Prefix         Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------
//!  API            10.10.10.0/24  172.16.20.90  00:00:00  Incomplete                                            00:00:00
//! ```
//!
//! IPv6 Unicast
//! ```sh
//! $ bgpd advertise route 10.10.10.0/24 172.16.20.90 --local-pref 500
//! Added route to RIB for announcement:
//!  Received From  Prefix         Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------
//!  API            10.10.10.0/24  172.16.20.90  00:00:00  Incomplete                                            00:00:00
//! ```
//!
//! ```sh
//! $ bgpd show routes advertised
//! IPv4 / Unicast
//!  Advertised To  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! -----------------------------------------------------------------------------------------------------------------------
//!  ...
//!  172.16.20.2    10.10.10.0/24   172.16.20.90  00:01:17  Incomplete                                            00:01:17
//!
//! IPv6 / Unicast
//!  Advertised To  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
//! ----------------------------------------------------------------------------------------------------------------------------------
//!  ...
//!  172.16.20.2    3001:100:abcd::/64   3001:1::1           00:00:03  Incomplete                                            00:00:03
//! ```
//!
//! ### Flowspec
//! ```sh
//! $ bgpd advertise flow ipv4 'traffic-rate 100' -m 'source 192.168.10.0/24'
//! Added flow to RIB for announcement:
//!  Received From  Prefix               Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities            Age
//! ----------------------------------------------------------------------------------------------------------------------------------
//!  Config         Src 192.168.10.0/24            00:00:00  Incomplete                               traffic-rate:0:100bps  00:00:00
//! $ bgpd advertise flow ipv6 'redirect 100:200' -m 'destination 3001:10:20::/64'
//! Added flow to RIB for announcement:
//!  Received From  Prefix               Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities       Age
//! -----------------------------------------------------------------------------------------------------------------------------
//!  Config         Dst 3001:10:20::/64            00:00:00  Incomplete                               redirect:100:200  00:00:00
//!  ```
//!
//!  ```sh
//! $ bgpd  show routes learned
//! IPv4 / Flowspec
//!  Received From  Prefix              Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities     Age
//! --------------------------------------------------------------------------------------------------------------------------
//!  Config         Src 192.168.0.0/16            00:01:55  Incomplete                               redirect:6:302  00:01:55
//!
//! IPv6 / Flowspec
//!  Received From  Prefix                    Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities              Age
//! -----------------------------------------------------------------------------------------------------------------------------------------
//!  127.0.0.2      Dst 3001:99:b::10/128               00:01:24  IGP                             200      traffic-rate:0:500bps    00:01:24
//!                 Src 3001:99:a::10/128
//!  Config         Src 3001:100::/56                   00:01:55  Incomplete                               traffic-rate:0:24000bps  00:01:55
//!                 DstPort >8000, && <=8080
//!                 Packet Length >100
//!
//! ```

use std::error::Error;
use std::net::{IpAddr, SocketAddr};

use clap::Parser;
use colored::*;
use ipnetwork::IpNetwork;
use itertools::Itertools;
use jsonrpsee::http_client::HttpClientBuilder;

use crate::api::rpc::{ApiClient, FlowSpec, RouteSpec};

mod display;
mod table;

use display::{AdvertisedRouteRow, LearnedRouteRow, PeerSummaryRow};

#[derive(Parser, Debug)]
#[clap(name = "bgpd-cli", rename_all = "kebab-case")]
/// CLI to interact with BGPd
pub struct Args {
    #[clap(subcommand)]
    pub cmd: Command,
    #[clap(long, default_value = "127.0.0.1")]
    pub host: String,
    #[clap(short, long, default_value_t = 8080)]
    pub port: u16,
    /// API Listening address/port (E.g. 127.0.0.1:8080). If not provided, will fall back to config file value
    #[clap(long)]
    pub api: Option<SocketAddr>,
    /// Show debug logs (additive for trace logs)
    #[clap(short, parse(from_occurrences), global = true)]
    pub verbose: u8,
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
/// CLI to query BGPd
pub enum Command {
    #[clap()]
    /// Run BGPd daemon
    Run(RunOptions),
    #[clap(alias = "s")]
    /// View details about BGPd
    #[clap(subcommand)]
    Show(Show),
    /// Send routes to be advertised
    #[clap(subcommand)]
    Advertise(Advertise),
}

#[derive(Parser, Debug)]
#[clap(name = "bgpd", rename_all = "kebab-case")]
/// BGPd Server
pub struct RunOptions {
    /// Path to BGP service config.toml
    pub config_path: String,
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub enum Show {
    /// View configured neighbors and session details
    /// (* prefix means peer is disabled)
    #[clap(alias = "n", visible_alias = "peers")]
    Neighbors(NeighborOptions),
    #[clap(alias = "r", subcommand)]
    Routes(Routes),
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub enum ShowOptions {
    /// Show expanded details
    #[clap(alias = "d")]
    Detail,
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct NeighborOptions {
    ///  IP Address or Network Prefix to filter peer(s)
    #[clap()]
    peer: Option<IpNetwork>,
    #[clap(subcommand)]
    options: Option<ShowOptions>,
    // #[clap()]
    // family: Option<AFI>,
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub enum Routes {
    Learned(RouteOptions),
    Advertised(RouteOptions),
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct RouteOptions {
    /// IP Address or Network Prefix to match route source
    #[clap()]
    peer: Option<IpNetwork>,
    // #[clap()]
    // family: Option<AFI>,
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub enum Advertise {
    #[clap()]
    Route(Route),
    #[clap()]
    Flow(Flow),
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct Route {
    /// Prefix to advertise
    #[clap()]
    prefix: IpNetwork,
    /// Next Hop for this route
    #[clap()]
    next_hop: IpAddr,
    /// Origin (defaults to Incomplete)
    #[clap(short, long)]
    origin: Option<String>,
    /// AS Path (e.g. --as-path 100 200 65000.100), defaults to an empty path
    #[clap(short, long, required = true)]
    as_path: Option<String>,
    /// Local Pref (defaults to 100)
    #[clap(short = 'p', long)]
    local_pref: Option<u32>,
    /// Multi-exit-discriminator
    #[clap(long)]
    med: Option<u32>,
    /// Communities (e.g. --communities 100 200 redirect:65000:100)
    #[clap(short, long)]
    communities: Option<String>,
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct Flow {
    /// AFI [ipv6, ipv4]
    #[clap()]
    family: String,
    /// Flowspec action E.g. "redirect 6:302" or "traffic-rate 302"
    #[clap()]
    action: String,
    /// Origin (defaults to Incomplete)
    #[clap(short, long)]
    matches: Vec<String>,
    /// Origin (defaults to Incomplete)
    #[clap(short, long)]
    origin: Option<String>,
    /// AS Path (e.g. --as-path 100 200 65000.100), defaults to an empty path
    #[clap(short, long)]
    as_path: Option<String>,
    /// Local Pref (defaults to 100)
    #[clap(short = 'p', long)]
    local_pref: Option<u32>,
    /// Multi-exit-discriminator
    #[clap(long)]
    med: Option<u32>,
    /// Communities (e.g. --communities 100 200 redirect:65000:100)
    #[clap(short, long)]
    communities: Option<String>,
}

async fn run_cmd(args: &Args) -> Result<(), Box<dyn Error>> {
    let client = {
        let base = format!("http://{}:{}", args.host, args.port);
        HttpClientBuilder::default().build(base)?
    };
    match &args.cmd {
        Command::Show(show) => match show {
            Show::Neighbors(options) => {
                if matches!(options.options, Some(ShowOptions::Detail)) {
                    let peers: Vec<_> = client.show_peer_detail().await?;
                    for peer in peers {
                        let summ = peer.summary;
                        let mut lines: Vec<String> = Vec::with_capacity(16);
                        lines.push(format!(
                            "BGP neighbor is {},  remote AS {}, local AS {}",
                            summ.peer, summ.remote_asn, summ.local_asn
                        ));
                        if !summ.enabled {
                            lines.push("*Peer is Disabled".to_string());
                        }
                        if let Some(router_id) = summ.router_id {
                            lines.push(format!("BGP version 4,  remote router-id {}", router_id));
                            if let Some(stream) = peer.tcp_connection {
                                lines.push(format!("  Local address: {}", stream.0));
                                lines.push(format!("  Remote address: {}", stream.1));
                            }
                            lines.push(format!(
                                "BGP state = {}, up for {}",
                                summ.state,
                                summ.uptime.unwrap(),
                            ));
                            lines.push(format!(
                                "Hold time is {} ({}), keepalive interval is {}",
                                peer.hold_timer,
                                peer.hold_time.unwrap(),
                                peer.hold_timer_interval,
                            ));
                            lines.push(format!(
                                "  Last read {}, last write {}",
                                peer.last_received.unwrap(),
                                peer.last_sent.unwrap(),
                            ));
                        }
                        lines.push("Neighbor capabilities:".to_string());
                        for capability in &peer.capabilities {
                            lines.push(format!("  {}", capability));
                        }
                        lines.push("".to_owned());
                        if let (Some(sent), Some(rcvd)) = (summ.msg_received, summ.msg_sent) {
                            lines.push("Message Statistics:".to_string());
                            lines.push("                    Sent      Received".to_string());
                            lines.push(format!("  Total             {}        {}", sent, rcvd));
                        }
                        println!("{}\n", lines.join("\n  "));
                    }
                } else {
                    let peers: Vec<_> = client
                        .show_peers()
                        .await?
                        .into_iter()
                        .map(PeerSummaryRow)
                        .collect();
                    let mut table = table::OutputTable::new();
                    for peer in peers {
                        table.add_row(&peer)?;
                    }
                    table.print();
                }
            }
            Show::Routes(routes) => match routes {
                Routes::Learned(options) => {
                    let mut routes: Vec<_> = client.show_routes_learned(options.peer).await?;
                    routes.sort_by_key(|r| (r.afi.clone(), r.safi.clone()));
                    for (afi, routes) in &routes.into_iter().group_by(|r| r.afi.clone()) {
                        for (safi, routes) in &routes.group_by(|r| r.safi.clone()) {
                            println!("{} / {}", afi, safi);
                            let mut table = table::OutputTable::new();
                            for route in routes {
                                table.add_row(&LearnedRouteRow(route))?;
                            }
                            table.print();
                            println!();
                        }
                    }
                }
                Routes::Advertised(options) => {
                    let mut routes: Vec<_> = client.show_routes_advertised(options.peer).await?;
                    routes.sort_by_key(|r| (r.afi.clone(), r.safi.clone()));
                    for (afi, routes) in &routes.into_iter().group_by(|r| r.afi.clone()) {
                        for (safi, routes) in &routes.group_by(|r| r.safi.clone()) {
                            println!("{} / {}", afi, safi);
                            let mut table = table::OutputTable::new();
                            for route in routes {
                                table.add_row(&AdvertisedRouteRow(route))?;
                            }
                            table.print();
                            println!();
                        }
                    }
                }
            },
        },
        Command::Advertise(advertise) => match advertise {
            Advertise::Route(route) => {
                let mut spec = RouteSpec::new(route.prefix, route.next_hop);
                if let Some(origin) = &route.origin {
                    spec.attributes.origin = Some(origin.to_string());
                }
                if let Some(local_pref) = &route.local_pref {
                    spec.attributes.local_pref = Some(*local_pref);
                }
                if let Some(med) = &route.med {
                    spec.attributes.multi_exit_disc = Some(*med);
                }
                if let Some(as_path) = &route.as_path {
                    spec.attributes.as_path =
                        as_path.split(' ').map(|asn| asn.to_string()).collect();
                }
                if let Some(communities) = &route.communities {
                    spec.attributes.communities = communities
                        .split(' ')
                        .map(|comm| comm.to_string())
                        .collect();
                }
                match client.advertise_route(spec).await {
                    Ok(advertised) => {
                        println!("Added route to RIB for announcement:");
                        let mut table = table::OutputTable::new();
                        table.add_row(&LearnedRouteRow(advertised))?;
                        table.print();
                    }
                    Err(err) => eprintln!("Error adding route: {}", err),
                }
            }
            Advertise::Flow(flow) => {
                let afi = match flow.family.to_lowercase().as_str() {
                    "ipv4" => 1,
                    "ipv6" => 2,
                    _ => {
                        eprintln!("Invalid family, must be one of: [ipv4, ipv6]");
                        return Ok(()); // TODO, return err
                    }
                };
                let mut spec = FlowSpec::new(afi, flow.action.to_string(), flow.matches.clone());
                if let Some(origin) = &flow.origin {
                    spec.attributes.origin = Some(origin.to_string());
                }
                if let Some(local_pref) = &flow.local_pref {
                    spec.attributes.local_pref = Some(*local_pref);
                }
                if let Some(med) = &flow.med {
                    spec.attributes.multi_exit_disc = Some(*med);
                }
                if let Some(as_path) = &flow.as_path {
                    spec.attributes.as_path =
                        as_path.split(' ').map(|asn| asn.to_string()).collect();
                }
                if let Some(communities) = &flow.communities {
                    spec.attributes.communities = communities
                        .split(' ')
                        .map(|comm| comm.to_string())
                        .collect();
                }
                match client.advertise_flow(spec).await {
                    Ok(advertised) => {
                        println!("Added flow to RIB for announcement:");
                        let mut table = table::OutputTable::new();
                        table.add_row(&LearnedRouteRow(advertised))?;
                        table.print();
                    }
                    Err(err) => eprintln!("Error adding flow: {}", err),
                }
            }
        },
        _ => unimplemented!(), // ::Run should never get called since it's handled in main
    }
    Ok(())
}

/// BGPd interactive commands (other than running the daemon)
pub async fn query_bgpd(args: &Args) {
    if let Err(err) = run_cmd(args).await {
        eprintln!("{}", err.to_string().red());
    }
}
