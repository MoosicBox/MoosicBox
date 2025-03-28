#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::HashMap, path::Path};

use moosicbox_load_balancer::{PORT, Router, SSL_CRT_PATH, SSL_KEY_PATH, SSL_PORT};
use pingora::{listeners::tls::TlsSettings, prelude::*};
use pingora_core::services::{background::GenBackgroundService, listening::Service};
use pingora_load_balancing::{LoadBalancer, health_check::TcpHealthCheck, selection::RoundRobin};
use pingora_proxy::{HttpProxy, http_proxy_service};

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
                .collect::<HashMap<_, _>>(),
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

fn parse_clusters() -> HashMap<String, GenBackgroundService<LoadBalancer<RoundRobin>>> {
    std::env::var("CLUSTERS")
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
        .collect::<HashMap<_, _>>()
}

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
    } else if std::env::var("SSL_CRT_PATH").is_ok() || std::env::var("SSL_KEY_PATH").is_ok() {
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
