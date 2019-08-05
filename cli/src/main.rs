use bgpd_lib::models::{PeerSummary, Route};
use reqwest::Url;
use serde_json;
use structopt::StructOpt;

mod display;
mod table;

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
    #[structopt()]
    Neighbors,
    #[structopt()]
    Routes(Routes),
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
enum Routes {
    #[structopt()]
    Learned,
}

fn fetch_url(uri: Url) -> Result<String, String> {
    reqwest::get(uri)
        .and_then(|mut resp| resp.text())
        .map_err(|err| format!("{}", err))
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
                let peers: Vec<PeerSummary> = serde_json::from_str(&body).unwrap();
                let mut table = table::OutputTable::new();
                for peer in peers.iter() {
                    table.add_row(&peer);
                }
                table.print();
            }
            Show::Routes(routes) => match routes {
                Routes::Learned => {
                    let body = fetch_url(base_url.join("show/routes/learned").unwrap())?;
                    let routes: Vec<Route> = serde_json::from_str(&body).unwrap();
                    let mut table = table::OutputTable::new();
                    for route in routes.iter() {
                        table.add_row(route);
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
        eprintln!("{}", err.to_string());
    }
}
