#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "simulator")]
mod simulator;

#[cfg(feature = "actix")]
use actix_web::{Handler as _, HttpRequest, Responder, get, web};
#[cfg(feature = "actix")]
pub use actix_web_opentelemetry::RequestTracing;
#[cfg(feature = "actix")]
use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestMetricsBuilder};
#[cfg(feature = "actix")]
use futures_util::future::LocalBoxFuture;
use moosicbox_logging::free_log_client::DynLayer;
use opentelemetry::{
    InstrumentationScope, KeyValue,
    global::{self},
    trace::TracerProvider as _,
};
use opentelemetry_otlp::{ExporterBuildError, WithExportConfig};
#[cfg(feature = "actix")]
use opentelemetry_sdk::metrics::{MeterProviderBuilder, MetricError, SdkMeterProvider};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};

/// # Errors
///
/// * If the otlp fails to build
pub fn init_tracer(name: &'static str) -> Result<DynLayer, ExporterBuildError> {
    #[cfg(feature = "simulator")]
    if moosicbox_simulator_utils::simulator_enabled() {
        return Ok(Box::new(simulator::SimulatorLayer));
    }

    global::set_text_map_propagator(TraceContextPropagator::new());

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(
            opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(
                    std::env::var("OTEL_ENDPOINT")
                        .as_deref()
                        .unwrap_or("http://127.0.0.1:4317"),
                )
                .build()?,
        )
        .with_resource(get_resource_attr(name))
        .build();

    let scope = InstrumentationScope::builder(name)
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_schema_url("https://opentelemetry.io/schema/1.2.0")
        .build();

    let tracer = provider.tracer_with_scope(scope);
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let layer: DynLayer = Box::new(layer);

    global::set_tracer_provider(provider);

    Ok(layer)
}

#[must_use]
pub fn get_resource_attr(name: &'static str) -> Resource {
    Resource::builder()
        .with_service_name(name)
        .with_attribute(KeyValue::new("service.name", name))
        .build()
}

#[cfg(feature = "actix")]
pub trait HttpMetricsHandler: Send + Sync + std::fmt::Debug {
    fn call(
        &self,
        request: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<actix_web::HttpResponse<String>, actix_web::error::Error>>;

    fn request_middleware(&self) -> RequestMetrics;
}

/// # Errors
///
/// * If the Prometheus exporter fails to build
#[cfg(feature = "actix")]
pub fn get_http_metrics_handler() -> Result<Box<dyn HttpMetricsHandler>, MetricError> {
    #[cfg(feature = "simulator")]
    if moosicbox_simulator_utils::simulator_enabled() {
        return Ok(Box::new(simulator::SimulatorHttpMetricsHandler));
    }

    Ok(Box::new(Otel::new()?))
}

#[derive(Debug)]
#[cfg(feature = "actix")]
struct Otel {
    #[allow(unused)]
    pub meter_provider: SdkMeterProvider,
    pub request_metrics: RequestMetrics,
    pub prometheus_metrics_handler: PrometheusMetricsHandler,
}

#[cfg(feature = "actix")]
impl HttpMetricsHandler for Otel {
    fn call(
        &self,
        request: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<actix_web::HttpResponse<String>, actix_web::error::Error>>
    {
        self.prometheus_metrics_handler.call(request)
    }

    fn request_middleware(&self) -> RequestMetrics {
        self.request_metrics.clone()
    }
}

#[cfg(feature = "actix")]
impl Otel {
    /// # Errors
    ///
    /// * If the Prometheus exporter fails to build
    pub fn new() -> Result<Self, MetricError> {
        let registry = prometheus::Registry::default();

        #[cfg(feature = "actix")]
        let prometheus_registry = registry.clone();
        #[cfg(not(feature = "actix"))]
        let prometheus_registry = registry;

        let prometheus_exporter = opentelemetry_prometheus::exporter()
            .with_registry(prometheus_registry)
            .build()?;

        let meter_provider = MeterProviderBuilder::default()
            .with_reader(prometheus_exporter)
            .build();

        #[cfg(feature = "actix")]
        let request_metrics = RequestMetricsBuilder::new()
            .with_meter_provider(meter_provider.clone())
            .build();

        #[cfg(feature = "actix")]
        let prometheus_metrics_handler = PrometheusMetricsHandler::new(registry);

        Ok(Self {
            meter_provider,
            #[cfg(feature = "actix")]
            request_metrics,
            #[cfg(feature = "actix")]
            prometheus_metrics_handler,
        })
    }
}

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
