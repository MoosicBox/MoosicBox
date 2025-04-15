#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "simulator")]
mod simulator;

#[cfg(feature = "actix")]
use actix_web::{HttpRequest, Responder, get, web};
#[cfg(feature = "actix")]
pub use actix_web_opentelemetry::RequestTracing;
#[cfg(feature = "actix")]
use actix_web_opentelemetry::{RequestMetrics, RequestMetricsBuilder};
#[cfg(feature = "actix")]
use futures_util::future::LocalBoxFuture;
use moosicbox_logging::free_log_client::DynLayer;
use opentelemetry::KeyValue;
use opentelemetry_otlp::ExporterBuildError;
use opentelemetry_sdk::Resource;
#[cfg(feature = "actix")]
use opentelemetry_sdk::metrics::{MeterProviderBuilder, MetricError, SdkMeterProvider};

/// # Errors
///
/// * If the otlp fails to build
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
                    .with_endpoint(
                        std::env::var("OTEL_ENDPOINT")
                            .as_deref()
                            .unwrap_or("http://127.0.0.1:4317"),
                    )
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
    {
        Ok(Box::new(simulator::SimulatorHttpMetricsHandler))
    }

    #[cfg(not(feature = "simulator"))]
    {
        Ok(Box::new(simulator::SimulatorHttpMetricsHandler))
    }
}

#[derive(Debug)]
#[cfg(feature = "actix")]
struct Otel {
    #[allow(unused)]
    pub meter_provider: SdkMeterProvider,
    #[allow(unused)]
    pub request_metrics: RequestMetrics,
}

#[cfg(feature = "actix")]
#[allow(dead_code)]
impl Otel {
    /// # Errors
    ///
    /// * If the Prometheus exporter fails to build
    pub fn new() -> Result<Self, MetricError> {
        let registry = prometheus::Registry::default();

        #[cfg(feature = "actix")]
        let prometheus_registry = registry;
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

        Ok(Self {
            meter_provider,
            #[cfg(feature = "actix")]
            request_metrics,
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
