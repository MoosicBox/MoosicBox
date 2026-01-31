//! Tokio runtime types and builders.
//!
//! This module provides the runtime and handle types for the Tokio backend,
//! including builders for configuring and creating runtimes.

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use tokio::task::JoinHandle;

use crate::{Error, GenericRuntime};

pub use crate::Builder;

/// A handle to a tokio runtime that provides task spawning and execution capabilities.
///
/// This wrapper around tokio's Handle maintains a weak reference to the parent runtime
/// to enable proper I/O and timer driver access, particularly for signal handling.
#[derive(Debug, Clone)]
pub struct Handle {
    inner: tokio::runtime::Handle,
    // Keep a weak reference to prevent circular dependencies
    runtime: Weak<tokio::runtime::Runtime>,
}

impl Handle {
    /// Creates a new Handle from an Arc-wrapped runtime.
    fn new(runtime: &Arc<tokio::runtime::Runtime>) -> Self {
        Self {
            inner: runtime.handle().clone(),
            runtime: Arc::downgrade(runtime),
        }
    }

    /// Blocks on a future using the parent Runtime instead of Handle.
    ///
    /// This ensures proper I/O and timer driver access for signal handling.
    /// If the runtime has been dropped, falls back to the inner handle's `block_on`.
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        if let Some(runtime) = self.runtime.upgrade() {
            // Use Runtime::block_on which can drive IO/timers
            runtime.block_on(future)
        } else {
            // Fallback to Handle::block_on if runtime is dropped
            // This maintains existing behavior for edge cases
            self.inner.block_on(future)
        }
    }

    /// Spawns a future onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.inner.spawn(future)
    }

    /// Spawns a named future onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_with_name<T: Send + 'static>(
        &self,
        name: &str,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("spawn start: {name}");
            let name = name.to_owned();
            let future = async move {
                let response = future.await;
                log::trace!("spawn finished: {name}");
                response
            };
            self.inner.spawn(future)
        } else {
            self.inner.spawn(future)
        }
    }

    /// Spawns a blocking task onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the task's result.
    pub fn spawn_blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> JoinHandle<T> {
        self.inner.spawn_blocking(f)
    }

    /// Spawns a named blocking task onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_blocking_with_name<T: Send + 'static>(
        &self,
        name: &str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> JoinHandle<T> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("spawn_blocking start: {name}");
            let name = name.to_owned();
            let f = move || {
                let response = f();
                log::trace!("spawn_blocking finished: {name}");
                response
            };
            self.inner.spawn_blocking(f)
        } else {
            self.inner.spawn_blocking(f)
        }
    }

    /// Spawns a non-Send future onto the current thread.
    ///
    /// This allows spawning futures that are not `Send`, which must run on the current thread.
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn_local<T: 'static>(
        &self,
        future: impl std::future::Future<Output = T> + 'static,
    ) -> JoinHandle<T> {
        tokio::task::spawn_local(future)
    }

    /// Spawns a named non-Send future onto the current thread.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_local_with_name<T: 'static>(
        &self,
        name: &str,
        future: impl std::future::Future<Output = T> + 'static,
    ) -> JoinHandle<T> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("spawn_local start: {name}");
            let name = name.to_owned();
            let future = async move {
                let response = future.await;
                log::trace!("spawn_local finished: {name}");
                response
            };
            tokio::task::spawn_local(future)
        } else {
            tokio::task::spawn_local(future)
        }
    }

    /// Returns a handle to the currently running runtime.
    ///
    /// # Panics
    ///
    /// Panics if no runtime is currently running on this thread.
    #[must_use]
    pub fn current() -> Self {
        // We can't easily get the Runtime reference from a static context,
        // so we create a Handle that will fall back to tokio::runtime::Handle::block_on
        Self {
            inner: tokio::runtime::Handle::current(),
            runtime: Weak::new(), // Empty weak reference - will always fall back
        }
    }

    /// Try to get the current runtime handle
    ///
    /// # Errors
    ///
    /// * If no runtime is currently running on this thread
    pub fn try_current() -> Result<Self, tokio::runtime::TryCurrentError> {
        tokio::runtime::Handle::try_current().map(|inner| Self {
            inner,
            runtime: Weak::new(),
        })
    }
}

