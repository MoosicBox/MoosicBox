use rupnp::{Device, DeviceSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpnpDevice {
    pub name: String,
    pub volume: Option<String>,
}

impl From<&DeviceSpec> for UpnpDevice {
    fn from(value: &DeviceSpec) -> Self {
        Self {
            name: value.friendly_name().to_string(),
            volume: None,
        }
    }
}

impl From<Device> for UpnpDevice {
    fn from(value: Device) -> Self {
        Self {
            name: value.friendly_name().to_string(),
            volume: None,
        }
    }
}

impl UpnpDevice {
    pub fn with_volume(mut self, volume: Option<String>) -> Self {
        self.volume = volume;
        self
    }
}
