//! mDNS service registration and discovery for `MoosicBox` servers.
//!
//! This crate provides functionality for:
//! * Registering `MoosicBox` servers on the local network via mDNS
//! * Discovering `MoosicBox` servers on the local network (with `scanner` feature)
//!
//! # Features
//!
//! * `scanner` - Enables mDNS service discovery for finding `MoosicBox` servers
//! * `simulator` - Provides a simulated mDNS daemon for testing purposes
//!
//! # Examples
//!
//! Registering a `MoosicBox` server:
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! switchy_mdns::register_service("my-server", "192.168.1.100", 8080).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use mdns_sd::ServiceInfo;
use service::MdnsServiceDaemon;
use thiserror::Error;

#[cfg(feature = "scanner")]
/// mDNS service scanner for discovering `MoosicBox` servers on the network.
pub mod scanner;

/// mDNS service registration and daemon management.
pub mod service;

/// The mDNS service type for `MoosicBox` servers.
pub const SERVICE_TYPE: &str = "_moosicboxserver._tcp.local.";

/// Errors that can occur when registering an mDNS service.
#[derive(Debug, Error)]
pub enum RegisterServiceError {
    /// Error from the underlying mDNS service daemon during initialization or registration.
    #[error(transparent)]
    MdnsSd(#[from] mdns_sd::Error),
    /// IO error when attempting to get the hostname.
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

/// Returns the appropriate mDNS service daemon implementation.
///
/// Returns a simulator daemon when the `simulator` feature is enabled,
/// otherwise returns a real `mdns_sd` service daemon.
///
/// # Errors
///
/// * [`mdns_sd::Error`] - If the real service daemon fails to initialize (simulator always succeeds)
#[allow(clippy::unnecessary_wraps)]
fn get_service_daemon() -> Result<Box<dyn MdnsServiceDaemon>, mdns_sd::Error> {
    #[cfg(feature = "simulator")]
    {
        Ok(Box::new(service::simulator::SimulatorServiceDaemon))
    }

    #[cfg(not(feature = "simulator"))]
    {
        Ok(Box::new(service::MdnsSdServiceDaemon::new(
            mdns_sd::ServiceDaemon::new()?,
        )))
    }
}

/// Registers an mDNS service on the local network.
///
/// This function creates and registers a `MoosicBox` server instance with the specified
/// instance name, IP address, and port number.
///
/// # Errors
///
/// * [`RegisterServiceError::MdnsSd`] - If `mdns_sd` has an error initializing the mdns service
/// * [`RegisterServiceError::IO`] - If there is an IO error when getting the hostname
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
