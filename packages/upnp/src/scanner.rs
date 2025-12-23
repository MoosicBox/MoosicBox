//! `UPnP` device scanner implementations.
//!
//! This module provides traits and implementations for discovering `UPnP` devices on the network.
//! It supports both real network discovery via the `rupnp` library and simulated discovery
//! for testing purposes via the `simulator` feature.

use std::{pin::Pin, time::Duration};

use async_trait::async_trait;
use futures::Stream;
use rupnp::{Device, ssdp::SearchTarget};

/// Trait for discovering `UPnP` devices on the network.
///
/// Implementors of this trait provide different strategies for discovering `UPnP` devices,
/// such as using the standard `rupnp` library or simulating devices for testing.
#[async_trait]
pub trait UpnpScanner: Send + Sync {
    /// Discovers `UPnP` devices on the network matching the search target.
    ///
    /// # Errors
    ///
    /// * If the discovery operation fails due to network or protocol errors
    async fn discover(
        &self,
        search_target: &SearchTarget,
        timeout: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Device, rupnp::Error>> + Send>>, rupnp::Error>;
}

/// Real `UPnP` scanner implementation using the `rupnp` library.
///
/// This scanner performs actual network discovery of `UPnP` devices using SSDP.
#[allow(dead_code)]
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

    /// Simulated `UPnP` scanner for testing purposes.
    ///
    /// This scanner returns an empty stream of devices, allowing testing
    /// of `UPnP`-dependent code without requiring real devices on the network.
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
