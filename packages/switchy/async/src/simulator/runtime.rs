//! Simulator runtime types and builders.
//!
//! This module provides the runtime and handle types for the simulator backend,
//! which provides deterministic execution for testing async code.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, LazyLock, Mutex, PoisonError, Weak,
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

/// A Send-safe proxy for non-Send futures stored in thread-local storage.
///
/// This allows spawning non-Send futures on the simulator runtime by storing the actual
/// future in thread-local storage and providing a Send wrapper that can be moved between
/// threads safely. The actual future execution always happens on the original thread.
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

        // User polling and destruction can spawn local work. Never hold the registry
        // borrow across either operation.
        let future = LOCAL_FUTURES.with(|futures| futures.borrow_mut().remove(&self.id));
        let Some(mut future) = future else {
            self.completed = true;
            return Poll::Ready(());
        };
        match future.as_mut().poll(cx) {
            Poll::Ready(()) => {
                self.completed = true;
                Poll::Ready(())
            }
            Poll::Pending => {
                LOCAL_FUTURES.with(|futures| {
                    futures.borrow_mut().insert(self.id, future);
                });
                Poll::Pending
            }
        }
    }
}

impl Drop for LocalFutureProxy {
    fn drop(&mut self) {
        if !self.completed {
            let future = LOCAL_FUTURES.with(|futures| futures.borrow_mut().remove(&self.id));
            drop(future);
        }
    }
}

type Queue = Arc<Mutex<Vec<Arc<Task>>>>;

