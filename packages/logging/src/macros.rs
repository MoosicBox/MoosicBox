pub use log;

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
