//! Resource daemon pattern for managing !Send resources in dedicated threads
//!
//! Based on the solution from: <https://github.com/cdellacqua/miscellaneous_libs.rs/blob/main/resource_daemon.rs/src/lib.rs>

use std::{
    fmt::Debug,
    marker::PhantomData,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

/// The current state of a resource daemon.
///
/// Represents the lifecycle states of a [`ResourceDaemon`].
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum DaemonState<QuitReason> {
    /// The daemon is holding the resource and running normally.
    Holding,
    /// The daemon is in the process of quitting with an optional reason.
    Quitting(Option<QuitReason>),
    /// The daemon has quit with an optional reason.
    Quit(Option<QuitReason>),
}

/// A daemon that manages a !Send resource in a dedicated thread.
///
/// This allows Send+Sync wrappers around !Send resources by keeping the resource
/// confined to a single thread and providing a thread-safe interface for control.
#[derive(Debug)]
pub struct ResourceDaemon<T, QuitReason: Clone + Send + 'static> {
    phantom: PhantomData<T>,
    state: Arc<(Mutex<DaemonState<QuitReason>>, Condvar)>,
    thread_handle: Option<JoinHandle<()>>,
}

// SAFETY: the T is not held by the ResourceDaemon struct but
// rather by the thread it spawns in the constructor.
unsafe impl<T, QuitReason: Clone + Send + 'static> Send for ResourceDaemon<T, QuitReason> {}
unsafe impl<T, QuitReason: Clone + Send + 'static> Sync for ResourceDaemon<T, QuitReason> {}

/// A signal for requesting the daemon to quit.
///
/// This can be used from within the resource provider to signal that the daemon
/// should shut down, for example when an error occurs.
#[derive(Debug, Clone)]
pub struct QuitSignal<QuitReason: Clone + Send + 'static>(
    Arc<(Mutex<DaemonState<QuitReason>>, Condvar)>,
);

impl<QuitReason: Clone + Send + 'static> QuitSignal<QuitReason> {
    /// Dispatches a quit signal with the given reason.
    ///
    /// This will cause the daemon to transition to the quitting state and eventually shut down.
    pub fn dispatch(&self, reason: QuitReason) {
        wake_to_quit(&self.0, Some(reason));
    }
}

fn wake_to_quit<QuitReason: Clone + Send + 'static>(
    state: &Arc<(Mutex<DaemonState<QuitReason>>, Condvar)>,
    reason: Option<QuitReason>,
) {
    let mut guard = state.0.lock().unwrap();
    if matches!(&*guard, DaemonState::Holding) {
        *guard = DaemonState::Quitting(reason);
        state.1.notify_one();
    }
    drop(guard);
}

