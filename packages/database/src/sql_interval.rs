//! SQL interval types for type-safe time arithmetic
//!
//! This module provides the [`SqlInterval`] type for representing SQL intervals
//! in a type-safe manner, preventing SQL injection attacks that were possible
//! with the previous string-based approach.
//!
//! ## Features
//!
//! * **Type Safety**: No string-based interval construction
//! * **Cross-Database**: Generates appropriate SQL for `PostgreSQL`, `MySQL`, `SQLite`
//! * **Normalization**: Automatically normalizes time components (e.g., 90 minutes → 1 hour 30 minutes)
//! * **Builder Pattern**: Fluent API for constructing complex intervals
//! * **Duration Support**: Conversion from `std::time::Duration`
//!
//! ## Example
//!
//! ```rust
//! use switchy_database::sql_interval::SqlInterval;
//! use std::time::Duration;
//!
//! // Builder pattern
//! let interval = SqlInterval::new()
//!     .years(1)
//!     .months(3)
//!     .days(-7)
//!     .hours(2);
//!
//! // From Duration
//! let duration_interval = SqlInterval::from_duration(Duration::from_secs(3600));
//!
//! // Normalization
//! let normalized = SqlInterval::new()
//!     .minutes(90)  // Will be normalized to 1 hour 30 minutes
//!     .normalize();
//! ```

use std::time::Duration;

/// Represents a SQL interval with calendar and time components
///
/// This type provides a safe, structured way to represent time intervals
/// for SQL operations, replacing the vulnerable string-based approach.
///
/// ## Components
///
/// * `years`, `months` - Calendar components (cannot be converted to exact duration)
/// * `days` - Calendar days (not 24-hour periods due to DST)
/// * `hours`, `minutes`, `seconds` - Time components
/// * `nanos` - Nanosecond precision (0-999,999,999)
///
/// ## Normalization
///
/// The interval automatically normalizes overflow in time components:
/// * 90 minutes → 1 hour 30 minutes
/// * 25 hours → 1 day 1 hour
/// * -30 minutes → -30 minutes (preserved as-is)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SqlInterval {
    /// Number of years (can be negative for past intervals)
    pub years: i32,
    /// Number of months (can be negative for past intervals)
    pub months: i32,
    /// Number of days (can be negative for past intervals)
    pub days: i32,
    /// Number of hours (can be negative for past intervals)
    pub hours: i64,
    /// Number of minutes (can be negative for past intervals)
    pub minutes: i64,
    /// Number of seconds (can be negative for past intervals)
    pub seconds: i64,
    /// Number of nanoseconds (always 0-999,999,999)
    pub nanos: u32,
}

