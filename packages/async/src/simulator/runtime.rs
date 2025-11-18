//! Simulator runtime types and builders.
//!
//! This module provides the runtime and handle types for the simulator backend,
//! which provides deterministic execution for testing async code.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use scoped_tls::scoped_thread_local;
use switchy_random::{rand::rand::seq::IteratorRandom, rng};

pub use crate::Builder;

use crate::{Error, GenericRuntime, task};

use std::cell::RefCell;
use std::collections::BTreeMap;

type LocalFutureMap = RefCell<BTreeMap<u64, Pin<Box<dyn Future<Output = ()> + 'static>>>>;

thread_local! {
    static LOCAL_FUTURES: LocalFutureMap = RefCell::new(BTreeMap::new());
}

// A Send future that references a non-Send future stored in thread-local storage
struct LocalFutureProxy {
    id: u64,
    completed: bool,
}

impl LocalFutureProxy {
    fn new<T: 'static>(
        future: impl Future<Output = T> + 'static,
        sender: futures::channel::oneshot::Sender<T>,
    ) -> Self {
        let id = TASK_ID.fetch_add(1, Ordering::SeqCst);

        let wrapped_future = async move {
            let result = future.await;
            let _ = sender.send(result);
        };

        LOCAL_FUTURES.with(|futures| {
            futures.borrow_mut().insert(id, Box::pin(wrapped_future));
        });

        Self {
            id,
            completed: false,
        }
    }
}

impl Future for LocalFutureProxy {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.completed {
            return Poll::Ready(());
        }

        LOCAL_FUTURES.with(|futures| {
            let mut futures = futures.borrow_mut();
            if let Some(future) = futures.get_mut(&self.id) {
                match future.as_mut().poll(cx) {
                    Poll::Ready(()) => {
                        futures.remove(&self.id);
                        self.completed = true;
                        Poll::Ready(())
                    }
                    Poll::Pending => Poll::Pending,
                }
            } else {
                // Future was already completed and removed
                self.completed = true;
                Poll::Ready(())
            }
        })
    }
}

impl Drop for LocalFutureProxy {
    fn drop(&mut self) {
        if !self.completed {
            LOCAL_FUTURES.with(|futures| {
                futures.borrow_mut().remove(&self.id);
            });
        }
    }
}

type Queue = Arc<Mutex<Vec<Arc<Task>>>>;

static RUNTIME_ID: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(1));
static TASK_ID: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(1));

/// A handle to a simulator runtime that provides task spawning and execution capabilities.
#[derive(Debug, Clone)]
pub struct Handle {
    runtime: Arc<Runtime>,
}

impl Handle {
    /// Runs a future to completion on the runtime.
    ///
    /// This blocks the current thread until the future completes.
    pub fn block_on<F: Future>(&self, f: F) -> F::Output {
        self.runtime.block_on(f)
    }

    /// Spawns a future onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.runtime.spawn(future)
    }

    /// Spawns a named future onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_with_name<T: Send + 'static>(
        &self,
        name: &str,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("spawn start: {name}");
            let name = name.to_owned();
            let future = async move {
                let response = future.await;
                log::trace!("spawn finished: {name}");
                response
            };
            self.runtime.spawn(future)
        } else {
            self.runtime.spawn(future)
        }
    }

    /// Spawns a blocking task onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the task's result.
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.runtime.spawn_blocking(func)
    }

    /// Spawns a named blocking task onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_blocking_with_name<F, R>(&self, name: &str, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("spawn_blocking start: {name}");
            let name = name.to_owned();
            let func = move || {
                let response = func();
                log::trace!("spawn_blocking finished: {name}");
                response
            };
            self.runtime.spawn_blocking(func)
        } else {
            self.runtime.spawn_blocking(func)
        }
    }

    /// Spawns a non-Send future onto the runtime.
    ///
    /// This allows spawning futures that are not `Send`, which must run on the current thread.
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn_local<T: 'static>(
        &self,
        future: impl Future<Output = T> + 'static,
    ) -> JoinHandle<T> {
        self.runtime.spawn_local(future)
    }

    /// Spawns a named non-Send future onto the runtime.
    ///
    /// The name is used for logging when trace-level logging is enabled.
    pub fn spawn_local_with_name<T: 'static>(
        &self,
        name: &str,
        future: impl Future<Output = T> + 'static,
    ) -> JoinHandle<T> {
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("spawn_local start: {name}");
            let name = name.to_owned();
            let future = async move {
                let response = future.await;
                log::trace!("spawn_local finished: {name}");
                response
            };
            self.runtime.spawn_local(future)
        } else {
            self.runtime.spawn_local(future)
        }
    }

    /// Returns a handle to the currently running runtime.
    ///
    /// # Panics
    ///
    /// * If no runtime is currently running
    #[must_use]
    pub fn current() -> Self {
        Runtime::current().map(|x| x.handle()).unwrap()
    }
}

