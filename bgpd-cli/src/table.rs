use std::convert::From;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::marker::PhantomData;
use std::net::IpAddr;
use std::time::Instant;

use chrono::{DateTime, TimeZone, Utc};
use prettytable::{cell, format, row, Row, Table};

use bgpd_lib::db::Route;
use bgpd_lib::peer::{Peer, PeerState};
use bgpd_lib::utils::{asn_to_dotted, format_elapsed_time, maybe_string, EMPTY_VALUE};

pub trait ToRow {
    fn columns() -> Row;
    fn to_row(&self) -> Row;
}

pub struct OutputTable<T: ToRow> {
    inner: Table,
    row_type: PhantomData<T>,
}

impl<T> OutputTable<T>
where
    T: ToRow,
{
    pub fn new() -> Self {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_CLEAN);
        table.add_row(T::columns());
        Self {
            inner: table,
            row_type: PhantomData,
        }
    }

    pub fn add_row(&mut self, row: &T) {
        self.inner.add_row(row.to_row());
    }

    pub fn write(&self, path: &str) {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        let mut buf = BufWriter::new(file);
        buf.write_all(format!("{}", self.inner).as_bytes()).unwrap();
    }
}
