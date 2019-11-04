use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::error;

use super::{Entry, Families};

pub struct SessionRoutes {
    pub families: Families,
    pub routes: HashMap<DateTime<Utc>, Arc<Entry>>,
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

    pub fn pending(&self) -> Vec<Arc<Entry>> {
        self.routes
            .iter()
            .filter(|(ts, _)| self.pending.contains(&ts))
            .filter(|(_, entry)| self.families.contains(&entry.update.family))
            .map(|(_, entry)| entry.clone())
            .collect()
    }
    pub fn advertised(&self) -> Vec<Arc<Entry>> {
        self.routes
            .iter()
            .filter(|(ts, _)| self.advertised.contains(&ts))
            .filter(|(_, entry)| self.families.contains(&entry.update.family))
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    pub fn insert_routes(&mut self, entries: Vec<Arc<Entry>>) {
        for entry in entries.into_iter() {
            let ts = entry.timestamp;
            // If this entry is not present, add to pending routes
            if let None = self.routes.insert(ts, entry) {
                self.pending.insert(ts);
            }
        }
    }

    pub fn mark_advertised(&mut self, entry: &Arc<Entry>) {
        let ts = entry.timestamp;
        if !self.pending.remove(&ts) {
            error!("No route to remove: {}", ts);
        }
        self.advertised.insert(ts);
    }
}
