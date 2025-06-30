#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "git-diff")]
pub mod git_diff;

#[cfg(feature = "git-diff")]
pub use git_diff::*;
