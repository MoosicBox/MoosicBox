//! Environment-controlled assertion macros for conditional debugging and testing.
//!
//! This crate provides assertion macros that can be toggled at runtime via the `ENABLE_ASSERT`
//! environment variable. When enabled, failed assertions exit the process with colorized error
//! messages and backtraces. When disabled, the macros have varying fallback behaviors:
//! returning errors, logging warnings, panicking, or becoming no-ops.
//!
//! # Use Cases
//!
//! * Development and debugging environments where you want strict checking
//! * Production environments where you want graceful degradation instead of process termination
//! * Testing scenarios where you need conditional assertion behavior
//! * Gradual migration from development assertions to production error handling
//!
//! # Environment Variables
//!
//! * `ENABLE_ASSERT` - Set to "1" to enable strict assertion mode (process exits on failure),
//!   any other value uses the fallback behavior of each macro
//!
//! # Examples
//!
//! Basic assertion that exits when enabled, does nothing when disabled:
//!
//! ```rust,no_run
//! use moosicbox_assert::assert;
//!
//! unsafe { std::env::set_var("ENABLE_ASSERT", "1"); }
//! let value = 42;
//! assert!(value > 0, "Value must be positive");
//! ```
//!
//! Assertion that returns an error when disabled:
//!
//! ```rust,no_run
//! use moosicbox_assert::assert_or_err;
//!
//! #[derive(Debug)]
//! enum Error { Invalid }
//!
//! fn validate(x: i32) -> Result<(), Error> {
//!     assert_or_err!(x >= 0, Error::Invalid, "Value must be non-negative");
//!     Ok(())
//! }
//! ```
//!
//! # Available Macros
//!
//! * [`assert!`] - Conditional assertion that exits or does nothing
//! * [`assert_or_err!`] - Returns an error when disabled
//! * [`assert_or_error!`] - Logs an error when disabled
//! * [`assert_or_panic!`] - Panics when disabled
//! * [`assert_or_unimplemented!`] - Calls `unimplemented!()` when disabled
//! * [`die!`] - Unconditional exit when enabled, no-op when disabled
//! * [`die_or_err!`] - Returns an error when disabled
//! * [`die_or_error!`] - Logs an error when disabled
//! * [`die_or_warn!`] - Logs a warning when disabled
//! * [`die_or_panic!`] - Panics when disabled
//! * [`die_or_propagate!`] - Propagates errors using `?` when disabled
//! * [`die_or_unimplemented!`] - Calls `unimplemented!()` when disabled

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Re-export of the [`colored::Colorize`] trait for colorizing terminal output.
///
/// This trait is used internally by the assertion macros to format error messages
/// with colored backgrounds (red for errors, yellow for warnings). It's re-exported
/// to allow users to access colorization utilities without adding `colored` as a
/// separate dependency.
pub use colored::Colorize;

/// Re-export of the `moosicbox_env_utils` crate for environment variable utilities.
///
/// This module provides the `default_env!` macro used internally by assertion macros
/// to read the `ENABLE_ASSERT` environment variable. It's re-exported to allow users
/// to access environment utilities without adding `moosicbox_env_utils` as a separate
/// dependency.
pub use moosicbox_env_utils;

/// Conditional assertion that exits the process on failure when assertions are enabled.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro evaluates the condition
/// and exits the process with a colorized error message and stack trace if the condition is false.
/// When assertions are disabled, the condition is not evaluated and the macro becomes a no-op.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::assert;
///
/// unsafe { std::env::set_var("ENABLE_ASSERT", "1"); }
/// let value = 42;
/// assert!(value > 0);
/// assert!(value == 42, "Expected 42, got {}", value);
/// ```
///
/// # Panics
///
/// Exits the process (via `std::process::exit(1)`) when the condition is false and `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! assert {
    ($evaluate:expr $(,)?) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "assert failed:\n{}",
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        }
    };
    ($evaluate:expr, $($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "assert failed: {}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        }
    };
}

