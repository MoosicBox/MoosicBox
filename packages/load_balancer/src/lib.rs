#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use pingora_core::{upstreams::peer::HttpPeer, Result};
use pingora_load_balancing::{selection::RoundRobin, LoadBalancer};
use pingora_proxy::{ProxyHttp, Session};

pub struct Router(HashMap<String, Arc<LoadBalancer<RoundRobin>>>);

impl Router {
    pub fn new(upstreams: HashMap<String, Arc<LoadBalancer<RoundRobin>>>) -> Self {
        Self(upstreams)
    }
}

pub static PORT: Lazy<u16> = Lazy::new(|| {
    std::env::var("PORT")
        .unwrap_or("6188".to_string())
        .parse::<u16>()
        .expect("Invalid PORT")
});

static SNI: Lazy<String> = Lazy::new(|| format!("127.0.0.1:{}", *PORT));

#[async_trait]
impl ProxyHttp for Router {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let raw_path = std::str::from_utf8(session.req_header().raw_path()).map_err(|e| {
            log::error!("Failed to parse path: {e:?}");
            pingora_core::Error::new_str("Failed to parse path")
        })?;
        let headers = &session.req_header().headers;
        let host = headers
            .get("host")
            .and_then(|x| x.to_str().ok())
            .unwrap_or_default();

        log::debug!(
            "upstream_peer raw_path={raw_path} headers={:?} client={:?} server={:?}",
            headers,
            session.client_addr(),
            session.server_addr(),
        );

        let upstream = self
            .0
            .get(host)
            .map(|x| {
                log::debug!("Using cluster name={host}");
                x
            })
            .or_else(|| match self.0.get("*") {
                Some(fallback) => {
                    log::debug!("Unsupported host={host} Falling back to *");
                    Some(fallback)
                }
                None => {
                    log::debug!("Unsupported host={host}");
                    None
                }
            })
            .ok_or_else(|| {
                log::error!("Failed to select a cluster");
                pingora_core::Error::new(pingora_core::ErrorType::UnknownError)
            })?
            .select(b"", 256) // hash doesn't matter
            .ok_or_else(|| {
                log::error!("Failed to select an upstream");
                pingora_core::Error::new(pingora_core::ErrorType::UnknownError)
            })?;

        log::info!("upstream peer is: {:?}", upstream);

        Ok(Box::new(HttpPeer::new(upstream, false, SNI.to_string())))
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut pingora_http::RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        let sni: &str = &SNI;
        upstream_request.insert_header("Host", sni).unwrap();
        log::debug!(
            "upstream_request_filter path={} headers={:?} client={:?} server={:?}",
            std::str::from_utf8(upstream_request.raw_path()).map_err(|e| {
                log::error!("Failed to parse path: {e:?}");
                pingora_core::Error::new_str("Failed to parse path")
            })?,
            upstream_request.headers,
            session.client_addr(),
            session.server_addr(),
        );
        Ok(())
    }
}
