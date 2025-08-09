use std::time::{Instant, SystemTime};

#[must_use]
pub fn now() -> SystemTime {
    SystemTime::now()
}

#[must_use]
pub fn instant_now() -> Instant {
    Instant::now()
}
