use std::convert::From;
use std::fmt;
use std::sync::Arc;

use bgp_rs::NLRIEncoding;
use chrono::{DateTime, Utc};

use super::{EntrySource, Family, PathAttributes, RibEntry};
use crate::utils::format_time_as_elapsed;

/// External representation of RIB info
/// Used outside of RIB (API exports, Session RIB)
#[derive(Debug)]
pub struct ExportEntry {
    // Time received
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) update: ExportedUpdate,
    pub(crate) source: EntrySource,
}

impl ExportEntry {
    pub fn new(update: ExportedUpdate, source: EntrySource) -> Self {
        Self {
            timestamp: Utc::now(),
            update,
            source,
        }
    }
}

impl From<(&RibEntry, Arc<PathAttributes>)> for ExportEntry {
    fn from(route: (&RibEntry, Arc<PathAttributes>)) -> Self {
        let (entry, attributes) = route;
        ExportEntry {
            timestamp: entry.timestamp,
            source: entry.source,
            update: ExportedUpdate {
                family: entry.family,
                attributes,
                nlri: entry.nlri.clone(),
            },
        }
    }
}

impl fmt::Display for ExportEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<ExportEntry source={} age={}>",
            self.source,
            format_time_as_elapsed(self.timestamp),
        )
    }
}

/// External representation of Update Info
#[derive(Debug)]
pub struct ExportedUpdate {
    pub family: Family,
    pub attributes: Arc<PathAttributes>,
    pub nlri: NLRIEncoding,
}
