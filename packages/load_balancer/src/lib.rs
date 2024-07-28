#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(not(target_os = "windows"))]
mod load_balancer;

#[cfg(not(target_os = "windows"))]
pub use load_balancer::*;
