use std::sync::OnceLock;

use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorInternalServerError};
use futures::future::{Ready, err, ok};

static SERVICE_INFO: OnceLock<ServiceInfo> = OnceLock::new();

/// # Errors
///
/// Will error if `SERVICE_INFO` has already been initialized
pub fn init(service_info: ServiceInfo) -> Result<(), ServiceInfo> {
    SERVICE_INFO.set(service_info)
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub port: u16,
}

impl FromRequest for ServiceInfo {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let Some(service_info) = SERVICE_INFO.get().cloned() else {
            return err(ErrorInternalServerError(
                "Config service_info not initialized",
            ));
        };

        ok(service_info)
    }
}
