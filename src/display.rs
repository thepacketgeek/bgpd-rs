use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::net::IpAddr;
use std::time::Instant;

use prettytable::{cell, format, row, Row, Table};

use crate::peer::PeerState;
use crate::utils::{asn_to_dotted, format_elapsed_time};

pub struct StatusRow {
    pub neighbor: IpAddr,
    pub asn: u32,
    pub msg_received: u64,
    pub msg_sent: u64,
    pub connect_time: Option<Instant>,
    pub state: PeerState,
    pub prefixes_received: u64,
}

impl StatusRow {
    pub fn columns() -> Row {
        row!["Neighbor", "AS", "MsgRcvd", "MsgSent", "Uptime", "State", "PfxRcd"]
    }

    pub fn to_row(&self) -> Row {
        row![
            self.neighbor.to_string(),
            asn_to_dotted(self.asn),
            self.msg_received.to_string(),
            self.msg_sent.to_string(),
            if let Some(connect_time) = self.connect_time {
                format_elapsed_time(connect_time.elapsed())
            } else {
                String::from("---")
            },
            self.state.to_string(),
            self.prefixes_received.to_string(),
        ]
    }
}

pub struct StatusTable {
    inner: Table,
}

impl StatusTable {
    pub fn new() -> Self {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_CLEAN);
        table.add_row(StatusRow::columns());
        StatusTable { inner: table }
    }

    pub fn add_row(&mut self, row: &StatusRow) {
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