scoped_thread_local! {
    static RUNTIME: Runtime
}

/// A simulator-based async runtime.
///
/// This provides a deterministic simulator runtime for testing async code with controlled
/// time advancement and reproducible behavior. Tasks are executed in a simulated environment
/// where time only advances when explicitly controlled.
#[derive(Debug, Clone)]
pub struct Runtime {
    id: u64,
    queue: Queue,
    spawner: Spawner,
    tasks: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    handle: Option<Handle>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Runtime {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl GenericRuntime for Runtime {
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        assert!(
            Self::current().is_none(),
            "Cannot run block_on within a runtime"
        );
        log::trace!("block_on");
        self.start();
        RUNTIME.set(self, || {
            let mut future = Box::pin(future);
            let waker = futures::task::noop_waker();
            let mut ctx = Context::from_waker(&waker);
            loop {
                #[allow(clippy::significant_drop_in_scrutinee)]
                match future.as_mut().poll(&mut ctx) {
                    Poll::Ready(x) => {
                        return x;
                    }
                    Poll::Pending => {
                        if !self.process_next_task() {
                            std::thread::yield_now();
                        }
                    }
                }
            }
        })
    }

    fn wait(self) -> Result<(), Error> {
        log::debug!("wait: entering, outstanding tasks={}", self.tasks());
        while self.tasks() > 0 {
            log::debug!("wait: processing task={}", self.tasks());
            if !self.process_next_task() {
                std::thread::yield_now();
            }
        }
        self.active.store(false, Ordering::SeqCst);
        log::debug!("wait: completed, all tasks finished");
        Ok(())
    }
}

impl Runtime {
    /// Creates a new simulator runtime with default settings.
    #[must_use]
    pub fn new() -> Self {
        let queue = Arc::new(Mutex::new(vec![]));

        let mut this = Self {
            id: RUNTIME_ID.fetch_add(1, Ordering::SeqCst),
            spawner: Spawner {
                queue: queue.clone(),
            },
            queue,
            tasks: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(false)),
            handle: None,
        };

        this.handle = Some(Handle {
            runtime: Arc::new(this.clone()),
        });

        this
    }

    /// # Panics
    ///
    /// * If `handle` is empty
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.handle.clone().unwrap()
    }

    fn start(&self) {
        if self.active.fetch_or(true, Ordering::SeqCst) {
            return;
        }

        assert!(!RUNTIME.is_set(), "Cannot start a Runtime within a Runtime");
    }

    fn next_task(&self) -> Option<Arc<Task>> {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        let task_count = queue.len();
        if task_count == 0 {
            log::debug!("No tasks");
            return None;
        }
        let index = queue
            .iter()
            .enumerate()
            .filter(|(_, x)| x.block)
            .map(|(i, _)| i)
            .choose(&mut rng())
            .unwrap_or_else(|| rng().gen_range(0..task_count));
        log::debug!("next task index={index} task_count={task_count}");

        Some(queue.remove(index))
    }

    pub(crate) fn process_next_task(&self) -> bool {
        let Some(task) = self.next_task() else {
            return false;
        };

        RUNTIME.set(self, || {
            task.process();
        });

        true
    }

    /// Processes the next task in the runtime's queue.
    ///
    /// This advances the simulator by one task execution. Useful for manual stepping
    /// through task execution during testing.
    pub fn tick(&self) {
        self.process_next_task();
    }

    /// Spawns a future onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.start();
        RUNTIME.set(self, || self.spawner.spawn(self.clone(), future))
    }

    /// Spawn a named future onto the runtime
    pub fn spawn_with_name<T: Send + 'static>(
        &self,
        name: &str,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        self.handle().spawn_with_name(name, future)
    }

    /// Spawns a blocking task onto the runtime.
    ///
    /// Returns a `JoinHandle` that can be awaited to get the task's result.
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.start();
        RUNTIME.set(self, || self.spawner.spawn_blocking(self.clone(), func))
    }

    /// Spawn a named blocking task onto the runtime
    pub fn spawn_blocking_with_name<F, R>(&self, name: &str, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.handle().spawn_blocking_with_name(name, func)
    }

    /// Spawns a non-Send future onto the runtime.
    ///
    /// This allows spawning futures that are not `Send`, which must run on the current thread.
    /// Returns a `JoinHandle` that can be awaited to get the future's result.
    pub fn spawn_local<T: 'static>(
        &self,
        future: impl Future<Output = T> + 'static,
    ) -> JoinHandle<T> {
        self.start();
        RUNTIME.set(self, || self.spawner.spawn_local(self.clone(), future))
    }

    fn active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn tasks(&self) -> u64 {
        self.tasks.load(Ordering::SeqCst)
    }

    /// Returns a reference to the currently running runtime.
    ///
    /// Returns `None` if no runtime is currently active on this thread.
    #[must_use]
    pub fn current() -> Option<Self> {
        if RUNTIME.is_set() {
            Some(RUNTIME.with(Clone::clone))
        } else {
            None
        }
    }
}

