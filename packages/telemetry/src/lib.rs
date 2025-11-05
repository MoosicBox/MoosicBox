//! OpenTelemetry-based telemetry and metrics collection for Switchy services.
//!
//! This crate provides integration with OpenTelemetry for distributed tracing and metrics,
//! with optional Actix web integration for HTTP metrics endpoints. It supports both production
//! telemetry (via OTLP) and a simulator mode for testing.
//!
//! # Features
//!
//! * `actix` - Enables Actix web integration for HTTP metrics endpoints
//! * `simulator` - Enables simulator mode with stub implementations for testing
//!
//! # Examples
//!
//! Initialize a tracer for your service:
//!
//! ```rust,no_run
//! use switchy_telemetry::init_tracer;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let layer = init_tracer("my-service")?;
//! // Add the layer to your tracing subscriber
//! # Ok(())
//! # }
//! ```
//!
//! With the `actix` feature, serve metrics via HTTP:
//!
//! ```rust,ignore
//! use switchy_telemetry::{get_http_metrics_handler, metrics, RequestTracing};
//! use actix_web::{App, HttpServer, web};
//!
//! let handler = get_http_metrics_handler();
//! HttpServer::new(move || {
//!     App::new()
//!         .wrap(RequestTracing::new())
//!         .app_data(web::Data::new(std::sync::Arc::new(handler.clone())))
//!         .service(metrics)
//! })
//! .bind("127.0.0.1:8080")?
//! .run()
//! .await
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "simulator")]
mod simulator;

#[cfg(feature = "actix")]
use actix_web::{HttpRequest, Responder, get, web};
#[cfg(feature = "actix")]
use actix_web_opentelemetry::RequestMetrics;
/// Middleware for tracing HTTP requests with OpenTelemetry.
///
/// This middleware automatically creates spans for incoming HTTP requests and records
/// relevant metadata such as HTTP method, path, and status code.
#[cfg(feature = "actix")]
pub use actix_web_opentelemetry::RequestTracing;
#[cfg(feature = "actix")]
use futures_util::future::LocalBoxFuture;
use moosicbox_logging::free_log_client::DynLayer;
use opentelemetry::KeyValue;
use opentelemetry_otlp::ExporterBuildError;
use opentelemetry_sdk::Resource;

/// Initializes an OpenTelemetry tracer layer for the given service.
///
/// In simulator mode, returns a no-op layer. Otherwise, creates a tracer that exports
/// spans to an OTLP endpoint via gRPC.
///
/// # Errors
///
/// * If the OTLP exporter fails to build
pub fn init_tracer(#[allow(unused)] name: &'static str) -> Result<DynLayer, ExporterBuildError> {
    #[cfg(feature = "simulator")]
    {
        Ok(Box::new(simulator::SimulatorLayer))
    }

    #[cfg(not(feature = "simulator"))]
    {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::WithExportConfig as _;

        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(
                opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_endpoint(switchy_env::var_or(
                        "OTEL_ENDPOINT",
                        "http://127.0.0.1:4317",
                    ))
                    .build()?,
            )
            .with_resource(get_resource_attr(name))
            .build();

        let scope = opentelemetry::InstrumentationScope::builder(name)
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_schema_url("https://opentelemetry.io/schema/1.2.0")
            .build();

        let tracer = provider.tracer_with_scope(scope);
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let layer: DynLayer = Box::new(layer);

        opentelemetry::global::set_tracer_provider(provider);

        Ok(layer)
    }
}

/// Creates an OpenTelemetry resource with service name attributes.
#[must_use]
pub fn get_resource_attr(name: &'static str) -> Resource {
    Resource::builder()
        .with_service_name(name)
        .with_attribute(KeyValue::new("service.name", name))
        .build()
}

/// HTTP metrics handler for Actix web applications.
#[cfg(feature = "actix")]
pub trait HttpMetricsHandler: Send + Sync + std::fmt::Debug {
    /// Handles HTTP metrics endpoint requests.
    fn call(
        &self,
        request: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<actix_web::HttpResponse<String>, actix_web::error::Error>>;

    /// Returns the request metrics middleware.
    fn request_middleware(&self) -> RequestMetrics;
}

/// Stub HTTP metrics handler that returns empty responses.
#[derive(Debug)]
#[cfg(all(feature = "actix", not(feature = "simulator")))]
pub struct StubHttpMetricsHandler;

#[cfg(all(feature = "actix", not(feature = "simulator")))]
impl crate::HttpMetricsHandler for StubHttpMetricsHandler {
    fn call(
        &self,
        _request: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<actix_web::HttpResponse<String>, actix_web::error::Error>>
    {
        Box::pin(futures_util::future::ok(
            actix_web::HttpResponse::with_body(actix_web::http::StatusCode::OK, String::new()),
        ))
    }

    fn request_middleware(&self) -> RequestMetrics {
        RequestMetrics::builder().build()
    }
}

/// Returns the HTTP metrics handler implementation.
///
/// Uses the simulator implementation when the `simulator` feature is enabled.
#[cfg(feature = "actix")]
#[must_use]
pub fn get_http_metrics_handler() -> Box<dyn HttpMetricsHandler> {
    #[cfg(feature = "simulator")]
    {
        Box::new(simulator::SimulatorHttpMetricsHandler)
    }

    #[cfg(not(feature = "simulator"))]
    {
        Box::new(StubHttpMetricsHandler)
    }
}

/// Actix web endpoint for serving telemetry metrics.
///
/// This endpoint delegates to the configured [`HttpMetricsHandler`] to serve metrics
/// in the appropriate format (e.g., Prometheus text format).
///
/// # Errors
///
/// * Returns errors from the underlying [`HttpMetricsHandler::call`] method
#[allow(clippy::future_not_send)]
#[cfg(feature = "actix")]
#[tracing::instrument]
#[get("/metrics")]
pub async fn metrics(
    otel: web::Data<std::sync::Arc<Box<dyn HttpMetricsHandler>>>,
    request: HttpRequest,
) -> impl Responder {
    otel.call(request).await
}
