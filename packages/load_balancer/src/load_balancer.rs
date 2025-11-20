//! Core load balancer implementation and HTTP proxy routing logic.
//!
//! This module implements the [`Router`] struct which provides HTTP/HTTPS reverse proxy
//! functionality using the Pingora framework. It handles hostname-based routing and
//! request forwarding to upstream servers.

use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use pingora_core::{Result, upstreams::peer::HttpPeer};
use pingora_load_balancing::{LoadBalancer, selection::RoundRobin};
use pingora_proxy::{ProxyHttp, Session};

/// HTTP port for the load balancer.
///
/// Defaults to 6188, can be overridden via the `PORT` environment variable.
pub static PORT: LazyLock<u16> = LazyLock::new(|| switchy_env::var_parse_or("PORT", 6188));

/// HTTPS/TLS port for the load balancer.
///
/// Defaults to 6189, can be overridden via the `SSL_PORT` environment variable.
pub static SSL_PORT: LazyLock<u16> = LazyLock::new(|| switchy_env::var_parse_or("SSL_PORT", 6189));

/// Path to the TLS certificate file.
///
/// Defaults to `/etc/pingora/ssl/tls.crt`, can be overridden via the `SSL_CRT_PATH` environment variable.
pub static SSL_CRT_PATH: LazyLock<String> =
    LazyLock::new(|| switchy_env::var_or("SSL_CRT_PATH", "/etc/pingora/ssl/tls.crt"));

/// Path to the TLS private key file.
///
/// Defaults to `/etc/pingora/ssl/tls.key`, can be overridden via the `SSL_KEY_PATH` environment variable.
pub static SSL_KEY_PATH: LazyLock<String> =
    LazyLock::new(|| switchy_env::var_or("SSL_KEY_PATH", "/etc/pingora/ssl/tls.key"));

static SNI: LazyLock<String> = LazyLock::new(|| format!("127.0.0.1:{}", *SSL_PORT));

/// HTTP proxy router that routes requests to upstream servers based on hostname.
///
/// The router maps hostnames to load balancers, which distribute requests across
/// multiple upstream servers using round-robin selection. Special handling is provided
/// for ACME challenge requests (`.well-known/acme-challenge/` paths) and a fallback
/// wildcard (`*`) hostname for unmatched hosts.
pub struct Router(BTreeMap<String, Arc<LoadBalancer<RoundRobin>>>);

impl Router {
    /// Creates a new router with the specified upstream load balancers.
    ///
    /// # Arguments
    ///
    /// * `upstreams` - Map of hostnames to their corresponding load balancers
    #[must_use]
    pub const fn new(upstreams: BTreeMap<String, Arc<LoadBalancer<RoundRobin>>>) -> Self {
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

    /// Creates a new context for the proxy session.
    ///
    /// Returns an empty unit type as no session-specific context is needed.
    fn new_ctx(&self) -> Self::CTX {}

    /// Filters incoming requests before routing.
    ///
    /// Currently performs no filtering, only logs the request path for debugging.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        let path = session.req_header().uri.path();

        log::debug!("request_filter: path={path}");

        Ok(false)
    }

    /// Selects the upstream peer for the current session.
    ///
    /// Routes requests based on hostname matching or ACME challenge paths. Falls back
    /// to the wildcard (`*`) cluster if no hostname match is found.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The request path contains invalid UTF-8
    /// * No matching cluster is found for the hostname (and no wildcard fallback exists)
    /// * The selected load balancer has no available upstream servers
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
            static NAME: &str = "solver";
            log::debug!("upstream_peer: Received challenge request");
            self.0.get(NAME).inspect(|_x| {
                log::debug!("upstream_peer: Using cluster name={NAME}");
            })
        } else {
            self.0
                .get(host)
                .inspect(|_x| {
                    log::debug!("upstream_peer: Using cluster name={host}");
                })
                .or_else(|| {
                    self.0.get("*").map_or_else(
                        || {
                            log::debug!("upstream_peer: Unsupported host={host}");
                            None
                        },
                        |fallback| {
                            log::debug!("upstream_peer: Unsupported host={host} Falling back to *");
                            Some(fallback)
                        },
                    )
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

        log::info!("upstream_peer: upstream peer is: {upstream:?}");

        Ok(Box::new(HttpPeer::new(upstream, false, SNI.to_string())))
    }

    /// Modifies the upstream request before forwarding.
    ///
    /// Sets the `Host` header to the SNI hostname for non-challenge requests.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The upstream request path contains invalid UTF-8
    ///
    /// # Panics
    ///
    /// Panics if inserting the `Host` header fails (which should not occur in normal operation).
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
