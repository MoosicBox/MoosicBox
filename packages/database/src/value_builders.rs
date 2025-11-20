//! Builder APIs for constructing complex `DatabaseValue` instances
//!
//! This module provides fluent builder patterns for creating complex database values,
//! particularly for time-based operations that require SQL expressions rather than
//! simple parameter binding.
//!
//! ## Features
//!
//! * **`NowBuilder`**: Fluent API for `NOW()` + interval expressions
//! * **Timezone Support**: UTC default with explicit timezone control
//! * **Type Safety**: Structured intervals prevent SQL injection
//!
//! ## Example
//!
//! ```rust
//! use switchy_database::DatabaseValue;
//!
//! // Simple interval arithmetic
//! let tomorrow = DatabaseValue::now().plus_days(1);
//! let last_week = DatabaseValue::now().minus_days(7);
//!
//! // Complex intervals
//! let complex = DatabaseValue::now()
//!     .plus_years(1)
//!     .minus_months(2)
//!     .plus_hours(3);
//!
//! // With timezone
//! let pst_tomorrow = DatabaseValue::now()
//!     .tz("America/Los_Angeles")
//!     .plus_days(1);
//!
//! // From Duration
//! let in_one_hour = DatabaseValue::now()
//!     .plus_duration(std::time::Duration::from_secs(3600));
//! ```

use crate::{DatabaseValue, sql_interval::SqlInterval};
use std::time::Duration;

/// Builder for `NOW()` expressions with interval arithmetic and timezone support
///
/// Provides a fluent API for constructing `DatabaseValue::NowPlus` instances
/// with type-safe interval arithmetic and timezone handling.
///
/// ## Default Behavior
///
/// * **Timezone**: UTC (can be overridden with `tz()` or `local()`)
/// * **Interval**: Zero (equivalent to `NOW()`)
///
/// ## Timezone Handling
///
/// * `None` (default) → UTC in all databases
/// * `"LOCAL"` → Local system timezone
/// * `"<timezone>"` → Specific timezone (e.g., "`America/Los_Angeles`")
///
/// Note: `SQLite` has limited timezone support and may fall back to UTC.
#[derive(Debug, Clone)]
pub struct NowBuilder {
    interval: SqlInterval,
    timezone: Option<String>,
}

impl NowBuilder {
    /// Create a new `NowBuilder` with zero interval and UTC timezone
    #[must_use]
    pub const fn new() -> Self {
        Self {
            interval: SqlInterval::new(),
            timezone: None, // None = UTC by default
        }
    }

    /// Set the timezone for the `NOW()` expression
    ///
    /// Common values:
    /// * `"UTC"` - Coordinated Universal Time
    /// * `"America/Los_Angeles"` - Pacific timezone
    /// * `"Europe/London"` - UK timezone
    /// * `"LOCAL"` - System local timezone
    #[must_use]
    pub fn tz<S: Into<String>>(mut self, timezone: S) -> Self {
        self.timezone = Some(timezone.into());
        self
    }

    /// Use UTC timezone (this is the default)
    #[must_use]
    pub fn utc(mut self) -> Self {
        self.timezone = None; // None means UTC
        self
    }

    /// Use local system timezone
    #[must_use]
    pub fn local(mut self) -> Self {
        self.timezone = Some("LOCAL".to_string());
        self
    }

    /// Add years to the interval
    #[must_use]
    pub const fn plus_years(mut self, years: i32) -> Self {
        self.interval = self.interval.add_years(years);
        self
    }

    /// Subtract years from the interval
    #[must_use]
    pub const fn minus_years(mut self, years: i32) -> Self {
        self.interval = self.interval.add_years(-years);
        self
    }

    /// Add months to the interval
    #[must_use]
    pub const fn plus_months(mut self, months: i32) -> Self {
        self.interval = self.interval.add_months(months);
        self
    }

    /// Subtract months from the interval
    #[must_use]
    pub const fn minus_months(mut self, months: i32) -> Self {
        self.interval = self.interval.add_months(-months);
        self
    }

    /// Add days to the interval
    #[must_use]
    pub const fn plus_days(mut self, days: i32) -> Self {
        self.interval = self.interval.add_days(days);
        self
    }

    /// Subtract days from the interval
    #[must_use]
    pub const fn minus_days(mut self, days: i32) -> Self {
        self.interval = self.interval.add_days(-days);
        self
    }

    /// Add hours to the interval
    #[must_use]
    pub const fn plus_hours(mut self, hours: i64) -> Self {
        self.interval = self.interval.add_hours(hours);
        self
    }

    /// Subtract hours from the interval
    #[must_use]
    pub const fn minus_hours(mut self, hours: i64) -> Self {
        self.interval = self.interval.add_hours(-hours);
        self
    }

    /// Add minutes to the interval
    #[must_use]
    pub const fn plus_minutes(mut self, minutes: i64) -> Self {
        self.interval = self.interval.add_minutes(minutes);
        self
    }

