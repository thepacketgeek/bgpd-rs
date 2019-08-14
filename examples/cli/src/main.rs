use colored::*;
use reqwest::Url;
use serde_json::{self, Value};
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
    port: u32,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
/// CLI to query BGPd
enum Command {
    #[structopt()]
    /// View details for a location
    Show(Show),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum Show {
    #[structopt(visible_alias = "peers")]
    Neighbors,
    #[structopt()]
    Routes(Routes),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum Routes {
    #[structopt()]
    Learned,
    #[structopt()]
    Advertised,
}

fn fetch_url(uri: Url) -> Result<String, String> {
    reqwest::get(uri)
        .and_then(|mut resp| resp.text())
        .map_err(|err| err.to_string())
}

fn run(args: Args) -> Result<(), String> {
    let base_url = {
        let base = format!("http://{}:{}", args.host, args.port);
        Url::parse(&base).expect("Must provide valid host & port")
    };
    match args.cmd {
        Command::Show(show) => match show {
            Show::Neighbors => {
                let body = fetch_url(base_url.join("show/neighbors").unwrap())?;
                let peers: Value = serde_json::from_str(&body[..]).unwrap();
                let peers = match peers {
                    Value::Array(peers) => {
                        let peers: Vec<PeerSummaryRow> =
                            peers.into_iter().map(PeerSummaryRow).collect();
                        peers
                    }
                    _ => unreachable!(),
                };
                let mut table = table::OutputTable::new();
                for peer in peers.iter() {
                    table.add_row(&peer).map_err(|err| err.to_string())?;
                }
                table.print();
            }
            Show::Routes(routes) => match routes {
                Routes::Learned => {
                    let body = fetch_url(base_url.join("show/routes/learned").unwrap())?;
                    let routes: Value = serde_json::from_str(&body[..]).unwrap();
                    let routes = match routes {
                        Value::Array(routes) => {
                            let routes: Vec<LearnedRouteRow> =
                                routes.into_iter().map(LearnedRouteRow).collect();
                            routes
                        }
                        _ => unreachable!(),
                    };
                    let mut table = table::OutputTable::new();
                    for route in routes.iter() {
                        table.add_row(&route).map_err(|err| err.to_string())?;
                    }
                    table.print();
                }
                Routes::Advertised => {
                    let body = fetch_url(base_url.join("show/routes/advertised").unwrap())?;
                    let routes: Value = serde_json::from_str(&body[..]).unwrap();
                    let routes = match routes {
                        Value::Array(routes) => {
                            let routes: Vec<AdvertisedRouteRow> =
                                routes.into_iter().map(AdvertisedRouteRow).collect();
                            routes
                        }
                        _ => unreachable!(),
                    };
                    let mut table = table::OutputTable::new();
                    for route in routes.iter() {
                        table.add_row(&route).map_err(|err| err.to_string())?;
                    }
                    table.print();
                }
            },
        },
    }
    Ok(())
}

fn main() {
    let args = Args::from_args();
    if let Err(err) = run(args) {
        eprintln!("{}", err.to_string().red());
    }
}
