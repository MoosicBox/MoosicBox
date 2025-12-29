//! Standard time functions using real system time.
//!
//! This module provides functions that directly call the standard library's time APIs,
//! returning actual system time without any simulation or mocking.

use std::time::{Instant, SystemTime};

/// Returns the current system time.
#[must_use]
pub fn now() -> SystemTime {
    SystemTime::now()
}

/// Returns the current monotonic instant.
#[must_use]
pub fn instant_now() -> Instant {
    Instant::now()
}

/// Returns the current local date and time.
#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_local_now() -> chrono::DateTime<chrono::Local> {
    chrono::Local::now()
}

/// Returns the current UTC date and time.
#[cfg(feature = "chrono")]
#[must_use]
pub fn datetime_utc_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}
