#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use pingora_core::{upstreams::peer::HttpPeer, Result};
use pingora_load_balancing::{selection::RoundRobin, LoadBalancer};
use pingora_proxy::{ProxyHttp, Session};

pub static PORT: Lazy<u16> = Lazy::new(|| {
    std::env::var("PORT")
        .unwrap_or("6188".to_string())
        .parse::<u16>()
        .expect("Invalid PORT")
});

pub static SSL_PORT: Lazy<u16> = Lazy::new(|| {
    std::env::var("SSL_PORT")
        .unwrap_or("6189".to_string())
        .parse::<u16>()
        .expect("Invalid SSL_PORT")
});

static SNI: Lazy<String> = Lazy::new(|| format!("127.0.0.1:{}", *SSL_PORT));

pub struct Router(HashMap<String, Arc<LoadBalancer<RoundRobin>>>);

impl Router {
    pub fn new(upstreams: HashMap<String, Arc<LoadBalancer<RoundRobin>>>) -> Self {
        Self(upstreams)
    }
}

impl Router {
    fn is_challenge(session: &Session) -> bool {
        let path = session.req_header().uri.path();
        path.starts_with("/.well-known/acme-challenge/")
    }
}

#[async_trait]
impl ProxyHttp for Router {
    type CTX = ();

    fn new_ctx(&self) -> Self::CTX {}

    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        let path = session.req_header().uri.path();

        log::debug!("request_filter: path={path}");

        Ok(false)
    }

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let raw_path = std::str::from_utf8(session.req_header().raw_path()).map_err(|e| {
            log::error!("upstream_peer: Failed to parse path: {e:?}");
            pingora_core::Error::new_str("Failed to parse path")
        })?;
        let headers = &session.req_header().headers;
        let host = session
            .req_header()
            .uri
            .host()
            .or_else(|| headers.get("host").and_then(|x| x.to_str().ok()))
            .unwrap_or_default();

        log::debug!(
            "upstream_peer: upstream_peer host={host} raw_path={raw_path} headers={:?} client={:?} server={:?}",
            headers,
            session.client_addr(),
            session.server_addr(),
        );

        let lb = if Self::is_challenge(session) {
            log::debug!("upstream_peer: Received challenge request");
            static NAME: &str = "solver";
            self.0.get(NAME).map(|x| {
                log::debug!("upstream_peer: Using cluster name={NAME}");
                x
            })
        } else {
            self.0
                .get(host)
                .map(|x| {
                    log::debug!("upstream_peer: Using cluster name={host}");
                    x
                })
                .or_else(|| match self.0.get("*") {
                    Some(fallback) => {
                        log::debug!("upstream_peer: Unsupported host={host} Falling back to *");
                        Some(fallback)
                    }
                    None => {
                        log::debug!("upstream_peer: Unsupported host={host}");
                        None
                    }
                })
        };

        let upstream = lb
            .ok_or_else(|| {
                log::error!("upstream_peer: Failed to select a cluster");
                pingora_core::Error::new_str("Failed to select a cluster")
            })?
            .select(b"", 256) // hash doesn't matter
            .ok_or_else(|| {
                log::error!("upstream_peer: Failed to select an upstream");
                pingora_core::Error::new_str("Failed to select an upstream")
            })?;

        log::info!("upstream_peer: upstream peer is: {:?}", upstream);

        Ok(Box::new(HttpPeer::new(upstream, false, SNI.to_string())))
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut pingora_http::RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        if Self::is_challenge(session) {
            log::debug!("upstream_request_filter: is challenge request");
        } else {
            let sni: &str = &SNI;
            upstream_request.insert_header("Host", sni).unwrap();
        }

        log::debug!(
            "upstream_request_filter: path={} headers={:?} client={:?} server={:?}",
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
