use crate::simulator::futures::Sleep;

// Re-export types for compatibility
pub use crate::simulator::futures::Interval;
pub use std::time::Duration;

#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

#[must_use]
pub fn interval(duration: Duration) -> crate::simulator::futures::Interval {
    crate::simulator::futures::Interval::new(duration)
}