/// A tokio-based async runtime.
///
/// This provides a wrapper around tokio's Runtime with additional utilities for task spawning,
/// blocking execution, and graceful shutdown.
#[derive(Debug)]
pub struct Runtime {
    inner: Arc<tokio::runtime::Runtime>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Creates a new runtime with default settings.
    ///
    /// # Panics
    ///
    /// * If `build_runtime` fails
    #[must_use]
    pub fn new() -> Self {
        build_runtime(&Builder::new()).unwrap()
    }

    /// Spawns a future onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.inner.spawn(future)
    }

    /// Spawns a named future onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_with_name<T: Send + 'static>(
        &self,
        name: &str,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.handle().spawn_with_name(name, future)
    }

    /// Spawns a blocking task onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the task's result.
    pub fn spawn_blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> JoinHandle<T> {
        self.inner.spawn_blocking(f)
    }

    /// Spawns a named blocking task onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_blocking_with_name<T: Send + 'static>(
        &self,
        name: &str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> JoinHandle<T> {
        self.handle().spawn_blocking_with_name(name, f)
    }

    /// Returns a handle to this runtime.
    #[must_use]
    pub fn handle(&self) -> Handle {
        Handle::new(&self.inner)
    }
}

impl GenericRuntime for Runtime {
    fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.inner.block_on(future)
    }

    /// FIXME: This doesn't await all tasks. We probably need to add all
    /// the task handles to a collection manually to handle this properly.
    fn wait(self) -> Result<(), Error> {
        // Extract the Arc and wait for all references to drop
        Arc::try_unwrap(self.inner).map_or_else(
            |_| {
                // Other references exist, cannot cleanly shutdown
                log::warn!("Runtime has outstanding references, forcing shutdown");
                Ok(())
            },
            |runtime| {
                runtime.shutdown_timeout(Duration::from_secs(10_000_000));
                Ok(())
            },
        )
    }
}

/// Builds a new tokio runtime from the given builder.
///
/// # Errors
///
/// * If the underlying tokio runtime fails to build
#[allow(unused)]
pub(crate) fn build_runtime(#[allow(unused)] builder: &Builder) -> Result<Runtime, Error> {
    #[cfg(feature = "rt-multi-thread")]
    #[allow(clippy::option_if_let_else)]
    let mut builder = if let Some(threads) = builder.max_blocking_threads {
        let mut builder = tokio::runtime::Builder::new_multi_thread();

        builder.max_blocking_threads(threads as usize);

        builder
    } else {
        tokio::runtime::Builder::new_current_thread()
    };
    #[cfg(not(feature = "rt-multi-thread"))]
    let mut builder = tokio::runtime::Builder::new_current_thread();

    #[cfg(feature = "time")]
    builder.enable_time();

    #[cfg(feature = "net")]
    builder.enable_io();

    Ok(Runtime {
        inner: Arc::new(builder.build()?),
    })
}

#[cfg(test)]
mod test {
    #[allow(unused)]
    use pretty_assertions::{assert_eq, assert_ne};
    use tokio::task;

    #[allow(unused)]
    use crate::GenericRuntime as _;
    use crate::{Builder, tokio::runtime::build_runtime};

