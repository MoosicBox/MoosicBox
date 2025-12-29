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
    /// Friendly name of the `UPnP` device.
    pub name: String,
    /// Unique device name (UDN) identifying the device.
    pub udn: String,
    /// Current volume level of the device, if available.
    pub volume: Option<String>,
    /// List of services provided by this device.
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
    /// Sets the volume level for this device.
    #[must_use]
    pub fn with_volume(mut self, volume: Option<String>) -> Self {
        self.volume = volume;
        self
    }

    /// Sets the services for this device.
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
    /// Service identifier (e.g., "urn:upnp-org:serviceId:AVTransport").
    pub id: String,
    /// Service type (e.g., "urn:schemas-upnp-org:service:AVTransport:1").
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_upnp_device_with_volume() {
        let device = UpnpDevice {
            name: "Test Device".to_string(),
            udn: "uuid:test-123".to_string(),
            volume: None,
            services: vec![],
        };

        let updated = device.with_volume(Some("50".to_string()));
        assert_eq!(updated.volume, Some("50".to_string()));
        assert_eq!(updated.name, "Test Device");
        assert_eq!(updated.udn, "uuid:test-123");
    }

    #[test_log::test]
    fn test_upnp_device_with_volume_none() {
        let device = UpnpDevice {
            name: "Test Device".to_string(),
            udn: "uuid:test-123".to_string(),
            volume: Some("30".to_string()),
            services: vec![],
        };

        let updated = device.with_volume(None);
        assert!(updated.volume.is_none());
    }

    #[test_log::test]
    fn test_upnp_device_with_services() {
        let device = UpnpDevice {
            name: "Test Device".to_string(),
            udn: "uuid:test-123".to_string(),
            volume: None,
            services: vec![],
        };

        let services = vec![
            UpnpService {
                id: "urn:upnp-org:serviceId:AVTransport".to_string(),
                r#type: "urn:schemas-upnp-org:service:AVTransport:1".to_string(),
            },
            UpnpService {
                id: "urn:upnp-org:serviceId:RenderingControl".to_string(),
                r#type: "urn:schemas-upnp-org:service:RenderingControl:1".to_string(),
            },
        ];

        let updated = device.with_services(services);
        assert_eq!(updated.services.len(), 2);
        assert_eq!(updated.services[0].id, "urn:upnp-org:serviceId:AVTransport");
        assert_eq!(
            updated.services[1].id,
            "urn:upnp-org:serviceId:RenderingControl"
        );
    }

    #[test_log::test]
    fn test_upnp_device_builder_pattern() {
        let device = UpnpDevice {
            name: "Test Device".to_string(),
            udn: "uuid:test-123".to_string(),
            volume: None,
            services: vec![],
        };

        let service = UpnpService {
            id: "urn:upnp-org:serviceId:AVTransport".to_string(),
            r#type: "urn:schemas-upnp-org:service:AVTransport:1".to_string(),
        };

        let updated = device
            .with_volume(Some("75".to_string()))
            .with_services(vec![service]);

        assert_eq!(updated.volume, Some("75".to_string()));
        assert_eq!(updated.services.len(), 1);
        assert_eq!(updated.services[0].id, "urn:upnp-org:serviceId:AVTransport");
    }
}
