#[cfg(feature = "actix")]
use actix_web::{HttpRequest, HttpResponse, http::StatusCode};
#[cfg(feature = "actix")]
use actix_web_opentelemetry::RequestMetrics;
#[cfg(feature = "actix")]
use futures_util::future::{self, LocalBoxFuture};
use tracing::Subscriber;
use tracing_subscriber::Layer;

pub struct SimulatorLayer;

impl<S: Subscriber> Layer<S> for SimulatorLayer {}

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
