#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "actix")]
use actix_web::{get, web, Handler as _, HttpRequest, Responder};
#[cfg(feature = "actix")]
pub use actix_web_opentelemetry::RequestTracing;
#[cfg(feature = "actix")]
use actix_web_opentelemetry::{PrometheusMetricsHandler, RequestMetrics, RequestMetricsBuilder};
use moosicbox_logging::free_log_client::DynLayer;
use opentelemetry::{
    global::{self},
    trace::{TraceError, TracerProvider as _},
    InstrumentationScope, KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    metrics::{MeterProviderBuilder, MetricError, SdkMeterProvider},
    propagation::TraceContextPropagator,
    trace::SdkTracerProvider,
    Resource,
};

/// # Errors
///
/// * If the otlp fails to build
pub fn init_tracer(name: &'static str) -> Result<DynLayer, TraceError> {
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

#[derive(Debug)]
pub struct Otel {
    pub meter_provider: SdkMeterProvider,
    #[cfg(feature = "actix")]
    pub request_metrics: RequestMetrics,
    #[cfg(feature = "actix")]
    pub prometheus_metrics_handler: PrometheusMetricsHandler,
}

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
    otel: web::Data<std::sync::Arc<Otel>>,
    request: HttpRequest,
) -> impl Responder {
    otel.prometheus_metrics_handler.call(request).await
}
