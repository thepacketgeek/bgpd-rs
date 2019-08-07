use crate::models::{PeerSummary, Route};
use rusqlite::{Connection, Result, NO_PARAMS};

use super::DBTable;

impl DBTable for Route {
    fn create_table(conn: &Connection) -> Result<usize> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS routes (
                id INTEGER PRIMARY KEY,
                router_id TEXT NOT NULL,
                received_at BIGINT NOT NULL,
                prefix TEXT NOT NULL,
                next_hop TEXT NOT NULL,
                origin TEXT NOT NULL,
                as_path TEXT NOT NULL,
                local_pref INTEGER,
                metric INTEGER,
                communities TEXT NOT NULL
            )",
            NO_PARAMS,
        )
    }
}

impl DBTable for PeerSummary {
    fn create_table(conn: &Connection) -> Result<usize> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS peers (
                id INTEGER PRIMARY KEY,
                neighbor TEXT NOT NULL UNIQUE,
                router_id TEXT,
                asn BIGINT NOT NULL,
                msg_received BIGINT,
                msg_sent BIGINT,
                connect_time BIGINT,
                state TEXT NOT NULL
            )",
            NO_PARAMS,
        )
    }
}