/// A handle to a spawned task that can be awaited for its result.
///
/// This handle allows you to wait for a task to complete and retrieve its output.
/// Dropping the handle will not cancel the task.
pub struct JoinHandle<T> {
    rx: futures::channel::oneshot::Receiver<T>,
    #[allow(clippy::option_option)]
    result: Option<Result<T, task::JoinError>>,
    finished: bool,
    #[allow(unused)]
    aborted: bool,
}

impl<T: Send + Unpin> JoinHandle<T> {
    /// Checks if the task has completed.
    ///
    /// Returns `true` if the task has finished executing, `false` otherwise.
    pub fn is_finished(&mut self) -> bool {
        if self.finished {
            return true;
        }

        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let receiver = Pin::new(&mut self.rx);
        match receiver.poll(&mut cx) {
            Poll::Ready(x) => {
                self.finished = true;
                self.result = Some(x.map_err(|_| task::JoinError::new()));
                true
            }
            Poll::Pending => false,
        }
    }

    /// Aborts the task associated with the handle.
    ///
    /// This is a no-op in the simulator runtime to maintain API compatibility with tokio.
    /// The simulator cannot abort running tasks.
    pub fn abort(&self) {
        // FIXME: We should implement this in the simulator
        // Note: In the simulator, we can't actually abort running tasks
        // This is a no-op to maintain API compatibility with tokio
        log::debug!("JoinHandle::abort() called (no-op in simulator)");
    }
}

impl<T: Send + Unpin> Future for JoinHandle<T> {
    type Output = Result<T, task::JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if let Some(result) = self.as_mut().result.take() {
            return Poll::Ready(result);
        }

        let receiver = Pin::new(&mut self.get_mut().rx);
        match receiver.poll(cx) {
            Poll::Ready(x) => Poll::Ready(x.map_err(|_| task::JoinError::new())),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Spawner {
    queue: Queue,
}

impl Spawner {
    fn spawn<T: Send + 'static>(
        &self,
        runtime: Runtime,
        future: impl Future<Output = T> + Send + 'static,
    ) -> JoinHandle<T> {
        let (tx, rx) = futures::channel::oneshot::channel();

        let wrapped = async move {
            let _ = tx.send(future.await);
        };

        self.inner_spawn(&Task::new(runtime, false, wrapped));

        JoinHandle {
            rx,
            result: None,
            finished: false,
            aborted: false,
        }
    }

    fn spawn_blocking<F, R>(&self, runtime: Runtime, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        log::trace!("spawn_blocking");
        let (tx, rx) = futures::channel::oneshot::channel();

        let wrapped = async move {
            let _ = tx.send(func());
        };

        self.inner_spawn_blocking(&Task::new(runtime, true, wrapped));

        JoinHandle {
            rx,
            result: None,
            finished: false,
            aborted: false,
        }
    }

    fn spawn_local<T: 'static>(
        &self,
        runtime: Runtime,
        future: impl Future<Output = T> + 'static,
    ) -> JoinHandle<T> {
        log::trace!("spawn_local");
        let (tx, rx) = futures::channel::oneshot::channel();

        // Create a Send proxy that references the non-Send future in thread-local storage
        let wrapped = LocalFutureProxy::new(future, tx);

        self.inner_spawn(&Task::new(runtime, false, wrapped));

        JoinHandle {
            rx,
            result: None,
            finished: false,
            aborted: false,
        }
    }

    fn inner_spawn(&self, task: &Arc<Task>) {
        log::trace!("inner_spawn");
        self.add_task(task);
    }

    fn inner_spawn_blocking(&self, task: &Arc<Task>) {
        log::trace!("inner_spawn_blocking");
        self.add_task(task);
    }

    fn add_task(&self, task: &Arc<Task>) {
        log::trace!("add_task");

        if !self.queue.lock().unwrap().iter().all(|x| x.id != task.id) {
            return;
        }
        // assert!(
        //     self.queue.lock().unwrap().iter().all(|x| x.id != task.id),
        //     "attempted to add duplicate task to queue"
        // );
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(task.clone());
    }
}

