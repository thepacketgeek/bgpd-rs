use std::error::Error;
use std::net::{IpAddr, SocketAddr};

use bgpd_rpc_lib as rpc;
use colored::*;
use itertools::Itertools;
use structopt::StructOpt;

mod display;
mod table;

use display::{AdvertisedRouteRow, LearnedRouteRow, PeerSummaryRow};

#[derive(StructOpt, Debug)]
#[structopt(name = "bgpd-cli", rename_all = "kebab-case")]
/// CLI to interact with BGPd
struct Args {
    #[structopt(subcommand)]
    cmd: Command,
    #[structopt(short, long, default_value = "127.0.0.1")]
    host: String,
    #[structopt(short, long, default_value = "8080")]
    port: u16,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
/// CLI to query BGPd
enum Command {
    #[structopt()]
    /// View details about BGPd
    Show(Show),
    /// Send routes to be advertised
    Advertise(Advertise),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum Show {
    /// View configured neighbors and session details
    /// (* prefix means peer is disabled)
    #[structopt(visible_alias = "peers")]
    Neighbors(NeighborOptions),
    #[structopt()]
    Routes(Routes),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct NeighborOptions {
    #[structopt()]
    detail: bool,
    // #[structopt()]
    // family: Option<AFI>,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum Routes {
    #[structopt()]
    Learned(RouteOptions),
    #[structopt()]
    Advertised(RouteOptions),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct RouteOptions {
    #[structopt()]
    peer: Option<IpAddr>,
    // #[structopt()]
    // family: Option<AFI>,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum Advertise {
    #[structopt()]
    Route(Route),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
struct Route {
    #[structopt()]
    prefix: String,
    #[structopt()]
    next_hop: IpAddr,
    // #[structopt(long)]
    // peer: Option<IpAddr>,
    #[structopt(long)]
    origin: Option<String>,
    #[structopt(long)]
    as_path: Option<String>,
    #[structopt(long)]
    local_pref: Option<u32>,
    #[structopt(long)]
    med: Option<u32>,
}

async fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let mut client = {
        let base = format!("http://{}:{}", args.host, args.port);
        jsonrpsee::http_client(&base)
    };
    match args.cmd {
        Command::Show(show) => match show {
            Show::Neighbors(options) => {
                if options.detail {
                    let peers: Vec<_> = rpc::Api::show_peer_detail(&mut client).await?;
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
                        lines.push(format!("",));
                        if let (Some(sent), Some(rcvd)) = (summ.msg_received, summ.msg_sent) {
                            lines.push("Message Statistics:".to_string());
                            lines.push("                    Sent      Received".to_string());
                            lines.push(format!("  Total             {}        {}", sent, rcvd));
                        }
                        println!("{}\n", lines.join("\n  "));
                    }
                } else {
                    let peers: Vec<_> = rpc::Api::show_peers(&mut client)
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
                    let mut routes: Vec<_> =
                        rpc::Api::show_routes_learned(&mut client, options.peer).await?;
                    routes.sort_by_key(|r| (r.afi.clone(), r.safi.clone()));
                    for (afi, routes) in &routes.into_iter().group_by(|r| r.afi.clone()) {
                        for (safi, routes) in &routes.into_iter().group_by(|r| r.safi.clone()) {
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
                    let mut routes: Vec<_> =
                        rpc::Api::show_routes_advertised(&mut client, options.peer).await?;
                    routes.sort_by_key(|r| (r.afi.clone(), r.safi.clone()));
                    for (afi, routes) in &routes.into_iter().group_by(|r| r.afi.clone()) {
                        for (safi, routes) in &routes.into_iter().group_by(|r| r.safi.clone()) {
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
                let to_advertise = rpc::AdvertiseRoute::new(route.prefix, route.next_hop);
                match rpc::Api::advertise_route(&mut client, to_advertise).await? {
                    Ok(advertised) => {
                        println!("Added route to RIB for announcement:");
                        let mut table = table::OutputTable::new();
                        table.add_row(&LearnedRouteRow(advertised))?;
                        table.print();
                    }
                    Err(err) => eprintln!("Error adding route: {}", err.to_string()),
                }
            }
        },
    }
    Ok(())
}

fn main() {
    let args = Args::from_args();
    let result = async_std::task::block_on(async { run(args).await });
    if let Err(err) = result {
        eprintln!("{}", err.to_string().red());
    }
}
