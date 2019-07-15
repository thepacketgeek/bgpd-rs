use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::marker::PhantomData;
use std::net::IpAddr;
use std::time::Instant;

use prettytable::{cell, format, row, Row, Table};

use crate::peer::PeerState;
use crate::utils::{asn_to_dotted, format_elapsed_time, maybe_string};

pub trait ToRow {
    fn columns() -> Row;
    fn to_row(&self) -> Row;
}

pub struct StatusRow {
    pub neighbor: IpAddr,
    pub asn: u32,
    pub msg_received: Option<u64>,
    pub msg_sent: Option<u64>,
    pub connect_time: Option<Instant>,
    pub state: PeerState,
    pub prefixes_received: Option<u64>,
}

impl ToRow for StatusRow {
    fn columns() -> Row {
        row!["Neighbor", "AS", "MsgRcvd", "MsgSent", "Uptime", "State", "PfxRcd"]
    }

    fn to_row(&self) -> Row {
        row![
            self.neighbor.to_string(),
            asn_to_dotted(self.asn),
            maybe_string(self.msg_received.as_ref()),
            maybe_string(self.msg_sent.as_ref()),
            if let Some(connect_time) = self.connect_time {
                format_elapsed_time(connect_time.elapsed())
            } else {
                String::from("---")
            },
            self.state.to_string(),
            maybe_string(self.prefixes_received.as_ref()),
        ]
    }
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
