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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_milliseconds_only() {
        assert_eq!(0u128.into_formatted(), "0ms");
        assert_eq!(1u128.into_formatted(), "1ms");
        assert_eq!(999u128.into_formatted(), "999ms");
    }

    #[test]
    fn test_format_seconds() {
        assert_eq!(1000u128.into_formatted(), "1s, 0ms");
        assert_eq!(5000u128.into_formatted(), "5s, 0ms");
        assert_eq!(5432u128.into_formatted(), "5s, 432ms");
        assert_eq!(59_999u128.into_formatted(), "59s, 999ms");
    }

    #[test]
    fn test_format_minutes() {
        assert_eq!(60_000u128.into_formatted(), "1 minute, 0s, 0ms");
        assert_eq!(120_000u128.into_formatted(), "2 minutes, 0s, 0ms");
        assert_eq!(65_432u128.into_formatted(), "1 minute, 5s, 432ms");
        assert_eq!(125_432u128.into_formatted(), "2 minutes, 5s, 432ms");
    }

    #[test]
    fn test_format_hours() {
        assert_eq!(3_600_000u128.into_formatted(), "1 hour, 0 minutes, 0s, 0ms");
        assert_eq!(
            7_200_000u128.into_formatted(),
            "2 hours, 0 minutes, 0s, 0ms"
        );
        assert_eq!(
            3_665_432u128.into_formatted(),
            "1 hour, 1 minute, 5s, 432ms"
        );
        assert_eq!(
            7_325_432u128.into_formatted(),
            "2 hours, 2 minutes, 5s, 432ms"
        );
    }

    #[test]
    fn test_format_days() {
        assert_eq!(
            86_400_000u128.into_formatted(),
            "1 day, 0 hours, 0 minutes, 0s, 0ms"
        );
        assert_eq!(
            172_800_000u128.into_formatted(),
            "2 days, 0 hours, 0 minutes, 0s, 0ms"
        );
        assert_eq!(
            90_065_432u128.into_formatted(),
            "1 day, 1 hour, 1 minute, 5s, 432ms"
        );
    }

    #[test]
    fn test_format_years() {
        // 1 year = 365 days
        let one_year = 365u128 * 24 * 60 * 60 * 1000;
        assert_eq!(
            one_year.into_formatted(),
            "1 year, 0 days, 0 hours, 0 minutes, 0s, 0ms"
        );

        let two_years = 2 * one_year;
        assert_eq!(
            two_years.into_formatted(),
            "2 years, 0 days, 0 hours, 0 minutes, 0s, 0ms"
        );

        // 1 year, 1 day, 1 hour, 1 minute, 1 second, 1ms
        let complex_time = one_year + 86_400_000 + 3_600_000 + 60_000 + 1_000 + 1;
        assert_eq!(
            complex_time.into_formatted(),
            "1 year, 1 day, 1 hour, 1 minute, 1s, 1ms"
        );
    }

    #[test]
    fn test_format_u32() {
        assert_eq!(1000u32.into_formatted(), "1s, 0ms");
        assert_eq!(5432u32.into_formatted(), "5s, 432ms");
    }

    #[test]
    fn test_format_u64() {
        assert_eq!(1000u64.into_formatted(), "1s, 0ms");
        assert_eq!(5432u64.into_formatted(), "5s, 432ms");
    }

    #[test]
    fn test_format_singular_plural() {
        // Test singular forms
        assert_eq!(60_000u128.into_formatted(), "1 minute, 0s, 0ms");
        assert_eq!(3_600_000u128.into_formatted(), "1 hour, 0 minutes, 0s, 0ms");
        assert_eq!(
            86_400_000u128.into_formatted(),
            "1 day, 0 hours, 0 minutes, 0s, 0ms"
        );

        // Test plural forms
        assert_eq!(120_000u128.into_formatted(), "2 minutes, 0s, 0ms");
        assert_eq!(
            7_200_000u128.into_formatted(),
            "2 hours, 0 minutes, 0s, 0ms"
        );
        assert_eq!(
            172_800_000u128.into_formatted(),
            "2 days, 0 hours, 0 minutes, 0s, 0ms"
        );
    }
}
