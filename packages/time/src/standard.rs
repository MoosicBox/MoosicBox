use std::time::{Instant, SystemTime};

#[must_use]
pub fn now() -> SystemTime {
    SystemTime::now()
}

#[must_use]
pub fn instant_now() -> Instant {
    Instant::now()
}

#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_local_now() -> chrono::DateTime<chrono::Local> {
    chrono::Local::now()
}

#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_utc_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}
