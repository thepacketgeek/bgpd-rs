use rusqlite::{Connection, Result};

mod conn;
mod tables;

pub use conn::DB;

pub trait DBTable {
    fn create_table(conn: &Connection) -> Result<usize>;
}
