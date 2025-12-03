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

/// A single cluster configuration entry mapping hostnames to upstream addresses.
///
/// This represents the parsed result of a cluster entry from the `CLUSTERS` environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterEntry {
    /// The hostname(s) that route to this cluster
    pub hostnames: Vec<String>,
    /// The upstream IP:port addresses for this cluster
    pub upstreams: Vec<String>,
}

/// Parses cluster configuration from a configuration string.
///
/// The configuration string should contain semicolon-separated entries in the format:
/// `hostname1,hostname2:ip1:port1,ip2:port2;hostname3:ip3:port3`
///
/// Each cluster entry maps one or more hostnames to a list of upstream server addresses.
///
/// # Arguments
///
/// * `config` - The cluster configuration string to parse
///
/// # Returns
///
/// A vector of `ClusterEntry` structs, each containing hostnames and their upstream addresses.
///
/// # Panics
///
/// Panics if any cluster entry is malformed (missing the `:` separator between hostnames and IPs).
///
/// # Examples
///
/// ```
/// use moosicbox_load_balancer::parse_cluster_config;
///
/// let entries = parse_cluster_config("example.com:192.168.1.1:8080");
/// assert_eq!(entries.len(), 1);
/// assert_eq!(entries[0].hostnames, vec!["example.com"]);
/// assert_eq!(entries[0].upstreams, vec!["192.168.1.1:8080"]);
///
/// // Multiple hostnames and upstreams
/// let entries = parse_cluster_config("host1,host2:10.0.0.1:80,10.0.0.2:80");
/// assert_eq!(entries[0].hostnames, vec!["host1", "host2"]);
/// assert_eq!(entries[0].upstreams, vec!["10.0.0.1:80", "10.0.0.2:80"]);
/// ```
#[must_use]
pub fn parse_cluster_config(config: &str) -> Vec<ClusterEntry> {
    config
        .split(';')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|entry| {
            let (names, ips) = entry.split_once(':').expect("Invalid cluster");
            let hostnames = names.split(',').map(str::to_owned).collect();
            let upstreams = ips
                .split(',')
                .map(|ip| {
                    // Handle IP:port format - need to reconstruct the full address
                    // since split_once on ':' consumes the first colon
                    ip.to_owned()
                })
                .collect();

            ClusterEntry {
                hostnames,
                upstreams,
            }
        })
        .collect()
}

