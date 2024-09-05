use std::{
    pin::Pin,
    sync::{Arc, LazyLock},
};

use futures::Future;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::RwLock;

use crate::ScanOrigin;

#[derive(Clone, Serialize)]
pub enum ApiProgressEvent {
    Count { scanned: usize, total: usize },
    Scanned { scanned: usize, total: usize },
}

impl From<GenericProgressEvent> for ApiProgressEvent {
    fn from(value: GenericProgressEvent) -> Self {
        match value {
            GenericProgressEvent::Count { scanned, total } => Self::Count { scanned, total },
            GenericProgressEvent::Scanned { scanned, total } => Self::Scanned { scanned, total },
        }
    }
}

impl From<ProgressEvent> for Option<ApiProgressEvent> {
    fn from(value: ProgressEvent) -> Self {
        match value {
            ProgressEvent::ScanCountUpdated { scanned, total, .. } => {
                Some(ApiProgressEvent::Count { scanned, total })
            }
            ProgressEvent::ItemScanned { scanned, total, .. } => {
                Some(ApiProgressEvent::Scanned { scanned, total })
            }
            ProgressEvent::State { .. } => None,
        }
    }
}

#[derive(Clone)]
pub enum GenericProgressEvent {
    Count { scanned: usize, total: usize },
    Scanned { scanned: usize, total: usize },
}

#[derive(Clone)]
pub enum ProgressEvent {
    ScanCountUpdated {
        task: ScanTask,
        scanned: usize,
        total: usize,
    },
    ItemScanned {
        task: ScanTask,
        scanned: usize,
        total: usize,
    },
    State {
        task: ScanTask,
        state: ScanTaskState,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanTask {
    #[cfg(feature = "local")]
    #[serde(rename_all = "camelCase")]
    Local { paths: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Api { origin: ScanOrigin },
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanTaskState {
    #[default]
    Pending,
    Paused,
    Cancelled,
    Started,
    Finished,
    Error,
}

pub type ProgressListenerFut = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ProgressListener =
    Box<dyn (FnMut(GenericProgressEvent) -> ProgressListenerFut) + Send + Sync>;
pub type ProgressListenerRefFut = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ProgressListenerRef =
    Box<dyn (Fn(&ProgressEvent) -> ProgressListenerRefFut) + Send + Sync>;

pub(crate) static PROGRESS_LISTENERS: LazyLock<Arc<RwLock<Vec<Arc<ProgressListenerRef>>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(vec![])));

pub async fn add_progress_listener(listener: ProgressListenerRef) {
    PROGRESS_LISTENERS.write().await.push(Arc::new(listener));
}
