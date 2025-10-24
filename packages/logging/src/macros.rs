pub use log;

/// Logs a message at trace level if trace logging is enabled, otherwise at debug level.
///
/// This macro is useful for detailed logging that can be conditionally more verbose
/// when trace-level logging is active.
#[macro_export]
macro_rules! debug_or_trace {
    (($($debug:tt)+), ($($trace:tt)+)) => {
        if $crate::log::log_enabled!(log::Level::Trace) {
            $crate::log::trace!($($trace)*);
        } else {
            $crate::log::debug!($($debug)*);
        }
    }
}