/// Spawns a future onto the current runtime.
///
/// Returns a `JoinHandle` that can be awaited to get the future's result.
///
/// # Panics
///
/// * If no runtime is currently running
pub fn spawn<T: Send + 'static>(future: impl Future<Output = T> + Send + 'static) -> JoinHandle<T> {
    RUNTIME.with(|runtime| runtime.spawn(future))
}

/// Spawns a non-Send future onto the current runtime.
///
/// This allows spawning futures that are not `Send`, which must run on the current thread.
/// Returns a `JoinHandle` that can be awaited to get the future's result.
///
/// # Panics
///
/// * If no runtime is currently running
pub fn spawn_local<T: 'static>(future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
    RUNTIME.with(|runtime| runtime.spawn_local(future))
}

/// Spawns a blocking task onto the current runtime.
///
/// Returns a `JoinHandle` that can be awaited to get the task's result.
///
/// # Panics
///
/// * If no runtime is currently running
pub fn spawn_blocking<F, R>(func: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    RUNTIME.with(|runtime| runtime.spawn_blocking(func))
}

/// Runs a future to completion on the current runtime.
///
/// This blocks the current thread until the future completes.
///
/// # Panics
///
/// * If no runtime is currently running
pub fn block_on<F: Future>(future: F) -> F::Output {
    RUNTIME.with(|runtime| runtime.block_on(future))
}

/// Waits for the current runtime to finish all pending tasks.
///
/// # Errors
///
/// * If the thread fails to join
///
/// # Panics
///
/// * If no runtime is currently running
pub fn wait() -> Result<(), Error> {
    RUNTIME.with(|runtime| runtime.clone().wait())
}

struct Task {
    id: u64,
    runtime: Runtime,
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    finished: AtomicBool,
    block: bool,
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("finished", &self.finished)
            .field("block", &self.block)
            .finish_non_exhaustive()
    }
}

impl Task {
    fn new(
        runtime: Runtime,
        block: bool,
        future: impl Future<Output = ()> + Send + 'static,
    ) -> Arc<Self> {
        runtime.tasks.fetch_add(1, Ordering::SeqCst);
        Arc::new(Self {
            id: TASK_ID.fetch_add(1, Ordering::SeqCst),
            runtime,
            future: Mutex::new(Box::pin(future)),
            finished: AtomicBool::new(false),
            block,
        })
    }

    fn waker(self: &Arc<Self>) -> Waker {
        self.clone().into()
    }

    fn poll(self: &Arc<Self>) -> Poll<()> {
        if self.finished() {
            return Poll::Ready(());
        }
        let waker = self.waker();
        let mut ctx = Context::from_waker(&waker);
        #[allow(clippy::significant_drop_in_scrutinee)]
        match self
            .future
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .poll(&mut ctx)
        {
            Poll::Ready(x) => {
                self.finished.store(true, Ordering::SeqCst);
                Poll::Ready(x)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn process(self: Arc<Self>) {
        // Execute a single scheduling step for this task.
        if !self.runtime.active() {
            return;
        }
        if self.finished() {
            return;
        }
        if self.block {
            // Blocking task: poll in a loop until ready, interleaving other tasks.
            while self.poll().is_pending() {
                if !self.runtime.process_next_task() {
                    std::thread::yield_now();
                }
            }
        } else {
            // Non-blocking task: poll once. If the future returns Pending,
            // it must register interest via its own waker to be rescheduled.
            let _ = self.poll();
        }
    }

    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| runtime.tasks.fetch_sub(1, Ordering::SeqCst));
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        log::trace!("wake");
        assert!(
            self.runtime.active(),
            "Attempted to wake on an inactive Runtime"
        );
        if self.block {
            self.runtime.spawner.inner_spawn_blocking(&self);
        } else {
            self.runtime.spawner.inner_spawn(&self);
        }
    }
}

/// Builds a new simulator runtime from the given builder.
///
/// # Errors
///
/// * This function always succeeds for the simulator runtime
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn build_runtime(_builder: &Builder) -> Result<Runtime, Error> {
    Ok(Runtime::new())
}

#[cfg(test)]
mod test {
    #[allow(unused)]
    use pretty_assertions::{assert_eq, assert_ne};

    use crate::{runtime::Builder, simulator::runtime::build_runtime, task};

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
    fn rt_spawn_local_works_with_non_send() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async move {
            use std::cell::RefCell;
            use std::rc::Rc;

            let data = Rc::new(RefCell::new(42));
            let data_clone = data.clone();

            let handle = task::spawn_local(async move {
                *data_clone.borrow_mut() += 1;
                *data_clone.borrow()
            });

            let result = handle.await.unwrap();
            assert_eq!(result, 43);
            assert_eq!(*data.borrow(), 43);
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
    fn rt_multi_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) });
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