impl SqlInterval {
    /// Create a new empty interval (all components zero)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            years: 0,
            months: 0,
            days: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
            nanos: 0,
        }
    }

    /// Set the years component
    #[must_use]
    pub const fn years(mut self, years: i32) -> Self {
        self.years = years;
        self
    }

    /// Set the months component
    #[must_use]
    pub const fn months(mut self, months: i32) -> Self {
        self.months = months;
        self
    }

    /// Set the days component
    #[must_use]
    pub const fn days(mut self, days: i32) -> Self {
        self.days = days;
        self
    }

    /// Set the hours component
    #[must_use]
    pub const fn hours(mut self, hours: i64) -> Self {
        self.hours = hours;
        self
    }

    /// Set the minutes component
    #[must_use]
    pub const fn minutes(mut self, minutes: i64) -> Self {
        self.minutes = minutes;
        self
    }

    /// Set the seconds component
    #[must_use]
    pub const fn seconds(mut self, seconds: i64) -> Self {
        self.seconds = seconds;
        self
    }

    /// Set the nanoseconds component
    #[must_use]
    pub const fn nanos(mut self, nanos: u32) -> Self {
        self.nanos = nanos;
        self
    }

    /// Add years to the current interval
    #[must_use]
    pub const fn add_years(mut self, years: i32) -> Self {
        self.years = self.years.saturating_add(years);
        self
    }

    /// Add months to the current interval
    #[must_use]
    pub const fn add_months(mut self, months: i32) -> Self {
        self.months = self.months.saturating_add(months);
        self
    }

    /// Add days to the current interval
    #[must_use]
    pub const fn add_days(mut self, days: i32) -> Self {
        self.days = self.days.saturating_add(days);
        self
    }

    /// Add hours to the current interval
    #[must_use]
    pub const fn add_hours(mut self, hours: i64) -> Self {
        self.hours = self.hours.saturating_add(hours);
        self
    }

    /// Add minutes to the current interval
    #[must_use]
    pub const fn add_minutes(mut self, minutes: i64) -> Self {
        self.minutes = self.minutes.saturating_add(minutes);
        self
    }

    /// Add seconds to the current interval
    #[must_use]
    pub const fn add_seconds(mut self, seconds: i64) -> Self {
        self.seconds = self.seconds.saturating_add(seconds);
        self
    }

    /// Create interval from years only
    #[must_use]
    pub const fn from_years(years: i32) -> Self {
        Self::new().years(years)
    }

    /// Create interval from months only
    #[must_use]
    pub const fn from_months(months: i32) -> Self {
        Self::new().months(months)
    }

    /// Create interval from days only
    #[must_use]
    pub const fn from_days(days: i32) -> Self {
        Self::new().days(days)
    }

    /// Create interval from hours only
    #[must_use]
    pub const fn from_hours(hours: i64) -> Self {
        Self::new().hours(hours)
    }

    /// Create interval from minutes only
    #[must_use]
    pub const fn from_minutes(minutes: i64) -> Self {
        Self::new().minutes(minutes)
    }

    /// Create interval from seconds only
    #[must_use]
    pub const fn from_seconds(seconds: i64) -> Self {
        Self::new().seconds(seconds)
    }

    /// Create interval from Duration
    ///
    /// Converts a `std::time::Duration` to an `SqlInterval`.
    /// Only handles time components (hours, minutes, seconds, nanos).
    #[must_use]
    pub const fn from_duration(duration: Duration) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let total_seconds = duration.as_secs() as i64;
        let nanos = duration.subsec_nanos();

        let hours = total_seconds / 3600;
        let remaining_seconds = total_seconds % 3600;
        let minutes = remaining_seconds / 60;
        let seconds = remaining_seconds % 60;

        Self {
            years: 0,
            months: 0,
            days: 0,
            hours,
            minutes,
            seconds,
            nanos,
        }
    }

    /// Normalize time components by carrying over excess values
    ///
    /// * Nanoseconds → Seconds (if >= 1,000,000,000)
    /// * Seconds → Minutes (if >= 60 or <= -60)
    /// * Minutes → Hours (if >= 60 or <= -60)
    /// * Hours → Days (if >= 24 or <= -24)
    ///
    /// Calendar components (years, months) are not normalized as they
    /// don't have fixed relationships.
    #[must_use]
    pub fn normalize(mut self) -> Self {
        // Normalize nanoseconds to seconds
        if self.nanos >= 1_000_000_000 {
            let extra_seconds = self.nanos / 1_000_000_000;
            self.seconds = self.seconds.saturating_add(i64::from(extra_seconds));
            self.nanos %= 1_000_000_000;
        }

        // Normalize seconds to minutes
        if self.seconds.abs() >= 60 {
            let extra_minutes = self.seconds / 60;
            self.minutes = self.minutes.saturating_add(extra_minutes);
            self.seconds %= 60;
        }

        // Normalize minutes to hours
        if self.minutes.abs() >= 60 {
            let extra_hours = self.minutes / 60;
            self.hours = self.hours.saturating_add(extra_hours);
            self.minutes %= 60;
        }

        // Normalize hours to days
        if self.hours.abs() >= 24 {
            let extra_days = self.hours / 24;
            // Saturating conversion from i64 to i32 for days
            self.days = self
                .days
                .saturating_add(extra_days.try_into().unwrap_or(i32::MAX));
            self.hours %= 24;
        }

        self
    }

    /// Check if the interval represents going backwards in time
    ///
    /// Returns true if any significant component is negative.
    /// Note: Nanoseconds are always positive (0-999,999,999).
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.years < 0
            || self.months < 0
            || self.days < 0
            || self.hours < 0
            || self.minutes < 0
            || self.seconds < 0
    }

    /// Get the absolute value of the interval
    ///
    /// Returns a new interval where all components are positive.
    #[must_use]
    pub const fn abs(mut self) -> Self {
        self.years = self.years.abs();
        self.months = self.months.abs();
        self.days = self.days.abs();
        self.hours = self.hours.abs();
        self.minutes = self.minutes.abs();
        self.seconds = self.seconds.abs();
        // nanos is already always positive
        self
    }

    /// Check if the interval is zero (all components are zero)
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.years == 0
            && self.months == 0
            && self.days == 0
            && self.hours == 0
            && self.minutes == 0
            && self.seconds == 0
            && self.nanos == 0
    }
}

