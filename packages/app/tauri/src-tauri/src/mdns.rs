//! mDNS service discovery for `MoosicBox` servers on the local network.
//!
//! This module provides functionality to discover `MoosicBox` servers using
//! mDNS/Zeroconf protocols. It automatically maintains a list of discovered servers
//! and provides a Tauri command to retrieve them.

use std::sync::{Arc, LazyLock};

use serde::Serialize;
use switchy::unsync::sync::RwLock;

use crate::TauriPlayerError;

/// Information about a discovered `MoosicBox` server on the network.
///
/// This structure is returned by mDNS service discovery and contains
/// the connection details for a `MoosicBox` server.
#[derive(Debug, Clone, Serialize)]
pub struct MoosicBox {
    /// Unique identifier for this server.
    pub id: String,
    /// Human-readable name of the server.
    pub name: String,
    /// HTTP URL for connecting to the server.
    pub host: String,
    /// DNS hostname of the server.
    pub dns: String,
}

impl From<moosicbox_mdns::scanner::MoosicBox> for MoosicBox {
    fn from(value: moosicbox_mdns::scanner::MoosicBox) -> Self {
        Self {
            id: value.id,
            name: value.name,
            host: format!("http://{}", value.host),
            dns: value.dns,
        }
    }
}

static MOOSICBOX_SERVERS: LazyLock<Arc<RwLock<Vec<MoosicBox>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(vec![])));

/// Fetches the list of `MoosicBox` servers discovered via mDNS.
///
/// This Tauri command returns all `MoosicBox` servers that have been discovered
/// on the local network through mDNS/Zeroconf service discovery.
///
/// # Errors
///
/// * Currently this function always succeeds, but returns a `Result` for
///   future extensibility and consistency with other Tauri commands.
#[tauri::command]
pub async fn fetch_moosicbox_servers() -> Result<Vec<MoosicBox>, TauriPlayerError> {
    log::debug!("fetch_moosicbox_servers");

    Ok(MOOSICBOX_SERVERS.read().await.clone())
}

/// Spawns the mDNS scanner service for discovering `MoosicBox` servers.
///
/// This function starts a background service that continuously scans for
/// `MoosicBox` servers on the local network using mDNS/Zeroconf. Discovered
/// servers are automatically added to the internal server list.
///
/// Returns a tuple containing the service handle for controlling the scanner
/// and a join handle for the scanner task.
#[must_use]
pub fn spawn_mdns_scanner(
    runtime_handle: &switchy::unsync::runtime::Handle,
) -> (
    moosicbox_mdns::scanner::service::Handle,
    switchy::unsync::task::JoinHandle<Result<(), moosicbox_mdns::scanner::service::Error>>,
) {
    let (tx, rx) = kanal::unbounded_async();

    let context = moosicbox_mdns::scanner::Context::new(tx);
    let service = moosicbox_mdns::scanner::service::Service::new(context);

    let handle = service.handle();

    runtime_handle.spawn_with_name("mdns_scanner", async move {
        while let Ok(server) = rx.recv().await {
            let mut servers = MOOSICBOX_SERVERS.write().await;

            if !servers.iter().any(|x| x.dns == server.dns) {
                servers.push(server.into());
            }
        }
    });

    (handle, service.start_on(runtime_handle))
}