/// Checks if a request path is an ACME challenge request.
///
/// ACME challenge requests are used during TLS certificate issuance (e.g., Let's Encrypt)
/// and are identified by paths starting with `/.well-known/acme-challenge/`.
///
/// # Arguments
///
/// * `path` - The request path to check
///
/// # Returns
///
/// `true` if the path is an ACME challenge request, `false` otherwise.
///
/// # Examples
///
/// ```
/// use moosicbox_load_balancer::is_acme_challenge_path;
///
/// assert!(is_acme_challenge_path("/.well-known/acme-challenge/token123"));
/// assert!(!is_acme_challenge_path("/api/users"));
/// ```
#[must_use]
pub fn is_acme_challenge_path(path: &str) -> bool {
    path.starts_with("/.well-known/acme-challenge/")
}

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
        is_acme_challenge_path(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_cluster_config_tests {
        use super::*;

        #[test_log::test]
        fn parses_single_host_single_upstream() {
            let entries = parse_cluster_config("example.com:192.168.1.1:8080");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].hostnames, vec!["example.com"]);
            assert_eq!(entries[0].upstreams, vec!["192.168.1.1:8080"]);
        }

        #[test_log::test]
        fn parses_multiple_hostnames_single_upstream() {
            let entries = parse_cluster_config("host1,host2,host3:10.0.0.1:80");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].hostnames, vec!["host1", "host2", "host3"]);
            assert_eq!(entries[0].upstreams, vec!["10.0.0.1:80"]);
        }

        #[test_log::test]
        fn parses_single_hostname_multiple_upstreams() {
            let entries = parse_cluster_config("example.com:10.0.0.1:80,10.0.0.2:80,10.0.0.3:80");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].hostnames, vec!["example.com"]);
            assert_eq!(
                entries[0].upstreams,
                vec!["10.0.0.1:80", "10.0.0.2:80", "10.0.0.3:80"]
            );
        }

        #[test_log::test]
        fn parses_multiple_hostnames_multiple_upstreams() {
            let entries = parse_cluster_config("host1,host2:10.0.0.1:80,10.0.0.2:80");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].hostnames, vec!["host1", "host2"]);
            assert_eq!(entries[0].upstreams, vec!["10.0.0.1:80", "10.0.0.2:80"]);
        }

        #[test_log::test]
        fn parses_multiple_clusters() {
            let entries =
                parse_cluster_config("host1:10.0.0.1:80;host2:10.0.0.2:80;host3:10.0.0.3:80");
            assert_eq!(entries.len(), 3);

            assert_eq!(entries[0].hostnames, vec!["host1"]);
            assert_eq!(entries[0].upstreams, vec!["10.0.0.1:80"]);

            assert_eq!(entries[1].hostnames, vec!["host2"]);
            assert_eq!(entries[1].upstreams, vec!["10.0.0.2:80"]);

            assert_eq!(entries[2].hostnames, vec!["host3"]);
            assert_eq!(entries[2].upstreams, vec!["10.0.0.3:80"]);
        }

        #[test_log::test]
        fn parses_complex_configuration() {
            let entries = parse_cluster_config(
                "api.example.com,www.example.com:10.0.0.1:8080,10.0.0.2:8080;\
                 solver:10.0.1.1:80;\
                 *:10.0.2.1:80,10.0.2.2:80",
            );
            assert_eq!(entries.len(), 3);

            assert_eq!(
                entries[0].hostnames,
                vec!["api.example.com", "www.example.com"]
            );
            assert_eq!(entries[0].upstreams, vec!["10.0.0.1:8080", "10.0.0.2:8080"]);

            assert_eq!(entries[1].hostnames, vec!["solver"]);
            assert_eq!(entries[1].upstreams, vec!["10.0.1.1:80"]);

            assert_eq!(entries[2].hostnames, vec!["*"]);
            assert_eq!(entries[2].upstreams, vec!["10.0.2.1:80", "10.0.2.2:80"]);
        }

        #[test_log::test]
        fn handles_whitespace_in_entries() {
            let entries = parse_cluster_config("  host1:10.0.0.1:80  ; host2:10.0.0.2:80  ");
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].hostnames, vec!["host1"]);
            assert_eq!(entries[1].hostnames, vec!["host2"]);
        }

        #[test_log::test]
        fn filters_empty_entries() {
            let entries = parse_cluster_config("host1:10.0.0.1:80;;host2:10.0.0.2:80;");
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].hostnames, vec!["host1"]);
            assert_eq!(entries[1].hostnames, vec!["host2"]);
        }

        #[test_log::test]
        fn returns_empty_for_empty_string() {
            let entries = parse_cluster_config("");
            assert!(entries.is_empty());
        }

        #[test_log::test]
        fn returns_empty_for_whitespace_only() {
            let entries = parse_cluster_config("   ");
            assert!(entries.is_empty());
        }

        #[test_log::test]
        fn returns_empty_for_only_semicolons() {
            let entries = parse_cluster_config(";;;");
            assert!(entries.is_empty());
        }

        #[test_log::test]
        #[should_panic(expected = "Invalid cluster")]
        fn panics_on_missing_colon_separator() {
            let _ = parse_cluster_config("host1-no-colon");
        }

        #[test_log::test]
        #[should_panic(expected = "Invalid cluster")]
        fn panics_on_partial_invalid_entry() {
            let _ = parse_cluster_config("host1:10.0.0.1:80;invalid-entry;host2:10.0.0.2:80");
        }

        #[test_log::test]
        fn handles_ipv6_addresses() {
            // IPv6 addresses contain colons, so the format needs to handle them carefully
            // Since we use split_once(':'), only the first colon separates hostname from upstreams
            let entries = parse_cluster_config("host:[::1]:8080");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].hostnames, vec!["host"]);
            assert_eq!(entries[0].upstreams, vec!["[::1]:8080"]);
        }

        #[test_log::test]
        fn handles_wildcard_hostname() {
            let entries = parse_cluster_config("*:10.0.0.1:80");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].hostnames, vec!["*"]);
            assert_eq!(entries[0].upstreams, vec!["10.0.0.1:80"]);
        }
    }

    mod is_acme_challenge_path_tests {
        use super::*;

        #[test_log::test]
        fn returns_true_for_acme_challenge_path() {
            assert!(is_acme_challenge_path(
                "/.well-known/acme-challenge/token123"
            ));
        }

        #[test_log::test]
        fn returns_true_for_acme_challenge_with_long_token() {
            assert!(is_acme_challenge_path(
                "/.well-known/acme-challenge/abcdefghijklmnopqrstuvwxyz1234567890"
            ));
        }

        #[test_log::test]
        fn returns_true_for_acme_challenge_root() {
            // Just the path prefix with trailing slash
            assert!(is_acme_challenge_path("/.well-known/acme-challenge/"));
        }

        #[test_log::test]
        fn returns_false_for_regular_api_path() {
            assert!(!is_acme_challenge_path("/api/users"));
        }

        #[test_log::test]
        fn returns_false_for_root_path() {
            assert!(!is_acme_challenge_path("/"));
        }

        #[test_log::test]
        fn returns_false_for_empty_path() {
            assert!(!is_acme_challenge_path(""));
        }

        #[test_log::test]
        fn returns_false_for_similar_but_different_path() {
            assert!(!is_acme_challenge_path("/.well-known/other-path/token"));
        }

        #[test_log::test]
        fn returns_false_for_partial_acme_path() {
            // Missing trailing slash after acme-challenge
            assert!(!is_acme_challenge_path("/.well-known/acme-challenge"));
        }

        #[test_log::test]
        fn returns_false_for_path_without_leading_slash() {
            assert!(!is_acme_challenge_path(".well-known/acme-challenge/token"));
        }

        #[test_log::test]
        fn returns_false_for_nested_acme_path() {
            // The path prefix appears later in the path
            assert!(!is_acme_challenge_path(
                "/prefix/.well-known/acme-challenge/token"
            ));
        }
    }
}