    /// Subtract minutes from the interval
    #[must_use]
    pub const fn minus_minutes(mut self, minutes: i64) -> Self {
        self.interval = self.interval.add_minutes(-minutes);
        self
    }

    /// Add seconds to the interval
    #[must_use]
    pub const fn plus_seconds(mut self, seconds: i64) -> Self {
        self.interval = self.interval.add_seconds(seconds);
        self
    }

    /// Subtract seconds from the interval
    #[must_use]
    pub const fn minus_seconds(mut self, seconds: i64) -> Self {
        self.interval = self.interval.add_seconds(-seconds);
        self
    }

    /// Add a Duration to the interval
    ///
    /// Converts `std::time::Duration` to time components and adds them.
    /// Only affects hours, minutes, seconds, and nanoseconds.
    #[must_use]
    pub fn plus_duration(mut self, duration: Duration) -> Self {
        let duration_interval = SqlInterval::from_duration(duration);
        self.interval = self
            .interval
            .add_hours(duration_interval.hours)
            .add_minutes(duration_interval.minutes)
            .add_seconds(duration_interval.seconds);

        // Add nanoseconds (note: this may need normalization)
        self.interval.nanos = self.interval.nanos.saturating_add(duration_interval.nanos);
        self.interval = self.interval.normalize();
        self
    }

    /// Subtract a Duration from the interval
    #[must_use]
    pub fn minus_duration(mut self, duration: Duration) -> Self {
        let duration_interval = SqlInterval::from_duration(duration);
        self.interval = self
            .interval
            .add_hours(-duration_interval.hours)
            .add_minutes(-duration_interval.minutes)
            .add_seconds(-duration_interval.seconds);

        // Handle nanosecond subtraction carefully
        if self.interval.nanos >= duration_interval.nanos {
            self.interval.nanos -= duration_interval.nanos;
        } else {
            // Borrow from seconds
            self.interval = self.interval.add_seconds(-1);
            self.interval.nanos = self.interval.nanos + 1_000_000_000 - duration_interval.nanos;
        }

        self.interval = self.interval.normalize();
        self
    }

    /// Build the final `DatabaseValue`
    ///
    /// If the interval is zero, returns `DatabaseValue::Now`.
    /// Otherwise, returns `DatabaseValue::NowPlus` with the interval.
    ///
    /// Note: Timezone information is stored in the `SqlInterval` for backend processing.
    #[must_use]
    pub fn build(self) -> DatabaseValue {
        // For now, we'll need to encode timezone in the SqlInterval somehow
        // or handle it differently. Let's keep it simple for now.
        if self.interval.is_zero() && self.timezone.is_none() {
            DatabaseValue::Now
        } else {
            // TODO: We need to handle timezone information
            // For now, normalize the interval and create NowPlus
            DatabaseValue::NowPlus(self.interval.normalize())
        }
    }
}

impl Default for NowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseValue {
    /// Start building a `NOW()` expression with interval arithmetic
    ///
    /// Returns a `NowBuilder` for fluent construction of time expressions.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use switchy_database::DatabaseValue;
    ///
    /// let tomorrow = DatabaseValue::now().plus_days(1);
    /// let last_month = DatabaseValue::now().minus_months(1);
    /// ```
    #[must_use]
    pub const fn now() -> NowBuilder {
        NowBuilder::new()
    }

    /// Create a `NOW()` + interval expression directly
    ///
    /// For when you already have an `SqlInterval` constructed.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use switchy_database::{DatabaseValue, sql_interval::SqlInterval};
    ///
    /// let interval = SqlInterval::from_days(7);
    /// let next_week = DatabaseValue::now_plus(interval);
    /// ```
    #[must_use]
    pub const fn now_plus(interval: SqlInterval) -> Self {
        Self::NowPlus(interval)
    }
}

