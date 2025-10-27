//! Simulator implementations for testing without real telemetry backends.
//!
//! This module provides stub implementations of telemetry components that can be used
//! in testing environments where actual OpenTelemetry infrastructure is not available.
//! The simulator implementations satisfy the required traits but perform no actual
//! telemetry operations.

#[cfg(feature = "actix")]
use actix_web::{HttpRequest, HttpResponse, http::StatusCode};
#[cfg(feature = "actix")]
use actix_web_opentelemetry::RequestMetrics;
#[cfg(feature = "actix")]
use futures_util::future::{self, LocalBoxFuture};
use tracing::Subscriber;
use tracing_subscriber::Layer;

/// A no-op tracing layer for simulator mode.
///
/// This layer satisfies the `Layer` trait but performs no actual tracing operations,
/// making it suitable for testing environments.
pub struct SimulatorLayer;

impl<S: Subscriber> Layer<S> for SimulatorLayer {}

/// A no-op HTTP metrics handler for simulator mode.
///
/// This handler returns empty responses for metrics requests and provides
/// default middleware, making it suitable for testing without a real metrics backend.
#[derive(Debug)]
#[cfg(feature = "actix")]
pub struct SimulatorHttpMetricsHandler;

#[cfg(feature = "actix")]
impl crate::HttpMetricsHandler for SimulatorHttpMetricsHandler {
    fn call(
        &self,
        _request: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<HttpResponse<String>, actix_web::error::Error>> {
        Box::pin(future::ok(actix_web::HttpResponse::with_body(
            StatusCode::OK,
            String::new(),
        )))
    }

    fn request_middleware(&self) -> RequestMetrics {
        RequestMetrics::builder().build()
    }
}
