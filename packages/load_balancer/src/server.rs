//! Server initialization and configuration for the `MoosicBox` load balancer.
//!
//! This module provides the main server entry point and configuration parsing for the
//! Pingora-based HTTP/HTTPS load balancer. It handles cluster configuration, health checks,
//! and TLS setup.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, path::Path};

use moosicbox_load_balancer::{PORT, Router, SSL_CRT_PATH, SSL_KEY_PATH, SSL_PORT};
use pingora::{listeners::tls::TlsSettings, prelude::*};
use pingora_core::services::{background::GenBackgroundService, listening::Service};
use pingora_load_balancing::{LoadBalancer, health_check::TcpHealthCheck, selection::RoundRobin};
use pingora_proxy::{HttpProxy, http_proxy_service};

/// Starts the load balancer server and runs it indefinitely.
///
/// This function initializes logging, configures the Pingora server with cluster mappings from
/// the `CLUSTERS` environment variable, sets up HTTP/HTTPS listeners, and starts health checks
/// for all upstream servers.
///
/// # Panics
///
/// Panics if:
/// * Logging initialization fails
/// * The `CLUSTERS` environment variable is not set or is malformed
/// * Server creation fails
/// * Invalid upstream IP addresses are provided in cluster configuration
/// * TLS certificate or key files cannot be read (when TLS paths are explicitly configured)
pub fn serve() {
    moosicbox_logging::init(Some("moosicbox_lb.log"), None).expect("Failed to initialize FreeLog");

    let mut pingora_server = Server::new(None).unwrap();
    pingora_server.bootstrap();

    let clusters = parse_clusters();

    let mut lb = http_proxy_service(
        &pingora_server.configuration,
        Router::new(
            clusters
                .iter()
                .map(|x| (x.0.to_owned(), x.1.task()))
                .collect::<BTreeMap<_, _>>(),
        ),
    );

    let addr = format!("0.0.0.0:{}", *PORT);
    lb.add_tcp(&addr);
    setup_tls(&mut lb);

    for service in clusters.into_values() {
        pingora_server.add_service(service);
    }

    pingora_server.add_service(lb);
    pingora_server.run_forever();
}

/// Parses cluster configuration from the `CLUSTERS` environment variable.
///
/// The `CLUSTERS` variable should contain semicolon-separated entries in the format:
/// `hostname1,hostname2:ip1:port1,ip2:port2;hostname3:ip3:port3`
///
/// Each cluster entry maps one or more hostnames to a list of upstream server addresses.
/// Health checks are automatically configured for all upstreams with a 10-second interval.
///
/// # Panics
///
/// Panics if:
/// * The `CLUSTERS` environment variable is not set
/// * Any cluster entry is malformed (missing the `:` separator)
/// * Any upstream IP address is invalid or cannot be parsed
fn parse_clusters() -> BTreeMap<String, GenBackgroundService<LoadBalancer<RoundRobin>>> {
    switchy_env::var("CLUSTERS")
        .expect("Must pass CLUSTERS environment variable")
        .split(';')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .flat_map(|x| {
            let (names, ips) = x.split_once(':').expect("Invalid cluster");
            let names = names.split(',');
            let ips = ips.split(',').collect::<Vec<_>>();

            names.map(move |x| (x.to_owned(), ips.clone()))
        })
        .map(|(name, ips)| {
            let mut upstreams = LoadBalancer::try_from_iter(&ips)
                .unwrap_or_else(|e| panic!("Invalid IPs '{ips:?}': {e:?}"));

            let hc = TcpHealthCheck::new();
            upstreams.set_health_check(hc);
            upstreams.health_check_frequency = Some(std::time::Duration::from_secs(10));

            (name, background_service("health check", upstreams))
        })
        .collect::<BTreeMap<_, _>>()
}

/// Configures TLS/HTTPS support for the load balancer service.
///
/// Checks for valid TLS certificate and key files at the paths specified by
/// `SSL_CRT_PATH` and `SSL_KEY_PATH` environment variables. If both files exist,
/// adds an HTTPS listener with HTTP/2 support enabled. Otherwise, logs warnings
/// or debug messages depending on whether the paths were explicitly configured.
///
/// # Panics
///
/// Panics if TLS settings cannot be created from valid certificate and key files.
fn setup_tls(lb: &mut Service<HttpProxy<Router>>) {
    let crt_path: &str = &SSL_CRT_PATH;
    let key_path: &str = &SSL_KEY_PATH;
    let crt_valid = Path::is_file(Path::new(crt_path));
    let key_valid = Path::is_file(Path::new(key_path));

    if crt_valid && key_valid {
        let mut tls_settings = TlsSettings::intermediate(crt_path, key_path).unwrap();
        tls_settings.enable_h2();

        let ssl_addr = format!("0.0.0.0:{}", *SSL_PORT);
        lb.add_tls_with_settings(&ssl_addr, None, tls_settings);
    } else if switchy_env::var("SSL_CRT_PATH").is_ok() || switchy_env::var("SSL_KEY_PATH").is_ok() {
        if !crt_valid {
            log::warn!("Invalid SSL_CRT_PATH");
        }
        if !key_valid {
            log::warn!("Invalid SSL_KEY_PATH");
        }
    } else if crt_valid {
        log::debug!("No key found. Not starting SSL port");
    } else if key_valid {
        log::debug!("No crt found. Not starting SSL port");
    } else {
        log::debug!("No crt or key found. Not starting SSL port");
    }
}
