use std::future::IntoFuture;

use crate::simulator::futures::{Sleep, Timeout};

// Re-export types for compatibility
pub use crate::simulator::futures::{Elapsed, Interval};
pub use std::time::Duration;

// Re-export simulator functionality
pub mod simulator {
    pub use switchy_time::simulator::with_real_time;
}

#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

#[must_use]
pub fn interval(duration: Duration) -> crate::simulator::futures::Interval {
    crate::simulator::futures::Interval::new(duration)
}

#[must_use]
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F::IntoFuture>
where
    F: IntoFuture,
{
    Timeout::new(duration, future.into_future())
}
