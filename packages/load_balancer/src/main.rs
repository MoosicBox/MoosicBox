#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::fs::create_dir_all;

use moosicbox_config::get_config_dir_path;
use moosicbox_env_utils::default_env;
use moosicbox_load_balancer::LB;
use pingora::prelude::*;
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

    let ips = std::env::var("IPS").expect("Must pass IPS environment variable");
    let ips = ips.split(',').collect::<Vec<_>>();
    let mut upstreams = LoadBalancer::try_from_iter(ips).unwrap();

    let hc = TcpHealthCheck::new();
    upstreams.set_health_check(hc);
    upstreams.health_check_frequency = Some(std::time::Duration::from_secs(1));

    let background = background_service("health check", upstreams);
    let upstreams = background.task();

    let mut lb = http_proxy_service(&my_server.configuration, LB::new(upstreams));
    lb.add_tcp("0.0.0.0:6188");

    my_server.add_service(background);

    my_server.add_service(lb);
    my_server.run_forever();
}
