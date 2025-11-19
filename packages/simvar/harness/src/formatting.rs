//! Time formatting utilities.
//!
//! This module provides the [`TimeFormat`] trait for converting time durations
//! in milliseconds into human-readable formatted strings.

/// Formats time durations in milliseconds into human-readable strings.
///
/// This trait is implemented for various unsigned integer types and provides
/// a consistent way to format time values for display.
pub trait TimeFormat {
    /// Converts the time value into a formatted human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use simvar_harness::formatting::TimeFormat;
    /// let formatted = 5000u64.into_formatted();
    /// assert_eq!(formatted, "5s, 0ms");
    /// ```
    #[must_use]
    fn into_formatted(self) -> String;
}

/// Implements time formatting for `u32` values.
impl TimeFormat for u32 {
    fn into_formatted(self) -> String {
        u128::from(self).into_formatted()
    }
}

/// Implements time formatting for `u64` values.
impl TimeFormat for u64 {
    fn into_formatted(self) -> String {
        u128::from(self).into_formatted()
    }
}

/// Implements time formatting for `u128` values.
///
/// Formats the time value in milliseconds into a human-readable string with
/// appropriate units (years, days, hours, minutes, seconds, milliseconds).
impl TimeFormat for u128 {
    fn into_formatted(self) -> String {
        #[must_use]
        const fn plural(num: u128) -> &'static str {
            if num == 1 { "" } else { "s" }
        }

        let years = self / 365 / 24 / 60 / 60 / 1000;
        let days = self / 24 / 60 / 60 / 1000 % 365;
        let hours = self / 60 / 60 / 1000 % 24;
        let minutes = self / 60 / 1000 % 60;
        let seconds = self / 1000 % 60;
        let ms = self % 1000;

        if years > 0 {
            format!(
                "{years} year{}, {days} day{}, {hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(years),
                plural(days),
                plural(hours),
                plural(minutes),
            )
        } else if days > 0 {
            format!(
                "{days} day{}, {hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(days),
                plural(hours),
                plural(minutes),
            )
        } else if hours > 0 {
            format!(
                "{hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(hours),
                plural(minutes),
            )
        } else if minutes > 0 {
            format!("{minutes} minute{}, {seconds}s, {ms}ms", plural(minutes))
        } else if seconds > 0 {
            format!("{seconds}s, {ms}ms")
        } else {
            format!("{ms}ms")
        }
    }
}
