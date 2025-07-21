//! Resource daemon pattern for managing !Send resources in dedicated threads
//!
//! Based on the solution from: <https://github.com/cdellacqua/miscellaneous_libs.rs/blob/main/resource_daemon.rs/src/lib.rs>

use std::{
    fmt::Debug,
    marker::PhantomData,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum DaemonState<QuitReason> {
    Holding,
    Quitting(Option<QuitReason>),
    Quit(Option<QuitReason>),
}

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

#[derive(Debug, Clone)]
pub struct QuitSignal<QuitReason: Clone + Send + 'static>(
    Arc<(Mutex<DaemonState<QuitReason>>, Condvar)>,
);

impl<QuitReason: Clone + Send + 'static> QuitSignal<QuitReason> {
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
    /// Create a new resource daemon that manages a !Send resource in a dedicated thread
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

    /// Drop the associated resource and stops the daemon thread
    ///
    /// # Panics
    ///
    /// * If the `Mutex` guarding the state of the associated thread is poisoned
    /// * If joining the associated thread fails
    pub fn quit(&mut self, reason: QuitReason) {
        self.wake_to_quit_and_join(Some(reason));
    }

    /// Get the current state of the daemon
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
