#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use mdns_sd::{ServiceDaemon, ServiceInfo};
use service::MdnsServiceDaemon;
use thiserror::Error;

#[cfg(feature = "scanner")]
pub mod scanner;

pub mod service;

pub const SERVICE_TYPE: &str = "_moosicboxserver._tcp.local.";

#[derive(Debug, Error)]
pub enum RegisterServiceError {
    #[error(transparent)]
    MdnsSd(#[from] mdns_sd::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

fn get_service_daemon() -> Result<Box<dyn MdnsServiceDaemon>, mdns_sd::Error> {
    #[cfg(feature = "simulator")]
    if moosicbox_simulator_utils::simulator_enabled() {
        return Ok(Box::new(service::simulator::SimulatorServiceDaemon));
    }
    Ok(Box::new(service::MdnsSdServiceDaemon::new(
        ServiceDaemon::new()?,
    )))
}

/// # Errors
///
/// * If `mdns_sd` has an error initializing the mdns service
/// * If there is an IO error
pub async fn register_service(
    instance_name: &str,
    ip: &str,
    port: u16,
) -> Result<(), RegisterServiceError> {
    let mdns = get_service_daemon()?;
    let host_name = format!(
        "{}.local.",
        hostname::get()?
            .into_string()
            .unwrap_or_else(|_| "unknown".to_string())
    );

    log::debug!(
        "register_service: Registering mdns service service_type={SERVICE_TYPE} instance_name={instance_name} host_name={host_name} ip={ip} port={port}"
    );

    let service_info = ServiceInfo::new(SERVICE_TYPE, instance_name, &host_name, ip, port, None)?;

    mdns.register(service_info).await?;

    log::debug!(
        "register_service: Registered mdns service service_type={SERVICE_TYPE} instance_name={instance_name} host_name={host_name} ip={ip} port={port}"
    );

    Ok(())
}
