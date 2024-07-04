#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{collections::HashMap, fs::create_dir_all};

use moosicbox_config::get_config_dir_path;
use moosicbox_env_utils::default_env;
use moosicbox_load_balancer::{Router, PORT, SSL_PORT};
use pingora::prelude::*;
use pingora_core::listeners::TlsSettings;
use pingora_load_balancing::{health_check::TcpHealthCheck, LoadBalancer};
use pingora_proxy::http_proxy_service;

fn main() {
    #[cfg(debug_assertions)]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=trace";
    #[cfg(not(debug_assertions))]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=info";

    let mut logs_config = free_log_client::LogsConfig::builder();

    if let Some(log_dir) = get_config_dir_path().map(|p| p.join("logs")) {
        if create_dir_all(&log_dir).is_ok() {
            logs_config = logs_config
                .with_file_writer(
                    free_log_client::FileWriterConfig::builder()
                        .file_path(log_dir.join("moosicbox_lb.log"))
                        .log_level(free_log_client::Level::Debug),
                )
                .expect("Failed to initialize file writer");
        } else {
            log::warn!("Could not create directory path for logs files at {log_dir:?}");
        }
    } else {
        log::warn!("Could not get config dir to put the logs into");
    }

    free_log_client::init(logs_config.env_filter(default_env!(
        "MOOSICBOX_LOG",
        default_env!("RUST_LOG", DEFAULT_LOG_LEVEL)
    )))
    .expect("Failed to initialize FreeLog");

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