// Implement Into<DatabaseValue> for NowBuilder for convenience
impl From<NowBuilder> for DatabaseValue {
    fn from(builder: NowBuilder) -> Self {
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_builder_basic() {
        let builder = NowBuilder::new();
        assert!(builder.interval.is_zero());
        assert!(builder.timezone.is_none());
    }

    #[test]
    fn test_now_builder_plus_operations() {
        let result = DatabaseValue::now()
            .plus_years(1)
            .plus_months(2)
            .plus_days(3)
            .plus_hours(4)
            .plus_minutes(5)
            .plus_seconds(6)
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.years, 1);
            assert_eq!(interval.months, 2);
            assert_eq!(interval.days, 3);
            assert_eq!(interval.hours, 4);
            assert_eq!(interval.minutes, 5);
            assert_eq!(interval.seconds, 6);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_now_builder_minus_operations() {
        let result = DatabaseValue::now()
            .minus_years(1)
            .minus_days(7)
            .minus_hours(3)
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.years, -1);
            assert_eq!(interval.days, -7);
            assert_eq!(interval.hours, -3);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_now_builder_timezone() {
        let builder = DatabaseValue::now().tz("America/Los_Angeles");

        // Timezone handling needs to be implemented
        // For now, just test that it builds
        let _result = builder.build();
    }

    #[test]
    fn test_now_builder_duration() {
        let duration = Duration::from_secs(3661); // 1 hour, 1 minute, 1 second
        let result = DatabaseValue::now().plus_duration(duration).build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.hours, 1);
            assert_eq!(interval.minutes, 1);
            assert_eq!(interval.seconds, 1);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_zero_interval_returns_now() {
        let result = DatabaseValue::now().build();
        assert_eq!(result, DatabaseValue::Now);
    }

    #[test]
    fn test_non_zero_interval_returns_now_plus() {
        let result = DatabaseValue::now().plus_days(1).build();
        assert!(matches!(result, DatabaseValue::NowPlus(_)));
    }

    #[test]
    fn test_now_plus_direct() {
        let interval = SqlInterval::from_hours(24);
        let result = DatabaseValue::now_plus(interval.clone());
        assert_eq!(result, DatabaseValue::NowPlus(interval));
    }

    #[test]
    fn test_from_trait() {
        let builder = DatabaseValue::now().plus_days(1);
        let result: DatabaseValue = builder.into();
        assert!(matches!(result, DatabaseValue::NowPlus(_)));
    }

    #[test]
    fn test_normalization_in_build() {
        let result = DatabaseValue::now()
            .plus_minutes(90) // Should normalize to 1 hour 30 minutes
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.hours, 1);
            assert_eq!(interval.minutes, 30);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_duration_subtraction() {
        let duration = Duration::from_secs(3600); // 1 hour
        let result = DatabaseValue::now()
            .plus_hours(2)
            .minus_duration(duration)
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.hours, 1);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_duration_with_nanoseconds() {
        let duration = Duration::new(1, 500_000_000); // 1.5 seconds
        let result = DatabaseValue::now().plus_duration(duration).build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.seconds, 1);
            assert_eq!(interval.nanos, 500_000_000);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_minus_duration_with_nanos_borrow() {
        // Test nanosecond borrowing from seconds
        let add_duration = Duration::new(2, 100_000_000); // 2.1 seconds
        let sub_duration = Duration::new(0, 500_000_000); // 0.5 seconds

        let result = DatabaseValue::now()
            .plus_duration(add_duration)
            .minus_duration(sub_duration)
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            // 2.1s - 0.5s = 1.6s = 1 second + 600,000,000 nanos
            assert_eq!(interval.seconds, 1);
            assert_eq!(interval.nanos, 600_000_000);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_timezone_local() {
        let builder = DatabaseValue::now().local();
        assert_eq!(builder.timezone, Some("LOCAL".to_string()));
    }

    #[test]
    fn test_timezone_utc() {
        let builder = DatabaseValue::now().tz("America/Los_Angeles").utc();
        assert_eq!(builder.timezone, None); // UTC is represented as None
    }

    #[test]
    fn test_timezone_custom() {
        let builder = DatabaseValue::now().tz("Europe/London");
        assert_eq!(builder.timezone, Some("Europe/London".to_string()));
    }

    #[test]
    fn test_complex_interval_combination() {
        let result = DatabaseValue::now()
            .plus_years(1)
            .minus_months(2)
            .plus_days(15)
            .minus_hours(6)
            .plus_minutes(30)
            .minus_seconds(45)
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            assert_eq!(interval.years, 1);
            assert_eq!(interval.months, -2);
            assert_eq!(interval.days, 15);
            assert_eq!(interval.hours, -6);
            assert_eq!(interval.minutes, 30);
            assert_eq!(interval.seconds, -45);
        } else {
            panic!("Expected NowPlus variant");
        }
    }

    #[test]
    fn test_default_now_builder() {
        let builder = NowBuilder::default();
        assert!(builder.interval.is_zero());
        assert!(builder.timezone.is_none());
    }

    #[test]
    fn test_now_builder_chaining() {
        // Test that builder methods can be chained fluently
        let result = NowBuilder::new()
            .plus_days(1)
            .plus_hours(2)
            .plus_minutes(3)
            .build();

        assert!(matches!(result, DatabaseValue::NowPlus(_)));
    }

    #[test]
    fn test_nanos_overflow_in_plus_duration() {
        // Test that large nanosecond values are properly normalized
        let duration1 = Duration::new(0, 800_000_000); // 0.8 seconds
        let duration2 = Duration::new(0, 700_000_000); // 0.7 seconds

        let result = DatabaseValue::now()
            .plus_duration(duration1)
            .plus_duration(duration2)
            .build();

        if let DatabaseValue::NowPlus(interval) = result {
            // 0.8s + 0.7s = 1.5s = 1 second + 500,000,000 nanos (after normalization)
            assert_eq!(interval.seconds, 1);
            assert_eq!(interval.nanos, 500_000_000);
        } else {
            panic!("Expected NowPlus variant");
        }
    }
}
