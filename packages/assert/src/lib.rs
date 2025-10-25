#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use colored::Colorize;
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
/// ```
/// use moosicbox_assert::assert;
///
/// std::env::set_var("ENABLE_ASSERT", "1");
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
/// ```
/// use moosicbox_assert::assert_or_err;
///
/// #[derive(Debug)]
/// enum MyError {
///     InvalidValue,
/// }
///
/// fn validate(value: i32) -> Result<(), MyError> {
///     assert_or_err!(value >= 0, MyError::InvalidValue);
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
/// ```
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
/// ```should_panic
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
/// ```should_panic
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
/// ```
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
/// ```
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
/// ```
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
/// ```
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
/// ```
/// use moosicbox_assert::die_or_propagate;
/// use std::fs::File;
///
/// fn read_config() -> std::io::Result<String> {
///     let file = die_or_propagate!(File::open("config.txt"), "Failed to open config");
///     Ok(String::new())
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
/// ```should_panic
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
/// ```should_panic
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