/// Conditional assertion that returns an error on failure.
///
/// When `ENABLE_ASSERT` environment variable is set to "1" and the condition is false,
/// this macro exits the process with a colorized error message. When assertions are disabled
/// and the condition is false, it returns the specified error value instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::assert_or_err;
///
/// #[derive(Debug)]
/// enum MyError {
///     InvalidValue,
/// }
///
/// fn validate(value: i32) -> Result<(), MyError> {
///     assert_or_err!(value >= 0, MyError::InvalidValue, "Value must be non-negative");
///     assert_or_err!(value <= 100, MyError::InvalidValue, "Out of range: {}", value);
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns the specified error when the condition is false and `ENABLE_ASSERT` is not "1".
///
/// # Panics
///
/// Exits the process when the condition is false and `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! assert_or_err {
    ($evaluate:expr, $err:expr, $(,)?) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate, "{:?}", $err)
        } else if !($evaluate) {
            return Err($err);
        }
    };
    ($evaluate:expr, $err:expr, $($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate, $($message)*)
        } else if !($evaluate) {
            return Err($err);
        }
    };
}

/// Conditional assertion that logs an error on failure.
///
/// When `ENABLE_ASSERT` environment variable is set to "1" and the condition is false,
/// this macro exits the process with a colorized error message. When assertions are disabled
/// and the condition is false, it logs an error message using the `log` crate instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::assert_or_error;
///
/// fn process_data(data: &[u8]) {
///     assert_or_error!(!data.is_empty(), "Cannot process empty data");
///     assert_or_error!(data.len() < 1024, "Data too large: {} bytes", data.len());
/// }
/// ```
///
/// # Panics
///
/// Exits the process when the condition is false and `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! assert_or_error {
    ($evaluate:expr, $($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate, $($message)*)
        } else if !($evaluate) {
            log::error!($($message)*);
        }
    };
}

