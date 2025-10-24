use std::sync::{Arc, OnceLock};

use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorInternalServerError};
use futures::future::{Ready, err, ok};

static TUNNEL_INFO: OnceLock<TunnelInfo> = OnceLock::new();

/// # Errors
///
/// Will error if `TUNNEL_INFO` has already been initialized
pub fn init(tunnel_info: TunnelInfo) -> Result<(), TunnelInfo> {
    TUNNEL_INFO.set(tunnel_info)
}

/// Tunnel configuration information accessible via Actix-web request extraction.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct TunnelInfo {
    /// The tunnel host, if configured.
    pub host: Arc<Option<String>>,
}

impl FromRequest for TunnelInfo {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let Some(tunnel_info) = TUNNEL_INFO.get().cloned() else {
            return err(ErrorInternalServerError(
                "Config tunnel_info not initialized",
            ));
        };

        ok(tunnel_info)
    }
}
// pub service_port: u16,
