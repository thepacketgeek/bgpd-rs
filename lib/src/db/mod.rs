use rusqlite::{Connection, Result};

mod conn;
mod peer_status;
mod route;

pub use conn::DB;
pub use peer_status::PeerStatus;
pub use route::{Community, CommunityList, Route};

pub trait DBTable {
    fn create_table(conn: &Connection) -> Result<usize>;
}
