use std::{
    pin::Pin,
    sync::{Arc, LazyLock},
};

use futures::Future;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::RwLock;

use crate::ScanOrigin;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiScanTask {
    #[cfg(feature = "local")]
    #[serde(rename_all = "camelCase")]
    Local { paths: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Api { origin: ScanOrigin },
}

impl From<ScanTask> for ApiScanTask {
    fn from(value: ScanTask) -> Self {
        match value {
            #[cfg(feature = "local")]
            ScanTask::Local { paths } => ApiScanTask::Local { paths },
            ScanTask::Api { origin } => ApiScanTask::Api { origin },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiProgressEvent {
    #[serde(rename_all = "camelCase")]
    Finished {
        scanned: usize,
        total: usize,
        task: ApiScanTask,
    },
    #[serde(rename_all = "camelCase")]
    Count {
        scanned: usize,
        total: usize,
        task: ApiScanTask,
    },
    #[serde(rename_all = "camelCase")]
    Scanned {
        scanned: usize,
        total: usize,
        task: ApiScanTask,
    },
}

impl From<ProgressEvent> for Option<ApiProgressEvent> {
    fn from(value: ProgressEvent) -> Self {
        match value {
            ProgressEvent::ScanFinished {
                scanned,
                total,
                task,
            } => Some(ApiProgressEvent::Finished {
                scanned,
                total,
                task: task.into(),
            }),
            ProgressEvent::ScanCountUpdated {
                scanned,
                total,
                task,
            } => Some(ApiProgressEvent::Count {
                scanned,
                total,
                task: task.into(),
            }),
            ProgressEvent::ItemScanned {
                scanned,
                total,
                task,
            } => Some(ApiProgressEvent::Scanned {
                scanned,
                total,
                task: task.into(),
            }),
            ProgressEvent::State { .. } => None,
        }
    }
}

#[derive(Clone)]
pub enum ProgressEvent {
    ScanFinished {
        task: ScanTask,
        scanned: usize,
        total: usize,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub enum ScanTask {
    #[cfg(feature = "local")]
    Local {
        paths: Vec<String>,
    },
    Api {
        origin: ScanOrigin,
    },
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

pub type ProgressListenerRefFut = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ProgressListenerRef =
    Box<dyn (Fn(&ProgressEvent) -> ProgressListenerRefFut) + Send + Sync>;

pub(crate) static PROGRESS_LISTENERS: LazyLock<Arc<RwLock<Vec<Arc<ProgressListenerRef>>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(vec![])));

pub async fn add_progress_listener(listener: ProgressListenerRef) {
    PROGRESS_LISTENERS.write().await.push(Arc::new(listener));
}
