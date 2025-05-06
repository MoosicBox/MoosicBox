use std::sync::{Arc, LazyLock};

use serde::Serialize;
use tauri::async_runtime::RuntimeHandle;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::TauriPlayerError;

#[derive(Debug, Clone, Serialize)]
pub struct MoosicBox {
    pub id: String,
    pub name: String,
    pub host: String,
    pub dns: String,
}

impl From<gimbal_mdns::scanner::MoosicBox> for MoosicBox {
    fn from(value: gimbal_mdns::scanner::MoosicBox) -> Self {
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
    gimbal_mdns::scanner::service::Handle,
    JoinHandle<Result<(), gimbal_mdns::scanner::service::Error>>,
) {
    let (tx, rx) = kanal::unbounded_async();

    let context = gimbal_mdns::scanner::Context::new(tx);
    let service = gimbal_mdns::scanner::service::Service::new(context);

    let handle = service.handle();
    let RuntimeHandle::Tokio(runtime_handle) = tauri::async_runtime::handle();

    moosicbox_task::spawn_on("mdns_scanner", &runtime_handle, async move {
        while let Ok(server) = rx.recv().await {
            let mut servers = MOOSICBOX_SERVERS.write().await;

            if !servers.iter().any(|x| x.dns == server.dns) {
                servers.push(server.into());
            }
        }
    });

    (handle, service.start_on(&runtime_handle))
}
