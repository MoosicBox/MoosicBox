//! Deterministic in-process renderer and harness for HyperChad UI tests.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod client;
pub mod dom;
pub mod harness;
#[cfg(feature = "test-utils")]
pub mod plan;
pub mod renderer;
pub mod snapshots;
pub mod time;
pub mod transcript;

pub use client::form::FormSubmission;
pub use client::http_events::{HttpEventKind, HttpEventPayload};
pub use client::routing::RouteTable;
pub use client::sse::SseFrame;
pub use harness::{Harness, HarnessError};
pub use renderer::{RendererSnapshot, TestingRenderer};
pub use transcript::{StreamFrame, Transcript};
