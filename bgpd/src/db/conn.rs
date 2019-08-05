use std::convert::{From, TryInto};
use std::net::IpAddr;
use std::string::ToString;

use bgp_rs::Prefix;
use bgpd_lib::utils::as_path_to_string;
use log::{error, trace};
use rusqlite::types::ToSql;
use rusqlite::{params, Connection, Result, NO_PARAMS};

use super::DBTable;
use crate::models::{PeerSummary, Route};

pub struct DB {
    conn: Connection,
}

impl DB {
    pub fn new() -> Result<Self> {
        let conn = Connection::open("/tmp/bgpd.sqlite3")?;
        PeerSummary::create_table(&conn)?;
        Route::create_table(&conn)?;
        Ok(Self { conn })
    }

    pub fn get_all_routes(&self) -> Result<Vec<Route>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT
                router_id, received_at, prefix, next_hop,
                origin, as_path, local_pref, metric, communities
            FROM routes ORDER BY router_id ASC, prefix ASC"#,
        )?;
        let route_iter = stmt.query_map(NO_PARAMS, |row| row.try_into())?;
        let mut routes: Vec<Route> = Vec::new();
        for route in route_iter {
            match route {
                Ok(route) => routes.push(route),
                Err(err) => error!("Error parsing route in RouteDB: {}", err),
            }
        }
        Ok(routes)
    }

    pub fn get_routes_for_peer(&self, router_id: IpAddr) -> Result<Vec<Route>> {
        trace!("Getting routes for peer {}", router_id);
        let mut stmt = self.conn.prepare(
            r#"SELECT
                router_id, received_at, prefix, next_hop,
                origin, as_path, local_pref, metric, communities
            FROM routes WHERE router_id = ?1"#,
        )?;
        let route_iter = stmt.query_map(&[&router_id.to_string()], |row| row.try_into())?;
        let mut routes: Vec<Route> = Vec::new();
        for route in route_iter {
            match route {
                Ok(route) => routes.push(route),
                Err(err) => error!("Error parsing route in RouteDB: {}", err),
            }
        }
        Ok(routes)
    }

    pub fn insert_routes(&self, routes: Vec<Route>) -> Result<()> {
        trace!("Inserting routes: {}", routes.len());
        for route in routes {
            let as_path = as_path_to_string(&route.as_path);
            self.conn.execute(
                r#"REPLACE INTO routes
                    (router_id, received_at, prefix, next_hop,
                    origin, as_path, local_pref, metric, communities)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
                &[
                    &route.received_from.to_string(),
                    &route.received_at.timestamp().to_string(),
                    &route.prefix.to_string(),
                    &route.next_hop.to_string(),
                    &((&route.origin).into()),
                    &as_path,
                    &route.local_pref as &ToSql,
                    &route.multi_exit_disc as &ToSql,
                    &route.communities as &ToSql,
                ],
            )?;
            trace!("\t{:?}", route);
        }
        Ok(())
    }

    pub fn remove_prefixes_from_peer(&self, router_id: IpAddr, prefixes: &[Prefix]) -> Result<()> {
        trace!("Removing prefixes [{}]: {}", router_id, prefixes.len());
        for prefix in prefixes {
            let addr = IpAddr::from(prefix);
            self.conn.execute(
                "DELETE FROM routes WHERE router_id = ?1 AND prefix = ?2",
                &[&router_id.to_string(), &addr.to_string()],
            )?;
            trace!("\t{:?}", prefix);
        }
        Ok(())
    }

    pub fn remove_routes_for_peer(&self, router_id: IpAddr) -> Result<()> {
        trace!("Removing routes from peer {}", router_id);
        self.conn.execute(
            "DELETE FROM routes WHERE router_id = ?1",
            &[&router_id.to_string()],
        )?;
        Ok(())
    }

    pub fn get_all_peers(&self) -> Result<Vec<PeerSummary>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT
                peers.neighbor, peers.router_id, peers.asn, peers.msg_received,
                peers.msg_sent, peers.connect_time, peers.state, routes.prefixes_received 
            FROM peers 
            LEFT OUTER JOIN
                (SELECT router_id, count(*) AS prefixes_received 
                FROM routes GROUP BY router_id) routes     
            ON peers.router_id = routes.router_id 
            ORDER BY neighbor ASC"#,
        )?;
        let peer_iter = stmt.query_map(NO_PARAMS, |row| row.try_into())?;
        let mut peers: Vec<PeerSummary> = Vec::new();
        for peer in peer_iter {
            match peer {
                Ok(peer) => peers.push(peer),
                Err(err) => error!("Error parsing peer in RouteDB: {}", err),
            }
        }
        Ok(peers)
    }

    pub fn update_peer(&self, status: &PeerSummary) -> Result<()> {
        trace!("Updating peer {}", status.neighbor);
        self.conn.execute(
            r#"INSERT OR REPLACE INTO peers
                (neighbor, router_id, asn, msg_received,
                msg_sent, connect_time, state)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                &status.neighbor.to_string(),
                status
                    .router_id
                    .map(|router_id| Some(router_id.to_string()))
                    .unwrap_or(None),
                &status.asn,
                &status
                    .msg_received
                    .map(|count| Some(count as i64))
                    .unwrap_or(None),
                &status
                    .msg_sent
                    .map(|count| Some(count as i64))
                    .unwrap_or(None),
                &status
                    .connect_time
                    .map(|timestamp| Some(timestamp.timestamp().to_string()))
                    .unwrap_or(None),
                &status.state.to_string(),
            ],
        )?;
        Ok(())
    }
}
