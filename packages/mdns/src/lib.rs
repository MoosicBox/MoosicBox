#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use mdns_sd::{ServiceDaemon, ServiceInfo};
use thiserror::Error;

#[cfg(feature = "scanner")]
pub mod scanner;

#[derive(Debug, Error)]
pub enum RegisterServiceError {
    #[error(transparent)]
    MdnsSd(#[from] mdns_sd::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub fn register_service(
    instance_name: &str,
    ip: &str,
    port: u16,
) -> Result<(), RegisterServiceError> {
    let mdns = ServiceDaemon::new()?;
    let service_type = "_moosicboxserver._tcp.local.";
    let host_name = format!(
        "{}.local.",
        hostname::get()?
            .into_string()
            .unwrap_or("unknown".to_string())
    );

    log::debug!("register_service: Registering mdns service service_type={service_type} instance_name={instance_name} host_name={host_name} ip={ip} port={port}");

    let service_info = ServiceInfo::new(service_type, instance_name, &host_name, ip, port, None)?;

    mdns.register(service_info)?;

    log::debug!("register_service: Registered mdns service service_type={service_type} instance_name={instance_name} host_name={host_name} ip={ip} port={port}");

    Ok(())
}
