//! Progress event types and listener registration for scan operations.
//!
//! This module provides types for tracking scan progress through events,
//! including scan task definitions, progress events, and listener registration.

#![allow(clippy::module_name_repetitions)]

use std::{
    pin::Pin,
    sync::{Arc, LazyLock},
};

use futures::Future;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::RwLock;

use crate::ScanOrigin;

/// API representation of a scan task for serialization.
///
/// This enum is the JSON-serializable version of [`ScanTask`] used in API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiScanTask {
    /// Local filesystem scan with specified paths.
    #[cfg(feature = "local")]
    #[serde(rename_all = "camelCase")]
    Local {
        /// Filesystem paths to scan.
        paths: Vec<String>,
    },
    /// Remote API scan for a specific origin.
    #[serde(rename_all = "camelCase")]
    Api {
        /// The remote scan origin (e.g., Tidal, Qobuz).
        origin: ScanOrigin,
    },
}

impl From<ScanTask> for ApiScanTask {
    fn from(value: ScanTask) -> Self {
        match value {
            #[cfg(feature = "local")]
            ScanTask::Local { paths } => Self::Local { paths },
            ScanTask::Api { origin } => Self::Api { origin },
        }
    }
}

/// API representation of a progress event for serialization.
///
/// This enum is the JSON-serializable version of [`ProgressEvent`] used in API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiProgressEvent {
    /// Scan operation has finished.
    #[serde(rename_all = "camelCase")]
    Finished {
        /// Number of items scanned.
        scanned: usize,
        /// Total number of items.
        total: usize,
        /// The scan task that finished.
        task: ApiScanTask,
    },
    /// Total item count has been updated.
    #[serde(rename_all = "camelCase")]
    Count {
        /// Number of items scanned so far.
        scanned: usize,
        /// Updated total count.
        total: usize,
        /// The scan task being counted.
        task: ApiScanTask,
    },
    /// An item has been scanned.
    #[serde(rename_all = "camelCase")]
    Scanned {
        /// Number of items scanned so far.
        scanned: usize,
        /// Total number of items.
        total: usize,
        /// The scan task being performed.
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

/// Progress events emitted during scanning operations.
///
/// These events are sent to registered listeners to track scan progress.
#[derive(Clone)]
pub enum ProgressEvent {
    /// Scan operation has finished.
    ScanFinished {
        /// The scan task that finished.
        task: ScanTask,
        /// Number of items scanned.
        scanned: usize,
        /// Total number of items.
        total: usize,
    },
    /// Total item count has been updated.
    ScanCountUpdated {
        /// The scan task being counted.
        task: ScanTask,
        /// Number of items scanned so far.
        scanned: usize,
        /// Updated total count.
        total: usize,
    },
    /// An item has been scanned.
    ItemScanned {
        /// The scan task being performed.
        task: ScanTask,
        /// Number of items scanned so far.
        scanned: usize,
        /// Total number of items.
        total: usize,
    },
    /// Scan task state has changed.
    State {
        /// The scan task whose state changed.
        task: ScanTask,
        /// The new state.
        state: ScanTaskState,
    },
}

/// Represents a scan task to be executed.
///
/// A scan task defines what should be scanned and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanTask {
    /// Scan local filesystem paths.
    #[cfg(feature = "local")]
    Local {
        /// Filesystem paths to scan.
        paths: Vec<String>,
    },
    /// Scan a remote music API origin.
    Api {
        /// The remote scan origin (e.g., Tidal, Qobuz).
        origin: ScanOrigin,
    },
}

/// Current state of a scan task.
#[derive(
    Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, Copy, PartialEq, Eq, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanTaskState {
    /// Task is waiting to start.
    #[default]
    Pending,
    /// Task has been paused.
    Paused,
    /// Task has been cancelled.
    Cancelled,
    /// Task is currently running.
    Started,
    /// Task has completed successfully.
    Finished,
    /// Task encountered an error.
    Error,
}

pub type ProgressListenerRefFut = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type ProgressListenerRef =
    Box<dyn (Fn(&ProgressEvent) -> ProgressListenerRefFut) + Send + Sync>;

pub(crate) static PROGRESS_LISTENERS: LazyLock<Arc<RwLock<Vec<Arc<ProgressListenerRef>>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(vec![])));

/// Registers a listener to receive progress events during scanning.
pub async fn add_progress_listener(listener: ProgressListenerRef) {
    PROGRESS_LISTENERS.write().await.push(Arc::new(listener));
}
