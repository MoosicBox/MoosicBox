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
    /// Registers an mDNS service using the `mdns_sd` service daemon.
    ///
    /// # Errors
    ///
    /// * [`mdns_sd::Error`] - If the service daemon encounters an error during registration
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
        /// Simulates registering an mDNS service (no-op).
        ///
        /// This implementation does nothing and always succeeds.
        ///
        /// # Errors
        ///
        /// This method never returns an error.
        async fn register(&self, _service_info: ServiceInfo) -> Result<(), mdns_sd::Error> {
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_simulator_service_daemon_register() {
            let daemon = SimulatorServiceDaemon;
            let service_info = ServiceInfo::new(
                "_moosicboxserver._tcp.local.",
                "test-instance",
                "test-host.local.",
                "192.168.1.100",
                8080,
                None,
            )
            .expect("Failed to create ServiceInfo");

            let result = daemon.register(service_info).await;
            assert!(
                result.is_ok(),
                "Simulator daemon should successfully register service"
            );
        }

        #[test_log::test(switchy_async::test)]
        async fn test_simulator_service_daemon_multiple_registrations() {
            let daemon = SimulatorServiceDaemon;

            let service1 = ServiceInfo::new(
                "_moosicboxserver._tcp.local.",
                "instance-1",
                "host1.local.",
                "192.168.1.100",
                8080,
                None,
            )
            .expect("Failed to create ServiceInfo");

            let service2 = ServiceInfo::new(
                "_moosicboxserver._tcp.local.",
                "instance-2",
                "host2.local.",
                "192.168.1.101",
                8081,
                None,
            )
            .expect("Failed to create ServiceInfo");

            let result1 = daemon.register(service1).await;
            let result2 = daemon.register(service2).await;

            assert!(result1.is_ok(), "First registration should succeed");
            assert!(result2.is_ok(), "Second registration should succeed");
        }
    }
}
