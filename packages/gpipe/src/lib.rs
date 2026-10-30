#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Generic Pipelines (gpipe)
//!
//! Native pull-worker orchestration behind the opt-in `runtime` feature, plus
//! workflow authoring types behind `ast`. Runtime execution uses resolved JSON
//! plans, not automatic interpretation of arbitrary GitHub/GitLab YAML.
//!
//! The runtime includes durable SQLite coordination, authenticated HTTP workers,
//! exact-revision native execution, dynamic graph generation, explicit artifacts,
//! Clippier case adapters, and administrator-side hosted-provider API clients.
//! Hosted capacity provisioning, full workflow parity and cross-platform live
//! acceptance remain separate integration milestones. See the package README.

/// Abstract syntax tree types for workflow definitions.
///
/// Re-exported from the `gpipe_ast` crate. Provides core data structures for
/// workflows, jobs, steps, and expressions that can be parsed from various CI/CD
/// formats and executed locally or translated to different backend formats.
///
/// Enable the `ast` feature to access this module.
#[cfg(feature = "ast")]
pub use gpipe_ast as ast;

/// Explicit local content-addressed artifacts.
#[cfg(feature = "runtime")]
pub mod artifacts;
/// Lossless Clippier case-plan import and receipt validation.
#[cfg(feature = "runtime")]
pub mod clippier;
/// Durable transactional orchestration state.
#[cfg(feature = "runtime")]
pub mod coordinator;
/// Native command execution with bounded output and cancellation.
#[cfg(feature = "runtime")]
pub mod executor;
/// Actual MoosicBox case planning and run evidence integration.
#[cfg(feature = "runtime")]
pub mod moosicbox;
/// Bounded authenticated operator API client.
#[cfg(feature = "runtime")]
pub mod operator;
/// Resolved, validated native execution plans.
#[cfg(feature = "runtime")]
pub mod plan;
/// Versioned coordinator and worker messages and configuration.
#[cfg(feature = "runtime")]
pub mod protocol;
/// Hosted provider API adapters with conservative dispatch outcomes.
#[cfg(feature = "runtime")]
pub mod providers;
/// Authenticated coordinator HTTP service.
#[cfg(feature = "runtime")]
pub mod server;

/// Explicit file artifact transport for task dependencies.
#[cfg(feature = "runtime")]
pub mod transfer;
/// Native outbound-polling worker.
#[cfg(feature = "runtime")]
pub mod worker;

/// Signed webhook validation and trusted pipeline template resolution.
#[cfg(feature = "runtime")]
pub mod triggers;
