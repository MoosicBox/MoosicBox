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

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn test_simulator_layer_implements_layer_trait() {
        // Create a basic subscriber with the simulator layer
        let layer = SimulatorLayer;
        let subscriber = tracing_subscriber::registry().with(layer);

        // If we can create a subscriber with the layer, it implements the trait correctly
        drop(subscriber);
    }

    #[cfg(feature = "actix")]
    #[test]
    fn test_simulator_http_metrics_handler_request_middleware() {
        use crate::HttpMetricsHandler;

        let handler = SimulatorHttpMetricsHandler;
        let _middleware = handler.request_middleware();
        // If we can create the middleware without panicking, the test passes
    }

    #[cfg(feature = "actix")]
    #[test]
    fn test_simulator_http_metrics_handler_debug() {
        let handler = SimulatorHttpMetricsHandler;
        let debug_output = format!("{handler:?}");
        assert!(
            debug_output.contains("SimulatorHttpMetricsHandler"),
            "Debug output should contain the handler name"
        );
    }
}