impl<T, QuitReason: Clone + Send + 'static> ResourceDaemon<T, QuitReason> {
    /// Creates a new resource daemon that manages a `!Send` resource in a dedicated thread.
    ///
    /// # Panics
    ///
    /// * If the thread creation fails
    /// * If the resource provider panics
    #[must_use]
    pub fn new<
        Provider: FnOnce(QuitSignal<QuitReason>) -> Result<T, QuitReason> + Send + 'static,
    >(
        resource_provider: Provider,
    ) -> Self {
        let state = Arc::new((Mutex::new(DaemonState::Holding), Condvar::default()));
        Self {
            thread_handle: Some(thread::spawn({
                let state = state.clone();
                move || {
                    let resource = resource_provider({
                        let state = state.clone();
                        QuitSignal(state)
                    });
                    match resource {
                        Err(err) => {
                            *state.0.lock().unwrap() = DaemonState::Quit(Some(err));
                        }
                        Ok(resource) => {
                            let s = state
                                .1
                                .wait_while(state.0.lock().unwrap(), |q| {
                                    matches!(q, DaemonState::Holding)
                                })
                                .unwrap();
                            // Dropping the guard before dropping the resource is necessary
                            // to prevent potential quit_signal dispatches (i.e. in a looping thread)
                            // from deadlocking on the daemon state.
                            drop(s);
                            log::debug!("ResourceDaemon: dropping resource in daemon thread");
                            drop(resource);
                            log::debug!("ResourceDaemon: resource dropped, updating state to Quit");
                            let mut s = state.0.lock().unwrap();
                            match *s {
                                DaemonState::Holding => {
                                    *s = DaemonState::Quit(None);
                                }
                                DaemonState::Quitting(ref mut reason) => {
                                    *s = DaemonState::Quit(reason.take());
                                }
                                DaemonState::Quit(_) => (),
                            }
                            log::debug!("ResourceDaemon: daemon thread exiting");
                        }
                    }
                }
            })),
            phantom: PhantomData,
            state,
        }
    }

    fn wake_to_quit_and_join(&mut self, reason: Option<QuitReason>) {
        log::debug!("ResourceDaemon: wake_to_quit_and_join called");
        wake_to_quit(&self.state, reason);
        if let Some(join_handle) = self.thread_handle.take() {
            log::debug!("ResourceDaemon: joining daemon thread...");
            let join_result = join_handle.join();
            log::debug!("ResourceDaemon: daemon thread join completed: {join_result:?}");
        } else {
            log::debug!("ResourceDaemon: no thread handle to join");
        }
    }

    /// Drops the associated resource and stops the daemon thread.
    ///
    /// # Panics
    ///
    /// * If the `Mutex` guarding the state of the associated thread is poisoned
    /// * If joining the associated thread fails
    pub fn quit(&mut self, reason: QuitReason) {
        self.wake_to_quit_and_join(Some(reason));
    }

    /// Gets the current state of the daemon.
    ///
    /// # Panics
    ///
    /// * If the `Mutex` guarding the state of the associated thread is poisoned
    #[must_use]
    pub fn state(&self) -> DaemonState<QuitReason> {
        self.state.0.lock().unwrap().clone()
    }
}

