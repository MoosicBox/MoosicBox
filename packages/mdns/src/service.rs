//! mDNS service registration and daemon management.
//!
//! This module provides abstractions for mDNS service daemons, supporting both
//! real network operations and simulated testing scenarios.

use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Trait for mDNS service daemon implementations.
///
/// This trait provides an abstraction over different mDNS service daemon implementations,
/// allowing for both real and simulated mDNS service registration.
#[async_trait]
pub trait MdnsServiceDaemon: Send + Sync {
    /// Registers an mDNS service with the given service information.
    ///
    /// # Errors
    ///
    /// * If the underlying mDNS service daemon encounters an error during registration
    async fn register(&self, service_info: ServiceInfo) -> Result<(), mdns_sd::Error>;
}

/// Wrapper around the `mdns_sd` crate's `ServiceDaemon`.
pub struct MdnsSdServiceDaemon(ServiceDaemon);

impl MdnsSdServiceDaemon {
    /// Creates a new `MdnsSdServiceDaemon` from the given `ServiceDaemon`.
    #[must_use]
    pub const fn new(service_daemon: ServiceDaemon) -> Self {
        Self(service_daemon)
    }
}

#[async_trait]
impl MdnsServiceDaemon for MdnsSdServiceDaemon {
    async fn register(&self, service_info: ServiceInfo) -> Result<(), mdns_sd::Error> {
        self.0.register(service_info)
    }
}

#[cfg(feature = "simulator")]
/// Simulated mDNS service daemon for testing.
pub mod simulator {
    use async_trait::async_trait;
    use mdns_sd::ServiceInfo;

    use super::MdnsServiceDaemon;

    /// A simulated mDNS service daemon that performs no actual network operations.
    ///
    /// This implementation is used for testing and simulation purposes.
    pub struct SimulatorServiceDaemon;

    #[async_trait]
    impl MdnsServiceDaemon for SimulatorServiceDaemon {
        async fn register(&self, _service_info: ServiceInfo) -> Result<(), mdns_sd::Error> {
            Ok(())
        }
    }
}