/// Every unfinished task of a runtime, by id, queued or not.
type Registry = Arc<Mutex<BTreeMap<u64, Weak<Task>>>>;

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
    /// This blocks the current thread until the future completes. Called from within this
    /// runtime (in a task, typically through `task::block_in_place`), it drives the future
    /// together with the runtime's other tasks on the calling thread, as a multi-thread tokio
    /// runtime does.
    pub fn block_on<F: Future>(&self, f: F) -> F::Output {
        if Runtime::current().is_some_and(|current| current.id == self.runtime.id) {
            return self.runtime.block_on_nested(f);
        }
        self.runtime.block_on(f)
    }

    /// Make this runtime current on the calling thread (for `Handle::current` and spawning)
    /// until the guard drops, as tokio's `Handle::enter` does.
    #[must_use]
    pub fn enter(&self) -> EnterGuard {
        ENTERED.with_borrow_mut(|entered| entered.push((*self.runtime).clone()));
        EnterGuard {
            _not_send: std::marker::PhantomData,
        }
    }

    /// How the runtime runs its tasks: [`crate::RuntimeFlavor::MultiThread`], since tasks
    /// may block in place and nest [`Self::block_on`] (see there).
    #[must_use]
    pub const fn runtime_flavor(&self) -> crate::RuntimeFlavor {
        crate::RuntimeFlavor::MultiThread
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

    /// Try to obtain the runtime active on this thread without panicking.
    ///
    /// # Errors
    ///
    /// * Returns [`TryCurrentError`] when no simulator runtime is active on this thread.
    pub fn try_current() -> Result<Self, TryCurrentError> {
        Runtime::current()
            .map(|runtime| runtime.handle())
            .ok_or(TryCurrentError)
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

/// No simulator runtime is active on the calling thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no simulator runtime is active on this thread")]
pub struct TryCurrentError;

scoped_thread_local! {
    static RUNTIME: Runtime
}

thread_local! {
    /// Runtimes entered on this thread with [`Handle::enter`], innermost last.
    static ENTERED: RefCell<Vec<Runtime>> = const { RefCell::new(Vec::new()) };
}

/// Makes a runtime current on this thread until dropped; see [`Handle::enter`].
#[derive(Debug)]
pub struct EnterGuard {
    // Entered on, and left on, one thread.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        ENTERED.with_borrow_mut(Vec::pop);
    }
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
    /// Set by [`Runtime::shutdown_now`]: the runtime never runs again.
    shut_down: Arc<AtomicBool>,
    registry: Registry,
    /// The `switchy_thread` group of the simulated threads its tasks start.
    #[cfg(feature = "thread-simulator")]
    thread_group: u64,
    /// Its number of workers, from `Builder::worker_threads`; 0 for the run's.
    #[cfg(feature = "thread-simulator")]
    worker_count: Arc<std::sync::atomic::AtomicUsize>,
    /// Its workers, started by the first step that needs them (in a simulation run).
    #[cfg(feature = "thread-simulator")]
    workers: Arc<std::sync::OnceLock<Arc<super::workers::Pool>>>,
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
            let waker = self.blocking_waker();
            let mut ctx = Context::from_waker(&waker);
            loop {
                #[allow(clippy::significant_drop_in_scrutinee)]
                match future.as_mut().poll(&mut ctx) {
                    Poll::Ready(x) => {
                        return x;
                    }
                    Poll::Pending => {
                        if !self.process_next_task() {
                            self.idle();
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
                self.idle();
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
            shut_down: Arc::new(AtomicBool::new(false)),
            registry: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(feature = "thread-simulator")]
            thread_group: switchy_thread::simulator::new_key(),
            #[cfg(feature = "thread-simulator")]
            worker_count: Arc::default(),
            #[cfg(feature = "thread-simulator")]
            workers: Arc::default(),
            handle: None,
        };

        this.handle = Some(Handle {
            runtime: Arc::new(this.clone()),
        });

        this
    }

    /// Returns a handle to this runtime.
    ///
    /// The handle can be used to spawn tasks onto the runtime from other threads.
    #[must_use]
    pub fn handle(&self) -> Handle {
        // The runtime a handle holds is a copy without a handle of its own.
        self.handle.clone().unwrap_or_else(|| Handle {
            runtime: Arc::new(self.clone()),
        })
    }

    /// Mark the runtime active (its tasks' wakers reschedule them). Spawning onto it from
    /// within another runtime's task is fine, as with tokio; only `block_on` refuses to nest.
    fn start(&self) {
        if self.shut_down.load(Ordering::SeqCst) {
            return;
        }
        self.active.store(true, Ordering::SeqCst);
    }

    fn next_task(&self) -> Option<Arc<Task>> {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        let task_count = queue.len();
        if task_count == 0 {
            log::debug!("No tasks");
            return None;
        }
        // A task being polled further up this thread's stack (one that nests `block_on`) waits
        // its turn until that poll returns.
        let eligible: Vec<usize> = (0..task_count)
            .filter(|&i| !queue[i].polling.load(Ordering::SeqCst))
            .collect();
        if eligible.is_empty() {
            return None;
        }
        let index = eligible
            .iter()
            .copied()
            .filter(|&i| queue[i].block)
            .choose(&mut rng())
            .unwrap_or_else(|| eligible[rng().gen_range(0..eligible.len())]);
        log::debug!("next task index={index} task_count={task_count}");

        Some(queue.remove(index))
    }

    /// [`Handle::block_on`] from within this runtime: drive `future` and the runtime's other
    /// tasks on the calling thread until it completes.
    fn block_on_nested<F: Future>(&self, future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let waker = self.blocking_waker();
        let mut ctx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut ctx) {
                return output;
            }
            if !self.process_next_task() {
                self.idle();
            }
        }
    }

    /// The waker of a future [`Self::block_on`] drives itself: waking it wakes this executor if
    /// it waits for something to do, so a future completed from another thread (a channel's
    /// sender, say) is polled again even when no task of this runtime runs.
    fn blocking_waker(&self) -> Waker {
        struct Blocking(Runtime);
        impl Wake for Blocking {
            fn wake(self: Arc<Self>) {
                self.0.notify();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.notify();
            }
        }
        Waker::from(Arc::new(Blocking(Self {
            handle: None,
            ..self.clone()
        })))
    }

    /// Wait for something to do, having nothing to run.
    ///
    /// Within a simulation run with other simulated threads alive (`thread-simulator`), this
    /// blocks on the run's scheduler until a task's waker wakes this executor: the threads run
    /// meanwhile, and if all of them wait, the run's clock moves to their earliest deadline.
    /// Otherwise it yields the OS thread.
    // `self` is used with `thread-simulator` only.
    #[cfg_attr(not(feature = "thread-simulator"), allow(clippy::unused_self))]
    fn idle(&self) {
        #[cfg(feature = "thread-simulator")]
        if let Some(scheduler) = switchy_thread::simulator::try_scheduler()
            && scheduler.others_alive()
        {
            let _ = scheduler.block(
                switchy_thread::simulator::Op::Executor,
                switchy_thread::simulator::WaitKey::Executor(self.id),
                None,
            );
            return;
        }
        std::thread::yield_now();
    }

    /// Wake this executor if it waits for a task (see [`Self::idle`]).
    #[cfg_attr(
        not(feature = "thread-simulator"),
        allow(clippy::unused_self, clippy::missing_const_for_fn)
    )]
    fn notify(&self) {
        #[cfg(feature = "thread-simulator")]
        if let Some(scheduler) = switchy_thread::simulator::try_scheduler() {
            scheduler.wake_all(switchy_thread::simulator::WaitKey::Executor(self.id));
        }
    }

    pub(crate) fn process_next_task(&self) -> bool {
        #[cfg(feature = "thread-simulator")]
        if let Some(processed) = self.process_batch() {
            return processed;
        }
        let Some(task) = self.next_task() else {
            return false;
        };
        self.process_on_this_thread(task);
        true
    }

    /// Run one step of `task` on the calling thread, with this runtime current.
    pub(crate) fn process_on_this_thread(&self, task: Arc<Task>) {
        RUNTIME.set(self, || {
            self.in_thread_group(|| task.process());
        });
    }

    /// The `switchy_thread` group of the simulated threads its tasks (and workers) start.
    #[cfg(feature = "thread-simulator")]
    pub(crate) const fn thread_group(&self) -> u64 {
        self.thread_group
    }

    /// With several workers, poll up to that many ready tasks at once (see `workers`), and
    /// then the tasks that must stay on this thread. `None` when this step polls one task on
    /// this thread, as with one executor: one worker, within such a step already, or outside
    /// a simulation run.
    #[cfg(feature = "thread-simulator")]
    fn process_batch(&self) -> Option<bool> {
        if super::workers::in_step() || switchy_thread::simulator::try_scheduler().is_none() {
            return None;
        }
        let wanted = match self.worker_count.load(Ordering::SeqCst) {
            0 => super::workers::run_workers(),
            count => count,
        };
        if wanted <= 1 {
            return None;
        }
        let (mut batch, mut here) = (Vec::new(), Vec::new());
        while batch.len() < wanted {
            let Some(task) = self.next_task() else {
                break;
            };
            // A local task's future lives on this thread; a blocking one drives the runtime
            // itself.
            if task.local.load(Ordering::SeqCst) || task.block {
                here.push(task);
            } else {
                batch.push(task);
            }
        }
        if batch.is_empty() && here.is_empty() {
            return Some(false);
        }
        if batch.len() == 1 {
            here.insert(0, batch.remove(0));
        } else if !batch.is_empty() {
            let pool = self.workers.get_or_init(Arc::default);
            pool.run_batch(self, batch);
        }
        for task in here {
            self.process_on_this_thread(task);
        }
        Some(true)
    }

    /// Run `f` with the simulated threads it starts in this runtime's thread group, so
    /// [`Self::shutdown_now`] kills them.
    #[cfg_attr(not(feature = "thread-simulator"), allow(clippy::unused_self))]
    fn in_thread_group<R>(&self, f: impl FnOnce() -> R) -> R {
        #[cfg(feature = "thread-simulator")]
        return switchy_thread::simulator::with_group(self.thread_group, f);
        #[cfg(not(feature = "thread-simulator"))]
        f()
    }

    /// Stop this runtime for good, at once, as when its process is killed: drop every task it
    /// has not finished, queued or waiting to be woken, without polling any of them again.
    ///
    /// Tasks are dropped one at a time in the order they were spawned, on the calling thread,
    /// with this runtime current, so their destructors run deterministically (and can see the
    /// caller's scope, such as a simulated host). Their join handles (and `JoinSet`s) report
    /// them cancelled. Tasks spawned while it runs (by a destructor, say) are dropped too, and
    /// the runtime never runs a task again: a task spawned onto it afterwards is dropped at
    /// once, and [`Self::tick`] does nothing.
    ///
    /// With `thread-simulator`, a poll of one of its tasks under way on another thread (one
    /// stepping it from outside its thread group) runs to its next await first: the task is
    /// dropped then, and nothing of the runtime runs once this returns.
    ///
    /// With `thread-simulator`, the simulated threads its tasks started (`spawn_blocking`, or
    /// `switchy_thread` spawns from a task), and the threads they started, are killed too: see
    /// `switchy_thread::simulator::Scheduler::kill_group`. A blocking closure is stopped at its
    /// current switch point and unwound, so the values it holds are dropped and its locks
    /// released; it runs none of its own code again, and its join handle reports it cancelled.
    ///
    /// # Panics
    ///
    /// * If called from within one of this runtime's own tasks
    pub fn shutdown_now(&self) {
        assert!(
            !(RUNTIME.is_set() && RUNTIME.with(|current| current.id == self.id)),
            "Cannot shut a Runtime down from within one of its tasks"
        );
        log::debug!("shutdown_now: runtime {}", self.id);
        self.shut_down.store(true, Ordering::SeqCst);
        self.active.store(false, Ordering::SeqCst);
        RUNTIME.set(self, || {
            loop {
                let tasks = self.live_tasks();
                if tasks.is_empty() {
                    break;
                }
                for task in tasks {
                    task.kill();
                }
                #[cfg(feature = "thread-simulator")]
                if let Some(scheduler) = switchy_thread::simulator::try_scheduler() {
                    scheduler.kill_group(self.thread_group);
                }
            }
            #[cfg(feature = "thread-simulator")]
            if let Some(scheduler) = switchy_thread::simulator::try_scheduler() {
                scheduler.kill_group(self.thread_group);
                self.await_polls_elsewhere();
            }
        });
        let queued =
            std::mem::take(&mut *self.queue.lock().unwrap_or_else(PoisonError::into_inner));
        drop(queued);
    }

    /// Wait until no thread outside this runtime's group (one stepping it: a worker of another
    /// runtime, say) is still polling one of its tasks. Killing the group cannot stop such a
    /// poll, and a runtime shut down must not go on: the poll returns at its next await, and
    /// drops the task then.
    ///
    /// # Panics
    ///
    /// * If such a poll never returns (it waits for the caller)
    #[cfg(feature = "thread-simulator")]
    fn await_polls_elsewhere(&self) {
        // Ten thousand seconds of the run's clock.
        const PATIENCE: u32 = 1_000_000;
        for _ in 0..PATIENCE {
            let polled = {
                let registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
                registry
                    .values()
                    .filter_map(Weak::upgrade)
                    .any(|task| task.polled_elsewhere())
            };
            if !polled {
                return;
            }
            // Asleep, not yielding: the poll may be sleeping itself, and the run's clock moves
            // only when every thread waits.
            switchy_thread::thread::sleep(std::time::Duration::from_millis(10));
        }
        let pollers: Vec<String> = {
            let registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
            let snapshot = switchy_thread::simulator::try_scheduler()
                .map(|scheduler| scheduler.snapshot());
            registry
                .values()
                .filter_map(Weak::upgrade)
                .filter(|task| task.polled_elsewhere())
                .filter_map(|task| *task.poller.lock().unwrap_or_else(PoisonError::into_inner))
                .map(|(_, index)| {
                    let thread = index.and_then(|index| {
                        snapshot.as_ref().and_then(|s| s.threads.get(index))
                    });
                    thread.map_or_else(
                        || "a thread outside the run".to_string(),
                        |t| format!("#{} {} ({:?} at {:?})", t.index, t.name, t.state, t.op),
                    )
                })
                .collect()
        };
        panic!(
            "shutting runtime {} down: a poll of one of its tasks on another thread never \
             returned (does it wait for the thread shutting it down?): {pollers:?}",
            self.id
        );
    }

    /// Whether [`Self::shutdown_now`] stopped this runtime.
    #[must_use]
    pub fn is_shut_down(&self) -> bool {
        self.shut_down.load(Ordering::SeqCst)
    }

    /// The unfinished tasks not killed yet, in the order they were spawned.
    fn live_tasks(&self) -> Vec<Arc<Task>> {
        let mut registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
        registry.retain(|_, task| task.strong_count() > 0);
        registry
            .values()
            .filter_map(Weak::upgrade)
            .filter(|task| !task.killed())
            .collect()
    }

    /// Finish this runtime only if every admitted task has released its future.
    ///
    /// This does not poll tasks, advance time, or request cancellation. A false
    /// result retains execution ownership so the caller can drive or cancel work
    /// and check again. A true result does not prevent subsequent admission through
    /// retained handles and is not a shutdown fence. Callers must stop producers
    /// before treating this observation as the end of a run.
    #[must_use]
    pub fn try_finish(&self) -> bool {
        if self.tasks() != 0 {
            return false;
        }
        self.active.store(false, Ordering::SeqCst);
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
            ENTERED.with_borrow(|entered| entered.last().cloned())
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
    abort_handle: Option<futures::future::AbortHandle>,
    /// Set once the task's result is sent or the task is dropped (see [`Finished`]).
    done: Arc<AtomicBool>,
}