    #[test]
    fn rt_current_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) })
                .await
                .unwrap();
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn rt_current_thread_runtime_block_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            assert_eq!(std::thread::current().id(), thread_id);
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test]
    fn rt_multi_thread_runtime_spawns_new_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_ne!(std::thread::current().id(), thread_id) })
                .await
                .unwrap();
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test]
    fn rt_multi_thread_runtime_block_on_same_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            assert_eq!(std::thread::current().id(), thread_id);
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn handle_block_on_with_signals() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let handle = runtime.handle();

        // Test that Handle::block_on can handle signal-like operations
        let result = handle.block_on(async {
            #[cfg(feature = "time")]
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            "success"
        });

        assert_eq!(result, "success");
        runtime.wait().unwrap();
    }

    #[test]
    fn handle_survives_runtime_drop() {
        let handle = {
            let runtime = build_runtime(&Builder::new()).unwrap();
            runtime.handle()
        };

        // Handle should still work even after runtime is dropped
        // (though it will fall back to inner Handle::block_on)
        let result = handle.block_on(async { "fallback" });
        assert_eq!(result, "fallback");
    }

    #[test]
    fn handle_delegates_to_runtime_block_on() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let handle = runtime.handle();

        // Verify that Handle::block_on works the same as Runtime::block_on
        let runtime_result = runtime.block_on(async { 42 });
        let handle_result = handle.block_on(async { 42 });

        assert_eq!(runtime_result, handle_result);
        runtime.wait().unwrap();
    }

    #[test]
    fn handle_current_returns_custom_handle() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            // Test that Handle::current() returns our custom Handle type
            let current_handle = super::Handle::current();
            // Just verify we can get the handle and it has the right type
            // We can't test block_on from within a runtime context
            let _spawned = current_handle.spawn(async { "spawned_works" });
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn handle_try_current_returns_error_outside_runtime() {
        // Outside of any runtime context, try_current should fail
        let result = super::Handle::try_current();
        assert!(result.is_err());
    }

    #[test]
    fn handle_try_current_returns_ok_inside_runtime() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            // Inside runtime context, try_current should succeed
            let result = super::Handle::try_current();
            assert!(result.is_ok());

            let handle = result.unwrap();
            // Verify we can use the handle
            let join_handle = handle.spawn(async { 42 });
            let value = join_handle.await.unwrap();
            assert_eq!(value, 42);
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn runtime_default_creates_working_runtime() {
        // Test that Default trait works correctly
        let runtime = super::Runtime::default();

        let result = runtime.block_on(async { "hello" });
        assert_eq!(result, "hello");

        runtime.wait().unwrap();
    }

    #[test]
    fn handle_spawn_local_works_with_non_send() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            use std::cell::RefCell;
            use std::rc::Rc;

            // Create a local task set to run spawn_local
            let local = tokio::task::LocalSet::new();

            local
                .run_until(async {
                    // Rc is not Send
                    let data = Rc::new(RefCell::new(42));
                    let data_clone = data.clone();

                    let handle = super::Handle::current();
                    let join_handle = handle.spawn_local(async move {
                        *data_clone.borrow_mut() += 1;
                        *data_clone.borrow()
                    });

                    let result = join_handle.await.unwrap();
                    assert_eq!(result, 43);
                    assert_eq!(*data.borrow(), 43);
                })
                .await;
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn handle_spawn_local_with_name_works() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            let local = tokio::task::LocalSet::new();

            local
                .run_until(async {
                    let handle = super::Handle::current();
                    let join_handle = handle.spawn_local_with_name("test_task", async { 99 });

                    let result = join_handle.await.unwrap();
                    assert_eq!(result, 99);
                })
                .await;
        });

        runtime.wait().unwrap();
    }

    #[test]
    fn runtime_spawn_with_name_works() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let join_handle = runtime.spawn_with_name("named_task", async { "named_result" });

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, "named_result");
        runtime.wait().unwrap();
    }

    #[test]
    fn runtime_spawn_blocking_with_name_works() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let join_handle = runtime.spawn_blocking_with_name("blocking_task", || {
            // Simulate some blocking work
            std::thread::sleep(std::time::Duration::from_millis(1));
            "blocking_result"
        });

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, "blocking_result");
        runtime.wait().unwrap();
    }

    #[test]
    fn handle_spawn_with_name_works() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let handle = runtime.handle();

        let join_handle = handle.spawn_with_name("handle_named_task", async { 123 });

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, 123);
        runtime.wait().unwrap();
    }

    #[test]
    fn handle_spawn_blocking_with_name_works() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let handle = runtime.handle();

        let join_handle =
            handle.spawn_blocking_with_name("handle_blocking_task", || "handle_blocking_result");

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, "handle_blocking_result");
        runtime.wait().unwrap();
    }
}
