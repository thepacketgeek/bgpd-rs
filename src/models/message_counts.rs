#[derive(Debug, Default)]
pub struct MessageCounts {
    received: u64,
    sent: u64,
}

impl MessageCounts {
    pub fn new() -> Self {
        MessageCounts::default()
    }

    pub fn received(&self) -> u64 {
        self.received
    }
    pub fn increment_received(&mut self) {
        self.received += 1;
    }

    pub fn sent(&self) -> u64 {
        self.sent
    }
    pub fn increment_sent(&mut self) {
        self.sent += 1;
    }
}
