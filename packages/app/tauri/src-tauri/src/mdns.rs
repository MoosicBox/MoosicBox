use std::sync::{Arc, LazyLock};

use serde::Serialize;
use tokio::sync::RwLock;

use crate::TauriPlayerError;

#[derive(Debug, Clone, Serialize)]
pub struct MoosicBox {
    pub id: String,
    pub name: String,
    pub host: String,
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

#[tauri::command]
pub async fn fetch_moosicbox_servers() -> Result<Vec<MoosicBox>, TauriPlayerError> {
    log::debug!("fetch_moosicbox_servers");

    Ok(MOOSICBOX_SERVERS.read().await.clone())
}

pub fn spawn_mdns_scanner() -> (
    switchy::mdns::scanner::service::Handle,
    switchy::unsync::task::JoinHandle<Result<(), switchy::mdns::scanner::service::Error>>,
) {
    let (tx, rx) = kanal::unbounded_async();

    let context = switchy::mdns::scanner::Context::new(tx);
    let service = switchy::mdns::scanner::service::Service::new(context);

    let handle = service.handle();
    let runtime_handle = switchy::unsync::runtime::Handle::current();

    runtime_handle.spawn_with_name("mdns_scanner", async move {
        while let Ok(server) = rx.recv().await {
            let mut servers = MOOSICBOX_SERVERS.write().await;

            if !servers.iter().any(|x| x.dns == server.dns) {
                servers.push(server.into());
            }
        }
    });

    (handle, service.start_on(&runtime_handle))
}