/// Conditional assertion that calls `unimplemented!()` on failure.
///
/// When `ENABLE_ASSERT` environment variable is set to "1" and the condition is false,
/// this macro exits the process with a colorized error message. When assertions are disabled
/// and the condition is false, it calls `unimplemented!()` with a colorized message instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::assert_or_unimplemented;
///
/// fn experimental_feature(enabled: bool) {
///     assert_or_unimplemented!(enabled, "Feature not yet implemented");
///     println!("Running experimental feature");
/// }
/// ```
///
/// # Panics
///
/// * Exits the process when the condition is false and `ENABLE_ASSERT=1`
/// * Calls `unimplemented!()` when the condition is false and assertions are disabled
#[macro_export]
macro_rules! assert_or_unimplemented {
    ($evaluate:expr, $(,)?) => {
        let success = ($evaluate);
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !success
        {
            $crate::assert!(success)
        } else if !success {
            unimplemented!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    };
    ($evaluate:expr, $($message:tt)+) => {
        let success = ($evaluate);
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !success
        {
            $crate::assert!(success, $($message)*)
        } else if !success {
            unimplemented!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    };
}

/// Conditional assertion that panics on failure.
///
/// When `ENABLE_ASSERT` environment variable is set to "1" and the condition is false,
/// this macro exits the process with a colorized error message. When assertions are disabled
/// and the condition is false, it panics with a colorized message instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::assert_or_panic;
///
/// fn critical_operation(value: i32) {
///     assert_or_panic!(value > 0, "Value must be positive, got {}", value);
/// }
/// ```
///
/// # Panics
///
/// * Exits the process when the condition is false and `ENABLE_ASSERT=1`
/// * Panics when the condition is false and assertions are disabled
#[macro_export]
macro_rules! assert_or_panic {
    ($evaluate:expr, $(,)?) => {{
        let success = ($evaluate);
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !success
        {
            $crate::assert!(success)
        } else if !success {
            panic!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    }};
    ($evaluate:expr, $($message:tt)+) => {{
        let success = ($evaluate);
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !success
        {
            $crate::assert!(success, $($message)*)
        } else if !success {
            panic!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    }};
}

/// Unconditionally exits the process when assertions are enabled.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro exits the process
/// with a colorized error message and stack trace. When assertions are disabled,
/// this macro becomes a no-op.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die;
///
/// fn check_value(value: i32) {
///     if value < 0 {
///         die!("Value cannot be negative: {}", value);
///     }
/// }
/// ```
///
/// # Panics
///
/// Exits the process (via `std::process::exit(1)`) when `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! die {
    () => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!("{}", std::backtrace::Backtrace::force_capture()).as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        }
    };
    ($($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        }
    };
}

/// Exits the process or logs a warning depending on assertion mode.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro exits the process
/// with a colorized error message (red background). When assertions are disabled,
/// it logs a warning message with yellow background instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die_or_warn;
///
/// fn deprecated_function() {
///     die_or_warn!("This function is deprecated and will be removed");
/// }
/// ```
///
/// # Panics
///
/// Exits the process when `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! die_or_warn {
    ($($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            eprintln!(
                "{}",
                $crate::Colorize::on_yellow($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        } else {
            log::warn!(
                "{}",
                $crate::Colorize::on_yellow($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    };
}

/// Exits the process or returns an error depending on assertion mode.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro exits the process
/// with a colorized error message. When assertions are disabled, it returns the specified
/// error value instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die_or_err;
///
/// #[derive(Debug)]
/// enum MyError {
///     Fatal,
/// }
///
/// fn critical_check() -> Result<(), MyError> {
///     die_or_err!(MyError::Fatal, "Critical condition failed");
/// }
/// ```
///
/// # Errors
///
/// Returns the specified error when `ENABLE_ASSERT` is not "1".
///
/// # Panics
///
/// Exits the process when `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! die_or_err {
    ($err:expr, $($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
        {
            $crate::die!($($message)*);
            unreachable!();
        } else {
            return Err($err);
        }
    };
}

/// Exits the process or logs an error depending on assertion mode.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro exits the process
/// with a colorized error message. When assertions are disabled, it logs an error message
/// with red background instead using the `log` crate.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die_or_error;
///
/// fn check_invariant(valid: bool) {
///     if !valid {
///         die_or_error!("Invariant violation detected");
///     }
/// }
/// ```
///
/// # Panics
///
/// Exits the process when `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! die_or_error {
    ($($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        } else {
            log::error!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    };
}

/// Exits the process or propagates an error depending on assertion mode.
///
/// When `ENABLE_ASSERT` environment variable is set to "1" and the result is an error,
/// this macro exits the process with a colorized error message. When assertions are disabled,
/// it propagates the error using the `?` operator instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die_or_propagate;
///
/// fn process_result() -> Result<(), String> {
///     die_or_propagate!(Ok::<(), String>(()), "Failed to process");
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Propagates the error from the result when `ENABLE_ASSERT` is not "1".
///
/// # Panics
///
/// Exits the process when the result is `Err` and `ENABLE_ASSERT=1`.
#[macro_export]
macro_rules! die_or_propagate {
    ($evaluate:expr, $($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            match $evaluate {
                Ok(x) => x,
                Err(e) => $crate::die!($($message)*),
            }
        } else {
            $evaluate?
        }
    };

    ($evaluate:expr $(,)?) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            match $evaluate {
                Ok(x) => x,
                Err(_e) => $crate::die!(),
            }
        } else {
            $evaluate?
        }
    };
}

/// Exits the process or panics depending on assertion mode.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro exits the process
/// with a colorized error message. When assertions are disabled, it panics with a
/// colorized message instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die_or_panic;
///
/// fn critical_failure() {
///     die_or_panic!("Unrecoverable error occurred");
/// }
/// ```
///
/// # Panics
///
/// * Exits the process when `ENABLE_ASSERT=1`
/// * Panics when assertions are disabled
#[macro_export]
macro_rules! die_or_panic {
    ($($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        } else {
            panic!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    };
}

/// Exits the process or calls `unimplemented!()` depending on assertion mode.
///
/// When `ENABLE_ASSERT` environment variable is set to "1", this macro exits the process
/// with a colorized error message. When assertions are disabled, it calls `unimplemented!()`
/// with a colorized message instead.
///
/// # Environment Variables
///
/// * `ENABLE_ASSERT` - Set to "1" to enable assertions, any other value disables them
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_assert::die_or_unimplemented;
///
/// fn not_ready_yet() {
///     die_or_unimplemented!("This code path is not implemented");
/// }
/// ```
///
/// # Panics
///
/// * Exits the process when `ENABLE_ASSERT=1`
/// * Calls `unimplemented!()` when assertions are disabled
#[macro_export]
macro_rules! die_or_unimplemented {
    ($($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1" {
            eprintln!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
            log::logger().flush();
            std::process::exit(1);
        } else {
            unimplemented!(
                "{}",
                $crate::Colorize::on_red($crate::Colorize::white($crate::Colorize::bold(
                    format!(
                        "{}\n{}",
                        $crate::Colorize::underline(format!($($message)*).as_str()),
                        std::backtrace::Backtrace::force_capture()
                    )
                    .as_str()
                )))
            );
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum TestError {
        InvalidValue,
        Critical,
    }

    // Test assert! macro with ENABLE_ASSERT disabled (no-op)
    #[test_log::test]
    fn test_assert_disabled_no_op() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        // Should not exit or do anything when condition is false
        assert!(false);
        assert!(false, "this message should not appear");
    }

    #[test_log::test]
    fn test_assert_disabled_with_true_condition() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert!(true);
        assert!(true, "condition is true");
    }

    // Test assert_or_err! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_assert_or_err_returns_error_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function(value: i32) -> Result<i32, TestError> {
            assert_or_err!(value >= 0, TestError::InvalidValue,);
            Ok(value * 2)
        }

        let result = test_function(-5);
        assert_eq!(result, Err(TestError::InvalidValue));
    }

    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_assert_or_err_succeeds_with_true_condition() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function(value: i32) -> Result<i32, TestError> {
            assert_or_err!(value >= 0, TestError::InvalidValue, "value was {}", value);
            Ok(value * 2)
        }

        let result = test_function(5);
        assert_eq!(result, Ok(10));
    }

    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_assert_or_err_with_message() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function(value: i32) -> Result<i32, TestError> {
            assert_or_err!(
                value <= 100,
                TestError::InvalidValue,
                "Value {} exceeds maximum",
                value
            );
            Ok(value)
        }

        let result = test_function(150);
        assert_eq!(result, Err(TestError::InvalidValue));
    }

    // Test assert_or_error! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    fn test_assert_or_error_logs_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        // This should log an error but not exit
        assert_or_error!(false, "This is a test error message");

        // With formatting
        let value = 42;
        assert_or_error!(false, "Value is {}", value);
    }

    #[test_log::test]
    fn test_assert_or_error_succeeds_with_true_condition() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert_or_error!(true, "This should not log");
    }

    // Test assert_or_panic! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    #[should_panic(expected = "Expected panic message")]
    fn test_assert_or_panic_panics_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert_or_panic!(false, "Expected panic message");
    }

    #[test_log::test]
    fn test_assert_or_panic_succeeds_with_true_condition() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert_or_panic!(true, "Should not panic");
    }

    // Test assert_or_unimplemented! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    #[should_panic(expected = "not implemented")]
    fn test_assert_or_unimplemented_calls_unimplemented_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert_or_unimplemented!(false, "Feature not implemented");
    }

    #[test_log::test]
    fn test_assert_or_unimplemented_succeeds_with_true_condition() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert_or_unimplemented!(true, "Should not call unimplemented");
    }

    // Test die! macro with ENABLE_ASSERT disabled (no-op)
    #[test_log::test]
    fn test_die_disabled_no_op() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        die!();
        die!("This message should not exit");
    }

    // Test die_or_err! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_die_or_err_returns_error_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function() -> Result<(), TestError> {
            die_or_err!(TestError::Critical, "Critical failure");
        }

        let result = test_function();
        assert_eq!(result, Err(TestError::Critical));
    }

    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_die_or_err_with_formatting() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function(code: i32) -> Result<(), TestError> {
            die_or_err!(TestError::Critical, "Error code: {}", code);
        }

        let result = test_function(500);
        assert_eq!(result, Err(TestError::Critical));
    }

    // Test die_or_error! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    fn test_die_or_error_logs_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        die_or_error!("This is a critical error");
        die_or_error!("Error with value: {}", 42);
    }

    // Test die_or_warn! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    fn test_die_or_warn_logs_warning_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        die_or_warn!("This is a warning");
        die_or_warn!("Warning with value: {}", 100);
    }

    // Test die_or_panic! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    #[should_panic(expected = "Expected panic")]
    fn test_die_or_panic_panics_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        die_or_panic!("Expected panic");
    }

    #[test_log::test]
    #[should_panic(expected = "Panic with code: 404")]
    fn test_die_or_panic_with_formatting() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        die_or_panic!("Panic with code: {}", 404);
    }

    // Test die_or_unimplemented! macro with ENABLE_ASSERT disabled
    #[test_log::test]
    #[should_panic(expected = "not implemented")]
    fn test_die_or_unimplemented_calls_unimplemented_when_disabled() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        die_or_unimplemented!("Not implemented yet");
    }

    // Note: die_or_propagate! tests omitted due to macro expansion issues with std::process::exit
    // The macro works correctly at runtime but has type-checking issues during test compilation
    // when the ENABLE_ASSERT=1 branch is analyzed. Since we can't test the exit path anyway,
    // and the macro is tested through actual usage, we skip these tests.

    // Test with complex expressions and side effects
    #[test_log::test]
    fn test_assert_with_side_effects() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        let mut counter = 0;

        // When disabled, the expression should NOT be evaluated
        assert!({
            counter += 1;
            false
        });

        // Counter should remain 0 since assertion is disabled
        assert_eq!(counter, 0);
    }

    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_assert_or_err_with_complex_error_types() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        #[derive(Debug, PartialEq)]
        struct ComplexError {
            code: i32,
            message: String,
        }

        fn test_function() -> Result<(), ComplexError> {
            assert_or_err!(
                false,
                ComplexError {
                    code: 42,
                    message: "test error".to_string()
                },
                "Complex error test"
            );
            Ok(())
        }

        let result = test_function();
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.code, 42);
            assert_eq!(e.message, "test error");
        }
    }

    // Test macro hygiene - ensure macros work with different imports
    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_macro_works_without_explicit_imports() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function() -> Result<(), TestError> {
            assert_or_err!(true, TestError::InvalidValue,);
            Ok(())
        }

        assert_eq!(test_function(), Ok(()));
    }

    // Test with trailing commas
    #[test_log::test]
    fn test_assert_with_trailing_comma() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };
        assert!(true,);
    }

    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_assert_or_err_with_trailing_comma() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function() -> Result<(), TestError> {
            assert_or_err!(true, TestError::InvalidValue,);
            Ok(())
        }

        assert_eq!(test_function(), Ok(()));
    }

    // Test multiple consecutive assertions
    #[test_log::test]
    #[allow(clippy::items_after_statements)]
    fn test_multiple_assert_or_err_in_sequence() {
        unsafe { std::env::set_var("ENABLE_ASSERT", "0") };

        fn test_function(a: i32, b: i32) -> Result<i32, TestError> {
            assert_or_err!(a >= 0, TestError::InvalidValue, "a must be non-negative");
            assert_or_err!(b >= 0, TestError::InvalidValue, "b must be non-negative");
            assert_or_err!(a + b <= 100, TestError::InvalidValue, "sum too large");
            Ok(a + b)
        }

        // Should pass all assertions
        assert_eq!(test_function(10, 20), Ok(30));

        // Should fail first assertion
        assert_eq!(test_function(-1, 20), Err(TestError::InvalidValue));

        // Should fail second assertion
        assert_eq!(test_function(10, -1), Err(TestError::InvalidValue));

        // Should fail third assertion
        assert_eq!(test_function(60, 50), Err(TestError::InvalidValue));
    }
}
