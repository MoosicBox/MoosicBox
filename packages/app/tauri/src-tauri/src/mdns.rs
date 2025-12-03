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

impl From<switchy::mdns::scanner::MoosicBox> for MoosicBox {
    fn from(value: switchy::mdns::scanner::MoosicBox) -> Self {
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
    switchy::mdns::scanner::service::Handle,
    switchy::unsync::task::JoinHandle<Result<(), switchy::mdns::scanner::service::Error>>,
) {
    let (tx, rx) = kanal::unbounded_async();

    let context = switchy::mdns::scanner::Context::new(tx);
    let service = switchy::mdns::scanner::service::Service::new(context);

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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;

    #[test_log::test]
    fn test_moosicbox_from_mdns_scanner_prepends_http_to_host() {
        let mdns_moosicbox = switchy::mdns::scanner::MoosicBox {
            id: "server-1".to_string(),
            name: "My MoosicBox".to_string(),
            host: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080),
            dns: "moosicbox.local".to_string(),
        };

        let result: MoosicBox = mdns_moosicbox.into();

        assert_eq!(result.id, "server-1");
        assert_eq!(result.name, "My MoosicBox");
        assert_eq!(result.host, "http://192.168.1.100:8080");
        assert_eq!(result.dns, "moosicbox.local");
    }

    #[test_log::test]
    fn test_moosicbox_from_mdns_scanner_with_different_port() {
        let mdns_moosicbox = switchy::mdns::scanner::MoosicBox {
            id: "abc-123".to_string(),
            name: "Production Server".to_string(),
            host: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 443),
            dns: "music.local".to_string(),
        };

        let result: MoosicBox = mdns_moosicbox.into();

        assert_eq!(result.host, "http://10.0.0.5:443");
    }

    #[test_log::test]
    fn test_moosicbox_preserves_all_fields_from_mdns_scanner() {
        let mdns_moosicbox = switchy::mdns::scanner::MoosicBox {
            id: "unique-id-456".to_string(),
            name: "Test Server Name".to_string(),
            host: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 9000),
            dns: "test.mdns.local".to_string(),
        };

        let result: MoosicBox = mdns_moosicbox.into();

        assert_eq!(result.id, "unique-id-456");
        assert_eq!(result.name, "Test Server Name");
        assert_eq!(result.dns, "test.mdns.local");
    }
}
