use std::time::Duration;

use crate::simulator::futures::Sleep;

#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}