/// Marks a task finished when dropped: when its future completes, or is dropped unfinished
/// (aborted), or (for blocking work on a simulated thread) when that work returns.
struct Finished(Arc<AtomicBool>);

impl Finished {
    fn new() -> (Self, Arc<AtomicBool>) {
        let done = Arc::new(AtomicBool::new(false));
        (Self(Arc::clone(&done)), done)
    }
}

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl<T> JoinHandle<T> {
    /// A handle that can abort this task without owning (or awaiting) it.
    #[must_use]
    pub fn abort_handle(&self) -> AbortHandle {
        AbortHandle {
            abort_handle: self.abort_handle.clone(),
            done: Arc::clone(&self.done),
        }
    }
}

/// A handle to abort a spawned task, without the right to await its result (as
/// `tokio::task::AbortHandle`).
///
/// Dropping it neither aborts nor detaches the task.
#[derive(Debug, Clone)]
pub struct AbortHandle {
    abort_handle: Option<futures::future::AbortHandle>,
    done: Arc<AtomicBool>,
}

impl AbortHandle {
    /// Requests cancellation of the task at its next poll; see [`JoinHandle::abort`].
    pub fn abort(&self) {
        if let Some(handle) = &self.abort_handle {
            handle.abort();
        }
    }

    /// Whether the task has finished: completed, or dropped after being aborted.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

impl<T: Send + Unpin> JoinHandle<T> {
    /// Checks if the task has completed.
    ///
    /// Returns `true` if the task has finished executing, `false` otherwise.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished || self.done.load(Ordering::SeqCst)
    }

    /// Requests cancellation of an asynchronous task at its next poll.
    ///
    /// Cancellation drops the task future and makes joining return an error. A completed
    /// task retains its result. A queued blocking closure can be cancelled before its
    /// first poll, but once it starts executing it cannot be interrupted.
    pub fn abort(&self) {
        if let Some(handle) = &self.abort_handle {
            handle.abort();
        }
    }
}

impl<T: Send + Unpin> Future for JoinHandle<T> {
    type Output = Result<T, task::JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if let Some(result) = self.as_mut().result.take() {
            return Poll::Ready(result);
        }

