use std::time::Duration;

use tokio::task::JoinHandle;

use crate::{
    Error,
    runtime::{Builder, GenericRuntime},
};

#[derive(Debug)]
pub struct Runtime(tokio::runtime::Runtime);

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// # Panics
    ///
    /// * If `build_runtime` fails
    #[must_use]
    pub fn new() -> Self {
        build_runtime(&Builder::new()).unwrap()
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.0.spawn(future)
    }
}

impl GenericRuntime for Runtime {
    fn block_on<F: Future + Send + 'static>(&self, f: F) -> F::Output
    where
        F::Output: Send,
    {
        self.0.block_on(f)
    }

    /// FIXME: This doesn't await all tasks. We probably need to add all
    /// the task handles to a collection manually to handle this properly.
    fn wait(self) -> Result<(), Error> {
        self.0.shutdown_timeout(Duration::from_secs(10_000_000));
        Ok(())
    }
}

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

    Ok(Runtime(builder.build()?))
}

#[cfg(test)]
mod test {
    #[allow(unused)]
    use pretty_assertions::{assert_eq, assert_ne};
    use tokio::task;

    #[allow(unused)]
    use crate::runtime::GenericRuntime as _;
    use crate::{runtime::Builder, tokio::runtime::build_runtime};

    #[test]
    fn rt_current_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) });
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
            task::spawn(async move { assert_ne!(std::thread::current().id(), thread_id) });
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
}
