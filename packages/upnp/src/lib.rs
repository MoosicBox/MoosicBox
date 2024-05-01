#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

pub mod models;

use async_recursion::async_recursion;
use futures::prelude::*;
use models::{UpnpDevice, UpnpService};
use rupnp::{ssdp::SearchTarget, DeviceSpec, Service};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("Failed to find RenderingControl service")]
    RenderingControlNotFound,
    #[error("Failed to find MediaRenderer service")]
    MediaRendererNotFound,
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
}

pub async fn scan_service(service: &Service, path: Option<&str>) -> Result<UpnpService, ScanError> {
    let path = path.unwrap_or_default();

    log::debug!(
        "\n\
        {path}Scanning service:\n\t\
        {path}service_type={}\n\t\
        {path}service_id={}\n\t\
        ",
        service.service_type(),
        service.service_id(),
    );

    Ok(service.into())
}

#[async_recursion]
pub async fn scan_device(
    device: &DeviceSpec,
    path: Option<&str>,
) -> Result<Vec<UpnpDevice>, ScanError> {
    let path = path.unwrap_or_default();

    log::debug!(
        "\n\
        {path}Scanning device: {}\n\t\
        {path}manufacturer={}\n\t\
        {path}manufacturer_url={}\n\t\
        {path}model_name={}\n\t\
        {path}model_description={}\n\t\
        {path}model_number={}\n\t\
        {path}model_url={}\n\t\
        {path}serial_number={}\n\t\
        {path}udn={}\n\t\
        {path}upc={}\
        ",
        device.friendly_name(),
        device.manufacturer(),
        device.manufacturer_url().unwrap_or("N/A"),
        device.model_name(),
        device.model_description().unwrap_or("N/A"),
        device.model_number().unwrap_or("N/A"),
        device.model_url().unwrap_or("N/A"),
        device.serial_number().unwrap_or("N/A"),
        device.udn(),
        device.upc().unwrap_or("N/A"),
    );

    let upnp_device: UpnpDevice = device.into();
    let mut upnp_services = vec![];

    let services = device.services();

    if services.is_empty() {
        log::debug!("no services for {}", device.friendly_name());
    } else {
        let path = format!("{path}\t");
        for sub in services {
            upnp_services.push(scan_service(sub, Some(&path)).await?);
        }
    }

    let mut upnp_devices = vec![upnp_device.with_services(upnp_services)];

    let sub_devices = device.devices();

    if sub_devices.is_empty() {
        log::debug!("no sub-devices for {}", device.friendly_name());
    } else {
        let path = format!("{path}\t");
        for sub in sub_devices {
            upnp_devices.extend_from_slice(&scan_device(sub, Some(&path)).await?);
        }
    }

    Ok(upnp_devices)
}

pub async fn scan_devices() -> Result<Vec<UpnpDevice>, ScanError> {
    let search_target = SearchTarget::RootDevice;
    let devices = rupnp::discover(&search_target, Duration::from_secs(3)).await?;
    pin_utils::pin_mut!(devices);

    let mut upnp_devices = vec![];

    while let Some(device) = devices.try_next().await? {
        upnp_devices.extend_from_slice(&scan_device(&device, None).await?);
    }

    Ok(upnp_devices)
}
