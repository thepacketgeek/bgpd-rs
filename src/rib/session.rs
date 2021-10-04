use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::error;

use super::{ExportEntry, Families};

pub struct SessionRoutes {
    pub families: Families,
    pub routes: HashMap<DateTime<Utc>, Arc<ExportEntry>>,
    pending: HashSet<DateTime<Utc>>,
    advertised: HashSet<DateTime<Utc>>,
}

impl SessionRoutes {
    pub fn new(families: Families) -> Self {
        Self {
            families,
            routes: HashMap::new(),
            pending: HashSet::new(),
            advertised: HashSet::new(),
        }
    }

    pub fn pending(&self) -> Vec<Arc<ExportEntry>> {
        self.routes
            .iter()
            .filter(|(ts, _)| self.pending.contains(ts))
            .filter(|(_, entry)| self.families.contains(entry.update.family))
            .map(|(_, entry)| entry.clone())
            .collect()
    }
    pub fn advertised(&self) -> Vec<Arc<ExportEntry>> {
        self.routes
            .iter()
            .filter(|(ts, _)| self.advertised.contains(ts))
            .filter(|(_, entry)| self.families.contains(entry.update.family))
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    pub fn insert_routes(&mut self, entries: Vec<Arc<ExportEntry>>) {
        for entry in entries.into_iter() {
            let ts = entry.timestamp;
            // If this entry is not present, add to pending routes
            if self.routes.insert(ts, entry).is_none() {
                self.pending.insert(ts);
            }
        }
    }

    pub fn mark_advertised(&mut self, entry: &Arc<ExportEntry>) {
        let ts = entry.timestamp;
        if !self.pending.remove(&ts) {
            error!("No route to remove: {}", ts);
        }
        self.advertised.insert(ts);
    }
}
