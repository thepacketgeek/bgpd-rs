use std::fmt;
use std::io::Error;

use chrono::{DateTime, Duration, Utc};
use futures::{Async, Poll};
use tokio::prelude::*;

use crate::utils::{format_elapsed_time, get_elapsed_time};

pub struct HoldTimer {
    hold_timer: u16,
    last_update: DateTime<Utc>,
}

impl HoldTimer {
    pub fn new(hold_timer: u16) -> HoldTimer {
        HoldTimer {
            hold_timer,
            last_update: Utc::now(),
        }
    }

    // Calculate the interval for sending keepalives
    fn get_keepalive_timer(&self) -> Duration {
        Duration::seconds((self.hold_timer / 3).into())
    }

    // Calculate remaining hold time available
    // Counts down from self.hold_timer to 0
    // Will never be less than 0, at which the peer hold time has expired
    pub fn get_hold_time(&self) -> Duration {
        let hold_time = Duration::seconds(self.hold_timer.into());
        if get_elapsed_time(self.last_update) > hold_time {
            Duration::seconds(0)
        } else {
            hold_time - get_elapsed_time(self.last_update)
        }
    }

    // Calculate if Keepalive message should be sent
    // Returns true when:
    //    Hold time remaining is less than 2/3 of the total hold_timer
    //    which is 2x the Keepalive timer
    pub fn should_send_keepalive(&self) -> bool {
        self.get_hold_time().num_seconds() < (2 * self.get_keepalive_timer().num_seconds())
    }

    pub fn received_update(&mut self) {
        self.last_update = Utc::now();
    }
}

impl Future for HoldTimer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Error> {
        if self.should_send_keepalive() {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl fmt::Display for HoldTimer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_elapsed_time(self.get_hold_time()))
    }
}
