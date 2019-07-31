use bgpd_lib::db::RouteDB;
use structopt::StructOpt;

mod display;
mod table;

#[derive(StructOpt, Debug)]
#[structopt(name = "bgpd-cli", rename_all = "kebab-case")]
/// CLI to interact with BGPd
struct Args {
    #[structopt(subcommand)]
    cmd: Command,
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

fn run(args: Args) -> Result<(), String> {
    match args.cmd {
        Command::Show(show) => match show {
            Show::Neighbors => {
                let db = RouteDB::new().map_err(|err| format!("{}", err))?;
                let peers = db.get_all_peers().map_err(|err| format!("{}", err))?;
                let mut table = table::OutputTable::new();
                for peer in peers.iter() {
                    let learned_routes = match peer.router_id {
                        Some(router_id) => db
                            .get_routes_for_peer(router_id)
                            .map(|routes| Some(routes.len())),
                        None => Ok(None),
                    }
                    .map_err(|err| format!("{}", err))?;
                    table.add_row(&(peer, learned_routes));
                }
                table.print();
            }
            Show::Routes(routes) => match routes {
                Routes::Learned => {
                    let routes = RouteDB::new()
                        .map_err(|err| format!("{}", err))
                        .map(|db| db.get_all_routes())?
                        .map_err(|err| format!("{}", err))?;
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
        eprintln!("Error: {}", err.to_string());
    }
}
