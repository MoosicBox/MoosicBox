use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceInfo};

#[async_trait]
pub trait MdnsServiceDaemon: Send + Sync {
    async fn register(&self, service_info: ServiceInfo) -> Result<(), mdns_sd::Error>;
}

pub struct MdnsSdServiceDaemon(ServiceDaemon);

impl MdnsSdServiceDaemon {
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
pub mod simulator {
    use async_trait::async_trait;
    use mdns_sd::ServiceInfo;

    use super::MdnsServiceDaemon;

    pub struct SimulatorServiceDaemon;

    #[async_trait]
    impl MdnsServiceDaemon for SimulatorServiceDaemon {
        async fn register(&self, _service_info: ServiceInfo) -> Result<(), mdns_sd::Error> {
            Ok(())
        }
    }
}