        let this = self.get_mut();
        match Pin::new(&mut this.rx).poll(cx) {
            Poll::Ready(x) => {
                this.finished = true;
                Poll::Ready(x.map_err(|_| task::JoinError::new()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Task spawner for the simulator runtime.
///
/// This handles spawning new tasks onto the runtime's task queue. It manages
/// both regular async tasks and blocking tasks.
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

        let (abort_handle, registration) = futures::future::AbortHandle::new_pair();
        let (finished, done) = Finished::new();
        let wrapped = async move {
            let _ = futures::future::Abortable::new(
                async move {
                    let _finished = finished;
                    let _ = tx.send(future.await);
                },
                registration,
            )
            .await;
        };

        let task = Task::new(runtime, false, wrapped);
        self.inner_spawn(&task);
        // Wake the executor if it waits for work: the spawn may come from another thread.
        task.runtime.notify();

        JoinHandle {
            rx,
            result: None,
            finished: false,
            abort_handle: Some(abort_handle),
            done,
        }
    }

    fn spawn_blocking<F, R>(&self, runtime: Runtime, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        log::trace!("spawn_blocking");
        let (tx, rx) = futures::channel::oneshot::channel();

        let (abort_handle, registration) = futures::future::AbortHandle::new_pair();
        let (finished, done) = Finished::new();
        #[cfg(feature = "thread-simulator")]
        let owner = runtime.clone();
        let wrapped = async move {
            let _ = futures::future::Abortable::new(
                async move {
                    // With simulated threads, the work starts on one when the executor reaches
                    // this task (so aborting before then still drops it unrun), and blocking in
                    // it blocks only that thread.
                    #[cfg(feature = "thread-simulator")]
                    start_blocking_thread(&owner, func, tx, finished);
                    #[cfg(not(feature = "thread-simulator"))]
                    {
                        let _finished = finished;
                        let _ = tx.send(func());
                    }
                },
                registration,
            )
            .await;
        };

        self.inner_spawn_blocking(&Task::new(runtime, true, wrapped));

        JoinHandle {
            rx,
            result: None,
            finished: false,
            abort_handle: Some(abort_handle),
            done,
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
        let proxy = LocalFutureProxy::new(future, tx);
        let (abort_handle, registration) = futures::future::AbortHandle::new_pair();
        let (finished, done) = Finished::new();
        let wrapped = async move {
            let _finished = finished;
            let _ = futures::future::Abortable::new(proxy, registration).await;
        };

        let task = Task::new(runtime, false, wrapped);
        task.local.store(true, Ordering::SeqCst);
        self.inner_spawn(&task);
        // Wake the executor if it waits for work: the spawn may come from another thread.
        task.runtime.notify();

        JoinHandle {
            rx,
            result: None,
            finished: false,
            abort_handle: Some(abort_handle),
            done,
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

/// A task running on the simulator runtime.
///
/// Tasks are the unit of execution in the simulator. Each task wraps a future and
/// tracks its execution state. Tasks can be either blocking or non-blocking, which
/// determines how the runtime schedules their execution.
pub(crate) struct Task {
    /// Unique identifier for this task
    id: u64,
    /// Runtime this task belongs to
    runtime: Runtime,
    /// The future being executed
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    /// Whether this task has completed
    finished: AtomicBool,
    /// Whether this is a blocking task
    block: bool,
    /// Whether its future is being polled (perhaps further up the stack, if it nests
    /// `Handle::block_on`).
    polling: AtomicBool,
    /// Whether its future must be polled on the thread that spawned it (`spawn_local`).
    pub(crate) local: AtomicBool,
    /// Killed (its runtime shut down) while being polled elsewhere: the poll that holds its
    /// future drops it when it returns.
    killed: AtomicBool,
    /// The thread polling its future right now, if any (and its index, if a simulated thread).
    poller: std::sync::Mutex<Option<Poller>>,
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
        let registry = Arc::clone(&runtime.registry);
        let task = Arc::new(Self {
            id: TASK_ID.fetch_add(1, Ordering::SeqCst),
            runtime,
            future: Mutex::new(Box::pin(future)),
            finished: AtomicBool::new(false),
            block,
            polling: AtomicBool::new(false),
            poller: std::sync::Mutex::new(None),
            local: AtomicBool::new(false),
            killed: AtomicBool::new(false),
        });
        registry
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(task.id, Arc::downgrade(&task));
        if task.runtime.is_shut_down() {
            // A shut-down runtime never runs a task again.
            task.kill();
        }
        task
    }

    /// Drop the task's future now, unpolled, and count it finished. If another thread is
    /// polling it right now (another worker), that poll drops it when it returns: a kill never
    /// waits for a poll.
    fn kill(&self) {
        self.killed.store(true, Ordering::SeqCst);
        let future = match self.future.try_lock() {
            Ok(mut future) => std::mem::replace(&mut *future, Box::pin(std::future::ready(()))),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                let mut future = poisoned.into_inner();
                std::mem::replace(&mut *future, Box::pin(std::future::ready(())))
            }
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        drop(future);
        self.finish();
    }

    /// Set the thread polling its future, returning the one before.
    fn set_poller(&self, poller: Option<Poller>) -> Option<Poller> {
        std::mem::replace(
            &mut *self.poller.lock().unwrap_or_else(PoisonError::into_inner),
            poller,
        )
    }

    /// Whether a thread other than the calling one is polling its future right now, and will
    /// go on (one killed runs none of the task's code again, though it may never return: parked
    /// for good, blocked while unwinding).
    #[cfg(feature = "thread-simulator")]
    fn polled_elsewhere(&self) -> bool {
        let poller = *self.poller.lock().unwrap_or_else(PoisonError::into_inner);
        poller.is_some_and(|(thread, index)| {
            thread != std::thread::current().id()
                && !index.is_some_and(|index| {
                    switchy_thread::simulator::try_scheduler()
                        .is_some_and(|scheduler| scheduler.killed(index))
                })
        })
    }

    /// Whether the task was killed (and is finished, or about to be when its poll returns).
    fn killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
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
        let mut future = match self.future.try_lock() {
            Ok(future) => future,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            // Polled on another worker right now (woken meanwhile): poll it again later.
            Err(std::sync::TryLockError::WouldBlock) => {
                self.runtime.spawner.inner_spawn(self);
                return Poll::Pending;
            }
        };
        if self.killed() {
            let killed = std::mem::replace(&mut *future, Box::pin(std::future::ready(())));
            drop(future);
            drop(killed);
            self.finish();
            return Poll::Ready(());
        }
        let poll = {
            // Cleared even when the poll unwinds (a panic, or its thread killed).
            let _polling = Polling::start(self);
            future.as_mut().poll(&mut ctx)
        };
        // Killed while it ran (on another worker): drop it now.
        let poll = if self.killed() { Poll::Ready(()) } else { poll };
        match poll {
            Poll::Ready(()) => {
                // Retained wakers may keep Task alive indefinitely. Release user state
                // now, outside the lock, before declaring the work drained.
                let completed = std::mem::replace(&mut *future, Box::pin(std::future::ready(())));
                drop(future);
                drop(completed);
                self.finish();
                Poll::Ready(())
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
                    self.runtime.idle();
                }
            }
        } else {
            // Non-blocking task: poll once. If the future returns Pending,
            // it must register interest via its own waker to be rescheduled.
            let _ = self.poll();
        }
    }

    fn finish(&self) {
        // Active work, not retained Arc/Waker references, determines runtime drain.
        if !self.finished.swap(true, Ordering::SeqCst) {
            self.runtime.tasks.fetch_sub(1, Ordering::SeqCst);
            self.runtime
                .registry
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&self.id);
        }
    }

    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

/// A thread polling a task: its id, and its index if it is a simulated thread of the run.
type Poller = (std::thread::ThreadId, Option<usize>);

/// Marks a task polled by the calling thread while it lives.
struct Polling<'a> {
    task: &'a Task,
    outer: Option<Poller>,
}

impl<'a> Polling<'a> {
    fn start(task: &'a Task) -> Self {
        task.polling.store(true, Ordering::SeqCst);
        #[cfg(feature = "thread-simulator")]
        let index = switchy_thread::simulator::try_scheduler()
            .and_then(|scheduler| scheduler.try_me());
        #[cfg(not(feature = "thread-simulator"))]
        let index = None;
        let outer = task.set_poller(Some((std::thread::current().id(), index)));
        Self { task, outer }
    }
}

impl Drop for Polling<'_> {
    fn drop(&mut self) {
        self.task.set_poller(self.outer);
        self.task.polling.store(false, Ordering::SeqCst);
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        self.finish();
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        log::trace!("wake");
        // Wakers can outlive their task or runtime. Stale delivery must not reopen
        // completed work or panic during cleanup.
        if self.finished() || !self.runtime.active() {
            return;
        }
        if self.block {
            self.runtime.spawner.inner_spawn_blocking(&self);
        } else {
            self.runtime.spawner.inner_spawn(&self);
        }
        self.runtime.notify();
    }
}

/// Run `func` on a simulated thread of the current run, sending its result on `result`. The
/// thread counts as one of `runtime`'s tasks until it returns, and wakes the executor then.
#[cfg(feature = "thread-simulator")]
fn start_blocking_thread<F, R>(
    runtime: &Runtime,
    func: F,
    result: futures::channel::oneshot::Sender<R>,
    finished: Finished,
) where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    runtime.tasks.fetch_add(1, Ordering::SeqCst);
    let done = runtime.clone();
    switchy_thread::thread::Builder::new()
        .name("spawn-blocking".to_owned())
        .spawn(move || {
            let _ = result.send(func());
            drop(finished);
            done.tasks.fetch_sub(1, Ordering::SeqCst);
            done.notify();
        })
        .expect("a simulated thread starts");
}

/// The current run's number of workers for runtimes that do not fix their own (see the
/// `Runtime` docs on workers): from `SIMULATOR_ASYNC_WORKERS`, else 1.
#[cfg(feature = "thread-simulator")]
#[must_use]
pub fn run_workers() -> usize {
    super::workers::run_workers()
}

/// Builds a new simulator runtime from the given builder.
///
/// # Errors
///
/// * This function always succeeds for the simulator runtime
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn build_runtime(builder: &Builder) -> Result<Runtime, Error> {
    let runtime = Runtime::new();
    // A current-thread runtime never runs two tasks at once; a multi-thread one runs its
    // `worker_threads`, else the run's number of workers.
    #[cfg(feature = "thread-simulator")]
    {
        let workers = match builder.flavor {
            crate::RuntimeFlavor::CurrentThread => 1,
            crate::RuntimeFlavor::MultiThread => {
                #[cfg(feature = "rt-multi-thread")]
                let fixed = builder.worker_threads.unwrap_or(0);
                #[cfg(not(feature = "rt-multi-thread"))]
                let fixed = 0;
                fixed
            }
        };
        runtime.worker_count.store(workers, Ordering::SeqCst);
    }
    #[cfg(not(feature = "thread-simulator"))]
    let _ = builder;
    Ok(runtime)
}

#[cfg(test)]
mod test {
    #[allow(unused)]
    use pretty_assertions::{assert_eq, assert_ne};

