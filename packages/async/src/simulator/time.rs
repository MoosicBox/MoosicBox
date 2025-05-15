use std::time::Duration;

use crate::simulator::futures::{Interval, Sleep};

#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

#[must_use]
pub fn interval(duration: Duration) -> Interval {
    Interval::new(duration)
}
