#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[macro_export]
macro_rules! assert {
    ($evaluate:expr $(,)?) => {
        if default_env!("ENABLE_ASSERT", "false") == "1" && !($evaluate) {
            eprintln!(
                "assert failed:\n{}",
                std::backtrace::Backtrace::force_capture()
            );
            std::process::exit(1);
        }
    };
    ($evaluate:expr, $message:expr $(,)?) => {
        if default_env!("ENABLE_ASSERT", "false") == "1" && !($evaluate) {
            eprintln!(
                "assert failed: \"{}\"\n{}",
                $message,
                std::backtrace::Backtrace::force_capture()
            );
            std::process::exit(1);
        }
    };
}
