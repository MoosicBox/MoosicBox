#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

pub use colored::Colorize;
pub use moosicbox_env_utils;

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

#[macro_export]
macro_rules! assert_or_err {
    ($evaluate:expr, $err:expr, $(,)?) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate)
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

#[macro_export]
macro_rules! assert_or_unimplemented {
    ($evaluate:expr, $(,)?) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate)
        } else if !($evaluate) {
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
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate, $($message)*)
        } else if !($evaluate) {
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

#[macro_export]
macro_rules! assert_or_panic {
    ($evaluate:expr, $(,)?) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate)
        } else if !($evaluate) {
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
    ($evaluate:expr, $($message:tt)+) => {
        if $crate::moosicbox_env_utils::default_env!("ENABLE_ASSERT", "false") == "1"
            && !($evaluate)
        {
            $crate::assert!($evaluate, $($message)*)
        } else if !($evaluate) {
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
