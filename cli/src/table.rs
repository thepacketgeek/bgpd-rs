use std::marker::PhantomData;

use prettytable::{format, Row, Table};

pub trait ToRow {
    fn columns() -> Row;
    fn to_row(&self) -> Result<Row, String>;
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
        let format = format::FormatBuilder::new()
            .padding(1, 1)
            .separator(
                format::LinePosition::Title,
                format::LineSeparator::new('-', '+', '+', '+'),
            )
            .build();
        Self::with_format(format)
    }

    pub fn with_format(format: format::TableFormat) -> Self {
        let mut table = Table::new();
        table.set_format(format);
        table.add_row(T::columns());
        Self {
            inner: table,
            row_type: PhantomData,
        }
    }

    pub fn add_row(&mut self, row: &T) -> Result<(), String> {
        row.to_row()
            .map(|row| self.inner.add_row(row))
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn print(&self) {
        self.inner.printstd();
    }
}