    use std::sync::{Arc, Mutex};

    use crate::{
        runtime::Builder,
        simulator::runtime::{Handle, Runtime, build_runtime},
        task,
    };

    #[test_log::test]
    fn try_current_tracks_runtime_scope_and_thread_isolation() {
        assert!(Handle::try_current().is_err());
        let runtime = build_runtime(&Builder::new()).unwrap();
        runtime.block_on(async {
            let handle = Handle::try_current().expect("active runtime");
            assert_eq!(handle.spawn(async { 42 }).await.unwrap(), 42);
            assert!(
                std::thread::spawn(|| Handle::try_current().is_err())
                    .join()
                    .unwrap()
            );
        });
        assert!(Handle::try_current().is_err());
        runtime.wait().unwrap();
    }

    struct DrainRelease(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for DrainRelease {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test_log::test]
    fn try_finish_observes_release_without_polling_work() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let release = DrainRelease(Arc::clone(&released));
        let task = runtime.spawn(async move {
            let _release = release;
            std::future::pending::<()>().await;
        });
        assert!(!runtime.try_finish());
        assert!(!released.load(std::sync::atomic::Ordering::SeqCst));
        task.abort();
        assert!(!runtime.try_finish());
        for _ in 0..100 {
            runtime.tick();
            if runtime.try_finish() {
                assert!(released.load(std::sync::atomic::Ordering::SeqCst));
                return;
            }
        }
        panic!("cancelled task did not release ownership");
    }