impl<T, QuitReason: Clone + Send + 'static> Drop for ResourceDaemon<T, QuitReason> {
    fn drop(&mut self) {
        log::debug!("ResourceDaemon: Drop called, shutting down daemon");
        self.wake_to_quit_and_join(None);
        log::debug!("ResourceDaemon: Drop completed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Wait for a daemon state to satisfy a predicate condition with timeout.
    ///
    /// This helper function polls the daemon state until either:
    /// - The predicate returns true, OR
    /// - The timeout is exceeded
    ///
    /// # Errors
    ///
    /// * If the timeout is exceeded before the predicate is satisfied
    fn wait_for_state<T, QuitReason: Clone + Send + Debug + 'static>(
        daemon: &ResourceDaemon<T, QuitReason>,
        predicate: impl Fn(&DaemonState<QuitReason>) -> bool,
        timeout: Duration,
    ) -> Result<DaemonState<QuitReason>, String> {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(1);

        loop {
            let state = daemon.state();
            if predicate(&state) {
                return Ok(state);
            }

            if start.elapsed() > timeout {
                return Err(format!(
                    "Timeout after {timeout:?} waiting for state condition, current state: {state:?}",
                ));
            }

            std::thread::sleep(poll_interval);
        }
    }

    /// Wait for a condition to become true with timeout.
    ///
    /// # Errors
    ///
    /// * If the timeout is exceeded before the condition becomes true
    fn wait_for_condition(
        condition: impl Fn() -> bool,
        timeout: Duration,
        description: &str,
    ) -> Result<(), String> {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(1);

        loop {
            if condition() {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(format!(
                    "Timeout after {timeout:?} waiting for: {description}",
                ));
            }

            std::thread::sleep(poll_interval);
        }
    }

    #[test_log::test]
    fn test_daemon_state_debug() {
        let state: DaemonState<String> = DaemonState::Holding;
        assert_eq!(format!("{state:?}"), "Holding");

        let state: DaemonState<String> = DaemonState::Quitting(Some("reason".to_string()));
        assert_eq!(format!("{state:?}"), "Quitting(Some(\"reason\"))");

        let state: DaemonState<String> = DaemonState::Quit(None);
        assert_eq!(format!("{state:?}"), "Quit(None)");
    }

    #[test_log::test]
    #[allow(clippy::redundant_clone)]
    fn test_daemon_state_clone() {
        let state: DaemonState<String> = DaemonState::Holding;
        let cloned = state.clone();
        assert_eq!(cloned, DaemonState::Holding);

        let state = DaemonState::Quitting(Some("test".to_string()));
        let cloned = state.clone();
        assert!(matches!(cloned, DaemonState::Quitting(Some(ref s)) if s == "test"));
    }

    #[test_log::test]
    fn test_daemon_state_equality() {
        let state1: DaemonState<String> = DaemonState::Holding;
        let state2: DaemonState<String> = DaemonState::Holding;
        assert_eq!(state1, state2);

        let state1 = DaemonState::Quitting(Some("reason".to_string()));
        let state2 = DaemonState::Quitting(Some("reason".to_string()));
        assert_eq!(state1, state2);

        let state1: DaemonState<String> = DaemonState::Quit(None);
        let state2: DaemonState<String> = DaemonState::Quit(None);
        assert_eq!(state1, state2);
    }

    #[test_log::test]
    fn test_daemon_state_ordering() {
        let holding: DaemonState<String> = DaemonState::Holding;
        let quitting: DaemonState<String> = DaemonState::Quitting(None);
        let quit: DaemonState<String> = DaemonState::Quit(None);

        assert!(holding < quitting);
        assert!(quitting < quit);
        assert!(holding < quit);
    }

    #[test_log::test]
    fn test_resource_daemon_new_success() {
        let daemon = ResourceDaemon::<i32, String>::new(|_signal| Ok(42));

        assert_eq!(daemon.state(), DaemonState::Holding);
    }

    #[test_log::test]
    fn test_resource_daemon_new_with_error() {
        let daemon = ResourceDaemon::<i32, String>::new(|_signal| Err("error".to_string()));

        // Wait for the daemon to reach Quit state
        let state = wait_for_state(
            &daemon,
            |s| matches!(s, DaemonState::Quit(_)),
            Duration::from_secs(1),
        )
        .expect("Daemon should reach Quit state");

        assert!(matches!(state, DaemonState::Quit(Some(ref s)) if s == "error"));
    }

    #[test_log::test]
    fn test_resource_daemon_quit() {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let mut daemon = ResourceDaemon::<i32, String>::new(move |_signal| {
            *counter_clone.lock().unwrap() = 1;
            Ok(42)
        });

        // Wait for resource to be created
        let counter_check = counter.clone();
        wait_for_condition(
            || *counter_check.lock().unwrap() == 1,
            Duration::from_secs(1),
            "resource creation",
        )
        .expect("Resource should be created");

        assert_eq!(*counter.lock().unwrap(), 1);

        // Quit the daemon
        daemon.quit("test reason".to_string());

        // Verify state is now Quit
        let state = daemon.state();
        assert!(matches!(state, DaemonState::Quit(Some(ref s)) if s == "test reason"));
    }

    #[test_log::test]
    #[allow(clippy::redundant_clone)]
    #[allow(clippy::items_after_statements)]
    fn test_resource_daemon_drop() {
        let dropped = Arc::new(Mutex::new(false));
        let dropped_clone = dropped.clone();

        struct DropTracker {
            dropped: Arc<Mutex<bool>>,
        }

        impl Drop for DropTracker {
            fn drop(&mut self) {
                *self.dropped.lock().unwrap() = true;
            }
        }

        {
            let _daemon = ResourceDaemon::<DropTracker, String>::new(move |_signal| {
                Ok(DropTracker {
                    dropped: dropped_clone.clone(),
                })
            });

            // Verify resource exists (not dropped yet)
            assert!(!*dropped.lock().unwrap());
        } // daemon is dropped here

        // Wait for resource to be dropped
        let dropped_check = dropped.clone();
        wait_for_condition(
            || *dropped_check.lock().unwrap(),
            Duration::from_secs(1),
            "resource to be dropped",
        )
        .expect("Resource should be dropped");

        assert!(*dropped.lock().unwrap());
    }

    #[test_log::test]
    fn test_quit_signal_dispatch() {
        let daemon = ResourceDaemon::<i32, String>::new(|signal| {
            // Dispatch quit signal from within the provider
            std::thread::spawn(move || {
                signal.dispatch("internal quit".to_string());
            });
            Ok(42)
        });

        // Wait for signal to be dispatched and daemon to transition
        let state = wait_for_state(
            &daemon,
            |s| matches!(s, DaemonState::Quitting(_) | DaemonState::Quit(_)),
            Duration::from_secs(1),
        )
        .expect("Daemon should reach Quitting or Quit state");

        assert!(
            matches!(state, DaemonState::Quitting(_) | DaemonState::Quit(_)),
            "Expected 'Quitting' or 'Quit' state, but got {state:?}"
        );
    }

    #[test_log::test]
    fn test_quit_signal_debug() {
        let daemon = ResourceDaemon::<i32, String>::new(|signal| {
            let debug_str = format!("{signal:?}");
            assert!(debug_str.contains("QuitSignal"));
            Ok(42)
        });

        // The daemon should be in Holding state (no wait needed for synchronous check)
        assert_eq!(daemon.state(), DaemonState::Holding);
    }

    #[test_log::test]
    #[allow(clippy::redundant_clone)]
    fn test_quit_signal_clone() {
        let daemon = ResourceDaemon::<i32, String>::new(|signal| {
            let signal_clone = signal.clone();

            // Both signals should work
            std::thread::spawn(move || {
                signal_clone.dispatch("cloned signal".to_string());
            });

            Ok(42)
        });

        // Wait for signal to be dispatched
        let state = wait_for_state(
            &daemon,
            |s| matches!(s, DaemonState::Quitting(_) | DaemonState::Quit(_)),
            Duration::from_secs(1),
        )
        .expect("Daemon should reach Quitting or Quit state");

        assert!(
            matches!(state, DaemonState::Quitting(_) | DaemonState::Quit(_)),
            "Expected 'Quitting' or 'Quit' state, but got {state:?}"
        );
    }

    #[test_log::test]
    #[allow(clippy::redundant_clone)]
    fn test_resource_daemon_state_transitions() {
        let state_log = Arc::new(Mutex::new(Vec::new()));
        let state_log_clone = state_log.clone();

        let mut daemon = ResourceDaemon::<i32, String>::new(move |_signal| {
            state_log_clone.lock().unwrap().push("created".to_string());
            Ok(42)
        });

        // Wait for resource creation to complete
        let state_log_check = state_log.clone();
        wait_for_condition(
            || {
                state_log_check
                    .lock()
                    .unwrap()
                    .contains(&"created".to_string())
            },
            Duration::from_secs(1),
            "resource creation",
        )
        .expect("Resource should be created");

        // Initial state should be Holding
        assert_eq!(daemon.state(), DaemonState::Holding);

        // Quit the daemon
        daemon.quit("test".to_string());

        // State should transition to Quit
        let state = daemon.state();
        assert!(matches!(state, DaemonState::Quit(_)));
    }

    #[test_log::test]
    fn test_multiple_quit_calls() {
        let mut daemon = ResourceDaemon::<i32, String>::new(|_signal| Ok(42));

        // Daemon should start in Holding state
        assert_eq!(daemon.state(), DaemonState::Holding);

        // First quit
        daemon.quit("first".to_string());
        let state1 = daemon.state();

        // Second quit should be a no-op
        daemon.quit("second".to_string());
        let state2 = daemon.state();

        // State should remain the same (both should be Quit with "first")
        assert_eq!(state1, state2);
        assert!(matches!(state1, DaemonState::Quit(Some(ref s)) if s == "first"));
    }

    #[test_log::test]
    fn test_resource_daemon_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<ResourceDaemon<i32, String>>();
        assert_sync::<ResourceDaemon<i32, String>>();
    }

    #[test_log::test]
    fn test_resource_daemon_debug() {
        let daemon = ResourceDaemon::<i32, String>::new(|_signal| Ok(42));

        let debug_str = format!("{daemon:?}");
        assert!(debug_str.contains("ResourceDaemon"));
    }
}
