use std::convert::{From, Into, TryFrom, TryInto};
use std::net::IpAddr;

use bgp_rs::{ASPath, Origin, Prefix, Segment};
use chrono::{DateTime, TimeZone, Utc};
use log::{error, trace};
use prettytable::{cell, row, Row as PTRow};
use rusqlite::types::Type;
use rusqlite::{Connection, Error as RError, Result, Row, NO_PARAMS};

use crate::display::ToRow;
use crate::utils::format_elapsed_time;

#[derive(Debug)]
pub struct Route {
    pub received_from: IpAddr, // router_id
    pub received_at: DateTime<Utc>,
    pub prefix: IpAddr,
    pub next_hop: IpAddr,
    pub origin: Origin,
    pub as_path: ASPath,
}

impl<'a> TryFrom<&Row<'a>> for Route {
    type Error = RError;

    fn try_from(row: &Row) -> std::result::Result<Self, Self::Error> {
        let received_from = row
            .get(0)
            .map(|prefix: String| prefix.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(0, Type::Text, Box::new(err)))?;
        let received_at = row
            .get(1)
            .map(|timestamp: i64| Utc.timestamp(timestamp, 0))?;
        let prefix = row
            .get(2)
            .map(|prefix: String| prefix.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(2, Type::Text, Box::new(err)))?;
        let next_hop = row
            .get(3)
            .map(|next_hop: String| next_hop.parse::<IpAddr>())?
            .map_err(|err| RError::FromSqlConversionFailure(3, Type::Text, Box::new(err)))?;
        let origin = row
            .get(4)
            .map(|origin: String| Origin::try_from(&origin[..]))?
            .map_err(|err| RError::FromSqlConversionFailure(4, Type::Text, Box::new(err)))?;
        let as_path = row
            .get(5)
            .map(|as_path: String| as_path_from_string(&as_path))?
            .map_err(|err| RError::FromSqlConversionFailure(5, Type::Text, Box::new(err)))?;
        Ok(Route {
            received_from,
            received_at,
            prefix,
            next_hop,
            origin,
            as_path,
        })
    }
}

impl ToRow for Route {
    fn columns() -> PTRow {
        row!["Neighbor", "AFI", "Prefix", "Next Hop", "Age", "Origin", "AS Path"]
    }

    fn to_row(&self) -> PTRow {
        let duration = Utc::now().signed_duration_since(self.received_at);
        let afi = match self.prefix {
            IpAddr::V4(_) => "IPv4",
            IpAddr::V6(_) => "IPv6",
        };
        let elapsed = duration
            .to_std()
            .map(format_elapsed_time)
            .unwrap_or_else(|_| duration.to_string());
        // TODO, this just displays the first segment as space delimited ASNs
        // Should it display more?
        let as_path = match self.as_path.segments.iter().next() {
            Some(segment) => {
                let path = match segment {
                    Segment::AS_SEQUENCE(sequence) => sequence,
                    Segment::AS_SET(set) => set,
                };
                path.iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(" ")
            }
            None => String::from(""),
        };
        row![
            self.received_from,
            afi,
            self.prefix,
            self.next_hop,
            elapsed,
            String::from(&self.origin),
            &as_path,
        ]
    }
}

pub struct RouteDB {
    conn: Connection,
}

impl RouteDB {
    pub fn new() -> Result<RouteDB> {
        let conn = Connection::open("/tmp/bgpd.sqlite3")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS routes (
                id INTEGER PRIMARY KEY,
                router_id TEXT NOT NULL,
                received_at BIGINT NOT NULL,
                prefix TEXT NOT NULL,
                next_hop TEXT NOT NULL,
                origin TEXT NOT NULL,
                as_path TEXT NOT NULL
            )",
            NO_PARAMS,
        )?;
        Ok(RouteDB { conn })
    }

    pub fn get_all_routes(&self) -> Result<Vec<Route>> {
        let mut stmt = self.conn.prepare(
            r###"SELECT router_id, received_at, prefix, next_hop, origin, as_path FROM
            routes ORDER BY router_id ASC, prefix ASC"###,
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
            "SELECT router_id, received_at, prefix, next_hop, origin, as_path FROM routes WHERE router_id = ?1",
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
                "INSERT INTO routes (router_id, received_at, prefix, next_hop, origin, as_path)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                &[
                    &route.received_from.to_string(),
                    &route.received_at.timestamp().to_string(),
                    &route.prefix.to_string(),
                    &route.next_hop.to_string(),
                    &((&route.origin).into()),
                    &as_path,
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
}

pub fn as_path_to_string(as_path: &ASPath) -> String {
    fn asns_to_string(asns: &[u32]) -> String {
        asns.iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(",")
    }

    fn segment_to_string(segment: &Segment) -> String {
        match segment {
            Segment::AS_SEQUENCE(sequence) => format!("seq:{}", asns_to_string(&sequence)),
            Segment::AS_SET(set) => format!("set:{}", asns_to_string(&set)),
        }
    }

    as_path
        .segments
        .iter()
        .map(segment_to_string)
        .collect::<Vec<String>>()
        .join(";")
}

pub fn as_path_from_string(as_path: &str) -> std::result::Result<ASPath, std::num::ParseIntError> {
    fn segment_from_string(
        segment: &str,
    ) -> std::result::Result<Option<Segment>, std::num::ParseIntError> {
        if let Some(i) = segment.find(':') {
            let (segment_type, paths) = segment.split_at(i + 1);
            let paths = paths
                .split(',')
                .collect::<Vec<&str>>()
                .into_iter()
                .map(|asn: &str| asn.parse::<u32>().unwrap())
                .collect();
            if segment_type.starts_with("seq") {
                Ok(Some(Segment::AS_SEQUENCE(paths)))
            } else {
                Ok(Some(Segment::AS_SET(paths)))
            }
        } else {
            Ok(None)
        }
    }

    let parts = as_path.split(';').collect::<Vec<&str>>();
    let mut segments: Vec<Segment> = Vec::new();
    for part in parts {
        match segment_from_string(part) {
            Ok(Some(segment)) => segments.push(segment),
            Err(err) => return Err(err),
            _ => continue,
        }
    }
    Ok(ASPath { segments })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_path_from_string() {
        let as_path = as_path_from_string("seq:100,200").unwrap();
        if let Segment::AS_SEQUENCE(seq) = &as_path.segments[0] {
            assert_eq!(seq, &vec![100 as u32, 200 as u32]);
        } else {
            panic!("Segment sequence did not match!");
        }

        let as_path = as_path_from_string("").unwrap();
        assert_eq!(as_path.segments.len(), 0);
    }

    #[test]
    fn test_as_path_to_string() {
        let as_path = ASPath {
            segments: vec![Segment::AS_SEQUENCE(vec![100, 200])],
        };
        let as_path_str = as_path_to_string(&as_path);
        assert_eq!(&as_path_str, "seq:100,200");

        let as_path_str = as_path_to_string(&ASPath { segments: vec![] });
        assert_eq!(&as_path_str, "");
    }
}
