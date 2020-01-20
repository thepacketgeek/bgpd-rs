use std::fmt;
use std::time;

use chrono::{DateTime, Duration, Utc};
use tokio::time::{interval, Interval};

use super::SessionError;
use crate::utils::{format_elapsed_time, get_elapsed_time};

#[derive(Debug)]
pub struct HoldTimer {
    pub(crate) hold_timer: u16,
    pub(crate) interval: u16,
    timer: Interval,
    pub(crate) last_sent: DateTime<Utc>,
    pub(crate) last_received: DateTime<Utc>,
}

impl HoldTimer {
    pub fn new(hold_timer: u16) -> HoldTimer {
        HoldTimer {
            hold_timer,
            interval: hold_timer / 3,
            timer: interval(time::Duration::from_millis(100)),
            last_received: Utc::now(),
            last_sent: Utc::now(),
        }
    }

    // Calculate if Keepalive message should be sent
    // Returns true when:
    //    Hold time remaining is less than 2/3 of the total hold_timer
    //    which is 2x the Keepalive timer
    pub async fn should_send_keepalive(&mut self) -> Result<bool, SessionError> {
        self.timer.tick().await;
        if self.is_expired() {
            return Err(SessionError::HoldTimeExpired(self.interval));
        }
        Ok(self.get_hold_time().num_seconds() < (2 * i64::from(self.interval)))
    }

    /// Bump the last received to now
    pub fn received(&mut self) {
        self.last_received = Utc::now();
    }
    /// Bump the last sent to now
    pub fn sent(&mut self) {
        self.last_sent = Utc::now();
    }

    // Calculate remaining hold time available
    // Counts down from self.hold_timer to 0
    // Will never be less than 0, at which the peer hold time has expired
    fn get_hold_time(&self) -> Duration {
        let hold_time = Duration::seconds(self.hold_timer.into());
        if get_elapsed_time(self.last_sent) > hold_time {
            Duration::seconds(0)
        } else {
            hold_time - get_elapsed_time(self.last_sent)
        }
    }

    fn is_expired(&self) -> bool {
        let hold_time = Duration::seconds(self.hold_timer.into());
        get_elapsed_time(self.last_received) >= hold_time
    }
}

impl fmt::Display for HoldTimer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_elapsed_time(self.get_hold_time()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_interval() {
        let mut ht = HoldTimer::new(30);
        assert_eq!(ht.interval, 10);
        assert!(!ht.is_expired());
        // Test that keepalive should not be sent yet
        ht.last_sent = ht.last_sent - Duration::seconds(5);
        ht.timer = interval(time::Duration::from_millis(1));
        assert!(!ht.should_send_keepalive().await.unwrap());
        // After waiting 1/3 of hold_time, we should send keepalive
        ht.last_sent = ht.last_sent - Duration::seconds(5);
        ht.timer = interval(time::Duration::from_millis(1));
        assert!(ht.should_send_keepalive().await.unwrap());

        ht.sent();
        ht.timer = interval(time::Duration::from_millis(1));
        assert!(!ht.should_send_keepalive().await.unwrap());

        // And if hold_time is past, this session is expired
        ht.last_received = ht.last_received - Duration::seconds(30);
        ht.timer = interval(time::Duration::from_millis(1));
        match ht.should_send_keepalive().await {
            Ok(_) => panic!("Should return Err"),
            _ => (),
        }
    }
}