impl From<Duration> for SqlInterval {
    fn from(duration: Duration) -> Self {
        Self::from_duration(duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_interval_is_zero() {
        let interval = SqlInterval::new();
        assert!(interval.is_zero());
        assert!(!interval.is_negative());
        assert_eq!(interval, SqlInterval::default());
    }

    #[test]
    fn test_builder_methods() {
        let interval = SqlInterval::new()
            .years(2)
            .months(3)
            .days(15)
            .hours(12)
            .minutes(30)
            .seconds(45)
            .nanos(123_456_789);

        assert_eq!(interval.years, 2);
        assert_eq!(interval.months, 3);
        assert_eq!(interval.days, 15);
        assert_eq!(interval.hours, 12);
        assert_eq!(interval.minutes, 30);
        assert_eq!(interval.seconds, 45);
        assert_eq!(interval.nanos, 123_456_789);
    }

    #[test]
    fn test_add_methods() {
        let interval = SqlInterval::new()
            .add_years(1)
            .add_months(6)
            .add_days(10)
            .add_hours(5)
            .add_minutes(20)
            .add_seconds(30);

        assert_eq!(interval.years, 1);
        assert_eq!(interval.months, 6);
        assert_eq!(interval.days, 10);
        assert_eq!(interval.hours, 5);
        assert_eq!(interval.minutes, 20);
        assert_eq!(interval.seconds, 30);
    }

    #[test]
    fn test_from_constructors() {
        assert_eq!(SqlInterval::from_years(3), SqlInterval::new().years(3));
        assert_eq!(SqlInterval::from_months(8), SqlInterval::new().months(8));
        assert_eq!(SqlInterval::from_days(7), SqlInterval::new().days(7));
        assert_eq!(SqlInterval::from_hours(24), SqlInterval::new().hours(24));
        assert_eq!(
            SqlInterval::from_minutes(90),
            SqlInterval::new().minutes(90)
        );
        assert_eq!(
            SqlInterval::from_seconds(3600),
            SqlInterval::new().seconds(3600)
        );
    }

    #[test]
    fn test_from_duration() {
        let duration = Duration::from_secs(3661); // 1 hour, 1 minute, 1 second
        let interval = SqlInterval::from_duration(duration);

        assert_eq!(interval.years, 0);
        assert_eq!(interval.months, 0);
        assert_eq!(interval.days, 0);
        assert_eq!(interval.hours, 1);
        assert_eq!(interval.minutes, 1);
        assert_eq!(interval.seconds, 1);
        assert_eq!(interval.nanos, 0);
    }

    #[test]
    fn test_from_duration_with_nanos() {
        let duration = Duration::new(0, 500_000_000); // 0.5 seconds
        let interval = SqlInterval::from_duration(duration);

        assert_eq!(interval.seconds, 0);
        assert_eq!(interval.nanos, 500_000_000);
    }

    #[test]
    fn test_normalize_seconds_to_minutes() {
        let interval = SqlInterval::new()
            .seconds(150) // 2 minutes 30 seconds
            .normalize();

        assert_eq!(interval.minutes, 2);
        assert_eq!(interval.seconds, 30);
    }

    #[test]
    fn test_normalize_minutes_to_hours() {
        let interval = SqlInterval::new()
            .minutes(90) // 1 hour 30 minutes
            .normalize();

        assert_eq!(interval.hours, 1);
        assert_eq!(interval.minutes, 30);
    }

    #[test]
    fn test_normalize_hours_to_days() {
        let interval = SqlInterval::new()
            .hours(25) // 1 day 1 hour
            .normalize();

        assert_eq!(interval.days, 1);
        assert_eq!(interval.hours, 1);
    }

    #[test]
    fn test_normalize_complex() {
        let interval = SqlInterval::new()
            .minutes(150) // 2 hours 30 minutes
            .seconds(3661) // 1 hour 1 minute 1 second
            .nanos(2_000_000_000) // 2 seconds
            .normalize();

        // Expected: 3 hours 31 minutes 3 seconds
        assert_eq!(interval.hours, 3);
        assert_eq!(interval.minutes, 31);
        assert_eq!(interval.seconds, 3);
        assert_eq!(interval.nanos, 0);
    }

    #[test]
    fn test_normalize_negative() {
        let interval = SqlInterval::new()
            .minutes(-90) // -1 hour -30 minutes
            .normalize();

        assert_eq!(interval.hours, -1);
        assert_eq!(interval.minutes, -30);
    }

    #[test]
    fn test_is_negative() {
        assert!(!SqlInterval::new().is_negative());
        assert!(!SqlInterval::new().days(5).is_negative());
        assert!(SqlInterval::new().days(-1).is_negative());
        assert!(SqlInterval::new().years(-1).is_negative());
        assert!(SqlInterval::new().hours(-1).is_negative());
    }

    #[test]
    fn test_abs() {
        let interval = SqlInterval::new().years(-2).days(-5).hours(-3).abs();

        assert_eq!(interval.years, 2);
        assert_eq!(interval.days, 5);
        assert_eq!(interval.hours, 3);
        assert!(!interval.is_negative());
    }

    #[test]
    fn test_is_zero() {
        assert!(SqlInterval::new().is_zero());
        assert!(!SqlInterval::new().days(1).is_zero());
        assert!(!SqlInterval::new().nanos(1).is_zero());
    }

    #[test]
    fn test_from_trait() {
        let duration = Duration::from_secs(3600);
        let interval: SqlInterval = duration.into();
        assert_eq!(interval.hours, 1);
        assert_eq!(interval.minutes, 0);
        assert_eq!(interval.seconds, 0);
    }

    #[test]
    fn test_saturating_add_overflow() {
        let interval = SqlInterval::new().years(i32::MAX).add_years(1); // Should saturate, not panic

        assert_eq!(interval.years, i32::MAX);
    }
}
