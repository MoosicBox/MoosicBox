#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::struct_excessive_bools)]

//! Cross-platform file watcher for monitoring filesystem changes.
//!
//! This crate provides a simple interface for watching files and directories
//! for changes using the `notify` crate. It supports filtering by event type
//! and provides both blocking and callback-based APIs.

use notify::{
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode},
};
use std::{
    path::Path,
    sync::mpsc::{RecvError, channel},
};

/// Errors that can occur during file watching operations.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    /// Error from the notify crate
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),

    /// Error receiving events from the watcher
    #[error("Channel receive error: {0}")]
    Receive(#[from] RecvError),

    /// Invalid event type specification
    #[error("Invalid event type: {0}")]
    InvalidEventType(String),
}

/// Filter for filesystem events.
///
/// This allows selecting which types of events to watch for.
#[derive(Debug, Clone, Copy, Default)]
#[must_use]
pub struct EventFilter {
    /// Watch for file modifications
    pub modify: bool,
    /// Watch for file close after write
    pub close_write: bool,
    /// Watch for file creation
    pub create: bool,
    /// Watch for file removal
    pub remove: bool,
    /// Watch for file access
    pub access: bool,
}

impl EventFilter {
    /// Creates a new empty event filter.
    pub const fn new() -> Self {
        Self {
            modify: false,
            close_write: false,
            create: false,
            remove: false,
            access: false,
        }
    }

    /// Enable watching for modify events.
    pub const fn with_modify(mut self) -> Self {
        self.modify = true;
        self
    }

    /// Enable watching for `close_write` events.
    pub const fn with_close_write(mut self) -> Self {
        self.close_write = true;
        self
    }

    /// Enable watching for create events.
    pub const fn with_create(mut self) -> Self {
        self.create = true;
        self
    }

    /// Enable watching for remove events.
    pub const fn with_remove(mut self) -> Self {
        self.remove = true;
        self
    }

    /// Enable watching for access events.
    pub const fn with_access(mut self) -> Self {
        self.access = true;
        self
    }

    /// Parse event filter from comma-separated string.
    ///
    /// # Errors
    ///
    /// * Returns `WatchError::InvalidEventType` if an unknown event type is specified
    pub fn parse(events: &str) -> Result<Self, WatchError> {
        let mut filter = Self::new();

        for event in events.split(',') {
            match event.trim() {
                "modify" => filter.modify = true,
                "close_write" => filter.close_write = true,
                "create" => filter.create = true,
                "remove" => filter.remove = true,
                "access" => filter.access = true,
                other => return Err(WatchError::InvalidEventType(other.to_string())),
            }
        }

        Ok(filter)
    }

    /// Check if an event matches this filter.
    #[must_use]
    pub const fn matches(&self, event: &notify::Event) -> bool {
        match &event.kind {
            EventKind::Modify(_) if self.modify => true,
            EventKind::Access(AccessKind::Close(AccessMode::Write)) if self.close_write => true,
            EventKind::Create(_) if self.create => true,
            EventKind::Remove(_) if self.remove => true,
            EventKind::Access(_) if self.access => true,
            _ => false,
        }
    }
}

/// Watch a directory or file for changes.
///
/// This function blocks indefinitely, calling the provided callback for each
/// matching event. To stop watching, the callback should return an error or
/// the process should be terminated.
///
/// # Arguments
///
/// * `path` - The directory or file to watch
/// * `filter` - Event filter to determine which events to report
/// * `callback` - Function to call for each matching event
///
/// # Errors
///
/// * Returns `WatchError::Notify` if the watcher cannot be created or the path cannot be watched
/// * Returns `WatchError::Receive` if there's an error receiving events from the watcher
///
/// # Examples
///
/// ```no_run
/// use moosicbox_file_watcher::{watch_directory, EventFilter};
/// use std::path::Path;
///
/// let path = Path::new("/tmp/test");
/// let filter = EventFilter::default().with_modify();
///
/// watch_directory(path, filter, |event| {
///     println!("Event: {:?}", event);
/// }).expect("Failed to watch directory");
/// ```
pub fn watch_directory<P, F>(
    path: P,
    filter: EventFilter,
    mut callback: F,
) -> Result<(), WatchError>
where
    P: AsRef<Path>,
    F: FnMut(notify::Event),
{
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())?;

    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

    log::info!("Watching: {}", path.as_ref().display());

    for res in rx {
        match res {
            Ok(event) => {
                if filter.matches(&event) {
                    callback(event);
                }
            }
            Err(e) => log::error!("Watch error: {e:?}"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_filter_parse() {
        let filter = EventFilter::parse("modify,create").unwrap();
        assert!(filter.modify);
        assert!(filter.create);
        assert!(!filter.remove);
    }

    #[test]
    fn test_event_filter_invalid() {
        let result = EventFilter::parse("modify,invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_event_filter_builder() {
        let filter = EventFilter::new().with_modify().with_create();
        assert!(filter.modify);
        assert!(filter.create);
        assert!(!filter.remove);
    }

    #[test]
    fn test_event_filter_matches() {
        let filter = EventFilter::new().with_modify();

        let modify_event = notify::Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![],
            attrs: notify::event::EventAttributes::default(),
        };

        let create_event = notify::Event {
            kind: EventKind::Create(notify::event::CreateKind::Any),
            paths: vec![],
            attrs: notify::event::EventAttributes::default(),
        };

        assert!(filter.matches(&modify_event));
        assert!(!filter.matches(&create_event));
    }
}
