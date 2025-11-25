//! AWS Lambda runtime support for the `MoosicBox` marketing site.
//!
//! This module provides the core AWS Lambda handler implementation for serving
//! the marketing site in a serverless environment. It manages the async runtime,
//! initializes tracing, and configures the application for Lambda execution.
//!
//! # Environment Variables
//!
//! * `MAX_THREADS` - Maximum blocking threads for async runtime (default: 64)

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc, LazyLock};

use hyperchad::renderer_html::lambda::lambda_http::tracing;
use switchy_async::runtime::Runtime;
use switchy_env::var_parse_or;

static RUNTIME: LazyLock<Arc<Runtime>> = LazyLock::new(|| {
    let threads = var_parse_or("MAX_THREADS", 64usize);
    log::debug!("Running with {threads} max blocking threads");
    let runtime = switchy_async::runtime::Builder::new()
        .max_blocking_threads(u16::try_from(threads).unwrap())
        .build()
        .unwrap();

    Arc::new(runtime)
});

/// Runs the marketing site application in AWS Lambda environment.
///
/// Initializes tracing, builds the application with a runtime handle, and starts
/// serving HTTP requests through the Lambda runtime.
///
/// # Errors
///
/// * If application building fails
/// * If Lambda handler setup fails
///
/// # Panics
///
/// * If static asset route registration fails (via [`moosicbox_marketing_site::init`])
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing::init_default_subscriber();

    let builder = moosicbox_marketing_site::init().with_runtime_handle(RUNTIME.handle());
    moosicbox_marketing_site::build_app(builder)?.handle_serve()?;

    Ok(())
}
