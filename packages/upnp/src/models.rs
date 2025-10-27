//! Data models for `UPnP` devices and services.
//!
//! This module provides the core data structures for representing discovered `UPnP` devices
//! and their associated services.

use rupnp::{Device, DeviceSpec, Service};
use serde::{Deserialize, Serialize};

/// Represents a discovered `UPnP` device with its associated services.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpnpDevice {
    pub name: String,
    pub udn: String,
    pub volume: Option<String>,
    pub services: Vec<UpnpService>,
}

impl From<&DeviceSpec> for UpnpDevice {
    fn from(value: &DeviceSpec) -> Self {
        Self {
            name: value.friendly_name().to_string(),
            udn: value.udn().to_string(),
            volume: None,
            services: vec![],
        }
    }
}

impl From<&Device> for UpnpDevice {
    fn from(value: &Device) -> Self {
        let spec: &DeviceSpec = value;
        spec.into()
    }
}

impl UpnpDevice {
    #[must_use]
    pub fn with_volume(mut self, volume: Option<String>) -> Self {
        self.volume = volume;
        self
    }

    #[must_use]
    pub fn with_services(mut self, services: Vec<UpnpService>) -> Self {
        self.services = services;
        self
    }
}

/// Represents a `UPnP` service provided by a device.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpnpService {
    pub id: String,
    pub r#type: String,
}

impl From<&Service> for UpnpService {
    fn from(value: &Service) -> Self {
        Self {
            id: value.service_id().to_string(),
            r#type: value.service_type().to_string(),
        }
    }
}
