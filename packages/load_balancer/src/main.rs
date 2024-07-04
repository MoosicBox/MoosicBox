#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::collections::HashMap;

use moosicbox_load_balancer::{Router, PORT, SSL_PORT};
use pingora::prelude::*;
use pingora_core::listeners::TlsSettings;
use pingora_load_balancing::{health_check::TcpHealthCheck, LoadBalancer};
use pingora_proxy::http_proxy_service;

fn main() {
    moosicbox_logging::init("moosicbox_lb.log").expect("Failed to initialize FreeLog");

    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    let clusters = std::env::var("CLUSTERS").expect("Must pass CLUSTERS environment variable");
    let clusters = clusters
        .split(';')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .flat_map(|x| {
            let (names, ips) = x.split_once(':').expect("Invalid cluster");
            let names = names.split(',').collect::<Vec<_>>();
            let ips = ips.split(',').collect::<Vec<_>>();

            names.into_iter().map(move |x| (x.to_owned(), ips.clone()))
        })
        .map(|(name, ips)| {
            let mut upstreams = LoadBalancer::try_from_iter(&ips)
                .unwrap_or_else(|e| panic!("Invalid IPs '{ips:?}': {e:?}"));

            let hc = TcpHealthCheck::new();
            upstreams.set_health_check(hc);
            upstreams.health_check_frequency = Some(std::time::Duration::from_secs(10));

            (name, background_service("health check", upstreams))
        })
        .collect::<HashMap<_, _>>();

    let mut lb = http_proxy_service(
        &my_server.configuration,
        Router::new(
            clusters
                .iter()
                .map(|x| (x.0.to_owned(), x.1.task()))
                .collect::<HashMap<_, _>>(),
        ),
    );

    let addr = format!("0.0.0.0:{}", *PORT);
    lb.add_tcp(&addr);

    let cert_dir = "/etc/pingora/ssl";
    let cert_path = format!("{cert_dir}/tls.crt");
    let key_path = format!("{cert_dir}/tls.key");

    let mut tls_settings = TlsSettings::intermediate(&cert_path, &key_path).unwrap();
    tls_settings.enable_h2();

    let ssl_addr = format!("0.0.0.0:{}", *SSL_PORT);
    lb.add_tls_with_settings(&ssl_addr, None, tls_settings);

    for (_, service) in clusters {
        my_server.add_service(service);
    }

    my_server.add_service(lb);
    my_server.run_forever();
}
