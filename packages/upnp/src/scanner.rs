use std::{pin::Pin, time::Duration};

use async_trait::async_trait;
use futures::Stream;
use rupnp::{Device, ssdp::SearchTarget};

#[async_trait]
pub trait UpnpScanner: Send + Sync {
    async fn discover(
        &self,
        search_target: &SearchTarget,
        timeout: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Device, rupnp::Error>> + Send>>, rupnp::Error>;
}

pub struct RupnpScanner;

#[async_trait]
impl UpnpScanner for RupnpScanner {
    async fn discover(
        &self,
        search_target: &SearchTarget,
        timeout: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Device, rupnp::Error>> + Send>>, rupnp::Error>
    {
        rupnp::discover(search_target, timeout, None)
            .await
            .map(|x| {
                Box::pin(x) as Pin<Box<dyn Stream<Item = Result<Device, rupnp::Error>> + Send>>
            })
    }
}

#[cfg(feature = "simulator")]
pub mod simulator {
    use std::{pin::Pin, time::Duration};

    use async_trait::async_trait;
    use futures::Stream;
    use rupnp::{Device, ssdp::SearchTarget};

    use super::UpnpScanner;

    pub struct SimulatorScanner;

    #[async_trait]
    impl UpnpScanner for SimulatorScanner {
        async fn discover(
            &self,
            _search_target: &SearchTarget,
            _timeout: Duration,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<Device, rupnp::Error>> + Send>>, rupnp::Error>
        {
            Ok(Box::pin(futures::stream::empty()))
        }
    }
}