    #[test_log::test]
    fn rt_current_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) });
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
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

    #[test_log::test]
    fn rt_current_thread_runtime_block_on_same_thread() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            assert_eq!(std::thread::current().id(), thread_id);
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test_log::test]
    fn rt_multi_thread_runtime_spawns_on_same_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            task::spawn(async move { assert_eq!(std::thread::current().id(), thread_id) });
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "rt-multi-thread")]
    #[test_log::test]
    fn rt_multi_thread_runtime_block_on_same_thread() {
        let runtime = build_runtime(Builder::new().max_blocking_threads(1)).unwrap();

        let thread_id = std::thread::current().id();

        runtime.block_on(async move {
            assert_eq!(std::thread::current().id(), thread_id);
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn runtime_tick_processes_single_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let completed = Arc::new(Mutex::new(false));
        let completed_clone = Arc::clone(&completed);

        runtime.spawn(async move {
            *completed_clone.lock().unwrap() = true;
        });

        // Tick should process the spawned task
        runtime.tick();

        // Give it a moment to complete
        std::thread::sleep(std::time::Duration::from_millis(10));

        assert!(*completed.lock().unwrap());
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn runtime_current_returns_none_outside_runtime() {
        let current = Runtime::current();
        assert!(current.is_none());
    }

    #[test_log::test]
    fn runtime_current_returns_some_inside_runtime() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            let current = Runtime::current();
            assert!(current.is_some());
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn runtime_equality_based_on_id() {
        let runtime1 = build_runtime(&Builder::new()).unwrap();
        let runtime2 = build_runtime(&Builder::new()).unwrap();

        // Same runtime should be equal to itself
        assert_eq!(runtime1, runtime1.clone());

        // Different runtimes should not be equal
        assert_ne!(runtime1, runtime2);
    }

    #[test_log::test]
    fn runtime_default_is_same_as_new() {
        let runtime1 = Runtime::default();
        let runtime2 = Runtime::new();

        // Both should work the same way
        let result1 = runtime1.block_on(async { 42 });
        let result2 = runtime2.block_on(async { 42 });

        assert_eq!(result1, result2);

        runtime1.wait().unwrap();
        runtime2.wait().unwrap();
    }

    #[test_log::test]
    fn handle_spawn_executes_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let handle = runtime.handle();

        let join_handle = handle.spawn(async { 42 });

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, 42);
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn handle_spawn_blocking_executes_blocking_code() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let handle = runtime.handle();

        let join_handle = handle.spawn_blocking(|| {
            // Simulate blocking work
            42
        });

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, 42);
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn runtime_spawn_with_name_executes_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let join_handle = runtime.spawn_with_name("test_task", async { 123 });

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, 123);
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn runtime_spawn_blocking_with_name_executes_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let join_handle = runtime.spawn_blocking_with_name("blocking_task", || 456);

        let result = runtime.block_on(async { join_handle.await.unwrap() });

        assert_eq!(result, 456);
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn join_handle_is_finished_detects_completion() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        let join_handle = runtime.spawn(async { 42 });

        // Initially not finished
        assert!(!join_handle.is_finished());

        // Wait for completion
        let _result = runtime.block_on(async { join_handle.await.unwrap() });

        // Now should be finished (need a new handle to test)
        let join_handle2 = runtime.spawn(async { 10 });
        runtime.block_on(async {
            let _ = join_handle2.await;
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn join_handle_abort_before_first_poll_cancels_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_ran = ran.clone();
        let join_handle = runtime.spawn(async move {
            task_ran.store(true, std::sync::atomic::Ordering::SeqCst);
            42
        });
        join_handle.abort();
        let result = runtime.block_on(join_handle);
        assert!(result.is_err());
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn joined_handle_remains_finished_after_result_consumption() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let mut handle = runtime.spawn(async { 42 });
        assert_eq!(runtime.block_on(&mut handle).unwrap(), 42);
        assert!(handle.finished);
        assert!(handle.is_finished());
        assert!(handle.is_finished());
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn cancelled_join_remains_finished_after_error_consumption() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let mut handle = runtime.spawn(std::future::pending::<()>());
        handle.abort();
        assert!(runtime.block_on(&mut handle).is_err());
        assert!(handle.finished);
        assert!(handle.is_finished());
        runtime.wait().unwrap();
    }

    fn scheduled_trace() -> Vec<(u8, u8)> {
        switchy_random::simulator::reset_rng();
        let runtime = build_runtime(&Builder::new()).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        for actor in 0..8_u8 {
            let events = events.clone();
            drop(runtime.spawn(async move {
                for step in 0..4_u8 {
                    events.lock().unwrap().push((actor, step));
                    task::yield_now().await;
                }
            }));
        }
        // A fixed poll budget catches lack of progress without a wall-clock hang.
        for _ in 0..128 {
            runtime.tick();
        }
        assert_eq!(runtime.tasks(), 0);
        runtime.wait().unwrap();
        Arc::try_unwrap(events).unwrap().into_inner().unwrap()
    }

    #[test_log::test]
    fn resetting_scheduler_rng_replays_concurrent_trace() {
        let first = scheduled_trace();
        assert_eq!(first.len(), 32);
        let second = scheduled_trace();
        assert_eq!(first, second);
        for actor in 0..8_u8 {
            let steps: Vec<_> = first
                .iter()
                .filter(|(id, _)| *id == actor)
                .map(|(_, step)| *step)
                .collect();
            assert_eq!(steps, vec![0, 1, 2, 3]);
        }
    }

    #[cfg(feature = "time")]
    fn timed_scheduled_trace(deadlines: [u32; 8]) -> Vec<u8> {
        // Fresh thread-local clock state per run avoids rewinding live timers.
        std::thread::spawn(move || {
            switchy_random::simulator::reset_rng();
            let period =
                std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let runtime = build_runtime(&Builder::new()).unwrap();
            let events = Arc::new(Mutex::new(Vec::new()));
            for actor in 0..8_u8 {
                let events = events.clone();
                drop(runtime.spawn(async move {
                    crate::time::sleep(period * deadlines[usize::from(actor)]).await;
                    events.lock().unwrap().push(actor);
                }));
            }
            for _ in 0..128 {
                runtime.tick();
            }
            assert!(events.lock().unwrap().is_empty());
            assert_eq!(runtime.tasks(), 8);
            for elapsed in 1..=8 {
                let _ = switchy_time::simulator::next_step();
                for _ in 0..128 {
                    runtime.tick();
                }
                let completed = deadlines
                    .iter()
                    .filter(|deadline| **deadline <= elapsed)
                    .count();
                assert_eq!(events.lock().unwrap().len(), completed);
                assert_eq!(runtime.tasks(), u64::try_from(8 - completed).unwrap());
            }
            runtime.wait().unwrap();
            Arc::try_unwrap(events).unwrap().into_inner().unwrap()
        })
        .join()
        .unwrap()
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn explicit_clock_steps_replay_sleeping_tasks() {
        let deadlines = [1, 2, 3, 4, 5, 6, 7, 8];
        let first = timed_scheduled_trace(deadlines);
        assert_eq!(first, (0..8).collect::<Vec<_>>());
        assert_eq!(first, timed_scheduled_trace(deadlines));
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn equal_deadline_sleepers_replay_completion_order() {
        let first = timed_scheduled_trace([1; 8]);
        assert_eq!(first, timed_scheduled_trace([1; 8]));
        let mut actors = first;
        actors.sort_unstable();
        assert_eq!(actors, (0..8).collect::<Vec<_>>());
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn abort_sleepers_before_dispatch_preserves_independent_timer() {
        std::thread::spawn(|| {
            let period =
                std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let runtime = build_runtime(&Builder::new()).unwrap();
            let events = Arc::new(Mutex::new(Vec::new()));
            let mut handles = Vec::new();
            let mut owned_states = Vec::new();
            for actor in 0..3_u8 {
                let state = Arc::new(());
                owned_states.push(Arc::downgrade(&state));
                let events = events.clone();
                handles.push(runtime.spawn(async move {
                    crate::time::sleep(period).await;
                    events.lock().unwrap().push(actor);
                    drop(state);
                }));
            }
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(runtime.tasks(), 3);
            assert!(owned_states.iter().all(|state| state.upgrade().is_some()));
            handles[0].abort();
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(runtime.tasks(), 2);
            assert!(owned_states[0].upgrade().is_none());
            let _ = switchy_time::simulator::next_step();
            // Deadline has arrived, but neither remaining task has been dispatched.
            handles[1].abort();
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(runtime.tasks(), 0);
            assert!(owned_states.iter().all(|state| state.upgrade().is_none()));
            assert_eq!(*events.lock().unwrap(), vec![2]);
            let mut handles = handles.into_iter();
            assert!(futures::executor::block_on(handles.next().unwrap()).is_err());
            assert!(futures::executor::block_on(handles.next().unwrap()).is_err());
            assert!(futures::executor::block_on(handles.next().unwrap()).is_ok());
            let _ = switchy_time::simulator::next_step();
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(*events.lock().unwrap(), vec![2]);
            runtime.wait().unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn scheduled_timeout_releases_pending_work_at_deadline() {
        std::thread::spawn(|| {
            let period =
                std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let runtime = build_runtime(&Builder::new()).unwrap();
            let state = Arc::new(());
            let weak = Arc::downgrade(&state);
            let handle = runtime.spawn(async move {
                crate::time::timeout(period, async move {
                    std::future::pending::<()>().await;
                    drop(state);
                })
                .await
            });
            for _ in 0..128 {
                runtime.tick();
            }
            assert!(!handle.is_finished());
            assert_eq!(runtime.tasks(), 1);
            assert!(weak.upgrade().is_some());
            let _ = switchy_time::simulator::next_step();
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(runtime.tasks(), 0);
            assert!(handle.is_finished());
            assert!(weak.upgrade().is_none());
            assert!(futures::executor::block_on(handle).unwrap().is_err());
            runtime.wait().unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn finishing_runtime_does_not_dispatch_or_cancel_other_runtime_timer() {
        std::thread::spawn(|| {
            let period =
                std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let first = build_runtime(&Builder::new()).unwrap();
            let second = build_runtime(&Builder::new()).unwrap();
            let state = Arc::new(());
            let weak = Arc::downgrade(&state);
            let sleeper = second.spawn(async move {
                crate::time::sleep(period).await;
                drop(state);
                19
            });
            for _ in 0..128 {
                second.tick();
            }
            let finished = first.spawn(async { 7 });
            for _ in 0..128 {
                first.tick();
            }
            assert_eq!(first.tasks(), 0);
            assert_eq!(futures::executor::block_on(finished).unwrap(), 7);
            first.wait().unwrap();
            assert_eq!(second.tasks(), 1);
            assert!(!sleeper.is_finished());
            assert!(weak.upgrade().is_some());
            let _ = switchy_time::simulator::next_step();
            // Clock advancement alone must not dispatch another runtime's queue.
            assert!(!sleeper.is_finished());
            for _ in 0..128 {
                second.tick();
            }
            assert_eq!(second.tasks(), 0);
            assert!(weak.upgrade().is_none());
            assert!(sleeper.is_finished());
            assert_eq!(futures::executor::block_on(sleeper).unwrap(), 19);
            second.wait().unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn abort_local_sleeper_releases_thread_local_state() {
        std::thread::spawn(|| {
            let period =
                std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let runtime = build_runtime(&Builder::new()).unwrap();
            let state = std::rc::Rc::new(());
            let weak = std::rc::Rc::downgrade(&state);
            let completed = std::rc::Rc::new(std::cell::Cell::new(false));
            let effect = completed.clone();
            let handle = runtime.spawn_local(async move {
                crate::time::sleep(period).await;
                effect.set(true);
                drop(state);
            });
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(runtime.tasks(), 1);
            assert!(weak.upgrade().is_some());
            assert!(!handle.is_finished());
            handle.abort();
            for _ in 0..128 {
                runtime.tick();
            }
            assert_eq!(runtime.tasks(), 0);
            assert!(weak.upgrade().is_none());
            assert!(handle.is_finished());
            assert!(futures::executor::block_on(handle).is_err());
            let _ = switchy_time::simulator::next_step();
            for _ in 0..128 {
                runtime.tick();
            }
            assert!(!completed.get());
            assert_eq!(std::rc::Rc::strong_count(&completed), 1);
            runtime.wait().unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn scheduled_interval_retains_cadence_across_clock_steps() {
        for clock_steps in [[0, 1, 2, 3], [0, 0, 0, 3]] {
            scheduled_interval_trace(clock_steps);
        }
    }

    #[cfg(feature = "time")]
    fn scheduled_interval_trace(clock_steps: [u64; 4]) {
        std::thread::spawn(move || {
            let period =
                std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
            assert!(!period.is_zero());
            let runtime = build_runtime(&Builder::new()).unwrap();
            let events = Arc::new(Mutex::new(Vec::new()));
            let observed = events.clone();
            let start = switchy_time::instant_now();
            let handle = runtime.spawn(async move {
                let mut interval = crate::time::interval(period);
                for _ in 0..4 {
                    let deadline = interval.tick().await;
                    observed.lock().unwrap().push(deadline);
                }
            });
            let initial_step = switchy_time::simulator::current_step();
            for elapsed in clock_steps {
                let _ = switchy_time::simulator::set_step(initial_step + elapsed);
                for _ in 0..128 {
                    runtime.tick();
                }
                let expected: Vec<_> = (0..=u32::try_from(elapsed).unwrap())
                    .map(|step| start + period * step)
                    .collect();
                assert_eq!(*events.lock().unwrap(), expected);
                assert_eq!(handle.is_finished(), elapsed == 3);
                assert_eq!(runtime.tasks(), u64::from(elapsed != 3));
            }
            assert!(futures::executor::block_on(handle).is_ok());
            runtime.wait().unwrap();
        })
        .join()
        .unwrap();
    }

    struct CountWake(std::sync::atomic::AtomicUsize);

    impl std::task::Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test_log::test]
    fn completion_probe_preserves_registered_join_waker() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let mut handle = runtime.spawn(async { 42 });
        let counter = Arc::new(CountWake(std::sync::atomic::AtomicUsize::new(0)));
        let waker = std::task::Waker::from(counter.clone());
        let mut cx = std::task::Context::from_waker(&waker);
        assert!(std::future::Future::poll(std::pin::Pin::new(&mut handle), &mut cx).is_pending());
        assert!(!handle.is_finished());
        runtime.tick();
        assert!(counter.0.load(std::sync::atomic::Ordering::SeqCst) > 0);
        assert_eq!(runtime.block_on(handle).unwrap(), 42);
        runtime.wait().unwrap();
    }

    struct ReadyOwned(Arc<()>);

    impl std::future::Future for ReadyOwned {
        type Output = ();
        fn poll(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            assert_eq!(Arc::strong_count(&self.0), 1);
            std::task::Poll::Ready(())
        }
    }

    #[test_log::test]
    fn completed_future_drops_before_retained_waker() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let owned = Arc::new(());
        let weak = Arc::downgrade(&owned);
        let task = super::Task::new(runtime.clone(), false, ReadyOwned(owned));
        let waker = task.waker();
        assert!(task.poll().is_ready());
        assert!(weak.upgrade().is_none());
        assert_eq!(runtime.tasks(), 0);
        drop(waker);
        drop(task);
    }

    #[test_log::test]
    fn completed_task_releases_count_before_retained_waker_is_dropped() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        runtime.start();
        let task = super::Task::new(runtime.clone(), false, async {});
        let waker = task.waker();
        assert_eq!(runtime.tasks(), 1);
        assert!(task.poll().is_ready());
        assert_eq!(runtime.tasks(), 0);
        assert!(task.poll().is_ready());
        assert_eq!(runtime.tasks(), 0);
        drop(task);
        runtime.clone().wait().unwrap();
        waker.wake();
        assert_eq!(runtime.tasks(), 0);
    }

    #[test_log::test]
    fn pending_task_remains_counted_until_released() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let task = super::Task::new(runtime.clone(), false, std::future::pending());
        assert!(task.poll().is_pending());
        assert_eq!(runtime.tasks(), 1);
        drop(task);
        assert_eq!(runtime.tasks(), 0);
    }

    #[test_log::test]
    fn stale_wake_after_completion_does_not_enqueue_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        runtime.start();
        let task = super::Task::new(runtime.clone(), false, async {});
        let waker = task.waker();
        assert!(task.poll().is_ready());
        waker.wake_by_ref();
        assert!(runtime.queue.lock().unwrap().is_empty());
        drop(waker);
        drop(task);
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn stale_wake_on_inactive_runtime_is_harmless() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let task = super::Task::new(runtime.clone(), false, async {});
        let waker = task.waker();
        waker.wake();
        assert!(runtime.queue.lock().unwrap().is_empty());
        drop(task);
        assert_eq!(runtime.tasks(), 0);
    }

    #[test_log::test]
    fn task_drop_without_runtime_scope_releases_owner_count() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let task = super::Task::new(runtime.clone(), false, async {});
        assert_eq!(runtime.tasks(), 1);
        drop(task);
        assert_eq!(runtime.tasks(), 0);
    }

    #[test_log::test]
    fn task_drop_in_foreign_scope_releases_only_owner_count() {
        let owner = build_runtime(&Builder::new()).unwrap();
        let other = build_runtime(&Builder::new()).unwrap();
        let task = super::Task::new(owner.clone(), false, async {});
        super::RUNTIME.set(&other, || drop(task));
        assert_eq!(owner.tasks(), 0);
        assert_eq!(other.tasks(), 0);
    }

    #[test_log::test]
    fn abort_queued_blocking_task_drops_captures_without_running() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let owned = Arc::new(());
        let weak = Arc::downgrade(&owned);
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_ran = ran.clone();
        let handle = runtime.spawn_blocking(move || {
            task_ran.store(true, std::sync::atomic::Ordering::SeqCst);
            drop(owned);
        });
        handle.abort();
        assert!(runtime.block_on(handle).is_err());
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(weak.upgrade().is_none());
        runtime.wait().unwrap();
    }

    // It relies on blocking work finishing within one tick, which work on a simulated thread
    // does not (tests/threads.rs has the threaded counterpart).
    #[cfg(not(feature = "thread-simulator"))]
    #[test_log::test]
    fn abort_started_blocking_task_does_not_discard_result() {
        let runtime = build_runtime(&Builder::new()).unwrap();
        let slot = Arc::new(Mutex::new(None::<super::JoinHandle<u32>>));
        let task_slot = slot.clone();
        let handle = runtime.spawn_blocking(move || {
            task_slot.lock().unwrap().as_ref().unwrap().abort();
            42
        });
        *slot.lock().unwrap() = Some(handle);
        runtime.tick();
        let handle = slot.lock().unwrap().take().unwrap();
        assert_eq!(runtime.block_on(handle).unwrap(), 42);
        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn handle_spawn_local_with_name_executes_task() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            use std::cell::RefCell;
            use std::rc::Rc;

            let data = Rc::new(RefCell::new(10));
            let data_clone = data.clone();

            let handle = Handle::current();
            let join_handle = handle.spawn_local_with_name("local_task", async move {
                *data_clone.borrow_mut() += 5;
                *data_clone.borrow()
            });

            let result = join_handle.await.unwrap();
            assert_eq!(result, 15);
            assert_eq!(*data.borrow(), 15);
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn runtime_spawn_local_executes_non_send_future() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            use std::cell::RefCell;
            use std::rc::Rc;

            // Rc is not Send
            let data = Rc::new(RefCell::new(vec![1, 2, 3]));
            let data_clone = data.clone();

            let handle = runtime.spawn_local(async move {
                data_clone.borrow_mut().push(4);
                data_clone.borrow().len()
            });

            let len = handle.await.unwrap();
            assert_eq!(len, 4);
            assert_eq!(data.borrow().len(), 4);
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn local_future_proxy_handles_drop_correctly() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async {
            use std::cell::RefCell;
            use std::rc::Rc;

            let data = Rc::new(RefCell::new(false));

            // Create and immediately drop a local future
            let _handle = runtime.spawn_local(async move {
                *data.borrow_mut() = true;
            });

            // The future may or may not have executed, but drop should be safe
        });

        runtime.wait().unwrap();
    }

    #[test_log::test]
    fn join_error_display_formatting() {
        let err = task::JoinError::new();
        assert_eq!(err.to_string(), "JoinError");
    }

    #[test_log::test]
    fn join_error_is_clonable() {
        let err1 = task::JoinError::new();
        let err2 = err1.clone();
        assert_eq!(err1.to_string(), err2.to_string());
    }
}
