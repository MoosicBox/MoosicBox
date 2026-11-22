//! A deterministic collection of spawned tasks for the simulator runtime, mirroring
//! `tokio::task::JoinSet`.
//!
//! Each task reports its outcome to the set when it finishes, so [`JoinSet::join_next`] yields
//! tasks in the order they finished. The simulator runs one task at a time and picks the next
//! from the run's seeded generator, so that order is the same for the same run seed.
//!
//! Differences from `tokio::task::JoinSet`:
//!
//! * A panicking task is not caught: the panic unwinds through the executor and fails the run
//!   (see [`JoinError`]). Every [`JoinError`] it yields is a cancellation.
//! * Task ids (`join_next_with_id`, `try_join_next_with_id`, `AbortHandle::id`),
//!   `spawn_on` / `spawn_blocking_on` / `spawn_local_on` (whose handle types differ between the
//!   backends), `poll_join_next` and the `Builder` are not provided.

use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll, Waker},
};

use super::{
    runtime::AbortHandle,
    task::{self, JoinError},
};

/// A collection of tasks spawned on the simulator runtime, awaited in the order they finish.
///
/// All tasks of a set have the same output type `T`. Dropping the set aborts every task still
/// in it, as does [`Self::abort_all`]; aborted tasks then join with a cancelled [`JoinError`].
pub struct JoinSet<T> {
    shared: Arc<Shared<T>>,
    tasks: BTreeMap<u64, AbortHandle>,
    next_id: u64,
}

/// The outcomes tasks report to their set, in the order they finished.
struct Shared<T> {
    state: Mutex<State<T>>,
}

struct State<T> {
    finished: VecDeque<(u64, Result<T, JoinError>)>,
    waker: Option<Waker>,
}

impl<T> Shared<T> {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                finished: VecDeque::new(),
                waker: None,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn report(&self, id: u64, result: Result<T, JoinError>) {
        let waker = {
            let mut state = self.lock();
            state.finished.push_back((id, result));
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// Reports a task's outcome to its set: its output on completion, or a cancellation when the
/// task's future (or blocking closure) is dropped unfinished.
struct Reporter<T> {
    id: u64,
    shared: Option<Arc<Shared<T>>>,
}

impl<T> Reporter<T> {
    fn complete(mut self, output: T) {
        if let Some(shared) = self.shared.take() {
            shared.report(self.id, Ok(output));
        }
    }
}

impl<T> Drop for Reporter<T> {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.take() {
            shared.report(self.id, Err(JoinError::new()));
        }
    }
}

impl<T> std::fmt::Debug for JoinSet<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinSet")
            .field("len", &self.tasks.len())
            .finish_non_exhaustive()
    }
}

impl<T> Default for JoinSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> JoinSet<T> {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Shared::new(),
            tasks: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// The number of tasks in the set, including finished tasks not yet joined.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the set has no tasks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    fn reporter(&mut self) -> Reporter<T> {
        let id = self.next_id;
        self.next_id += 1;
        Reporter {
            id,
            shared: Some(Arc::clone(&self.shared)),
        }
    }

    fn insert(&mut self, id: u64, handle: AbortHandle) -> AbortHandle {
        self.tasks.insert(id, handle.clone());
        handle
    }

    /// Aborts every task in the set. They stay in it, and join with a cancelled [`JoinError`]
    /// unless they finished first.
    pub fn abort_all(&mut self) {
        for handle in self.tasks.values() {
            handle.abort();
        }
    }

    /// Removes every task from the set without aborting them; they keep running.
    pub fn detach_all(&mut self) {
        self.tasks.clear();
        // Detached tasks report into the old outcomes, which nothing reads any more.
        self.shared = Shared::new();
    }

    /// Waits for one of the tasks to finish and returns its outcome, or `None` if the set is
    /// empty. Tasks are yielded in the order they finished.
    ///
    /// Cancel safe: if the future is dropped before completing, no outcome is lost.
    pub async fn join_next(&mut self) -> Option<Result<T, JoinError>> {
        std::future::poll_fn(|cx| self.poll_next(Some(cx))).await
    }

    /// The outcome of a task that has already finished, if any, without waiting.
    ///
    /// Returns `None` when no task has finished yet or the set is empty.
    pub fn try_join_next(&mut self) -> Option<Result<T, JoinError>> {
        match self.poll_next(None) {
            Poll::Ready(outcome) => outcome,
            Poll::Pending => None,
        }
    }

    fn poll_next(&mut self, cx: Option<&mut Context<'_>>) -> Poll<Option<Result<T, JoinError>>> {
        if self.tasks.is_empty() {
            return Poll::Ready(None);
        }
        let mut state = self.shared.lock();
        while let Some((id, result)) = state.finished.pop_front() {
            if self.tasks.remove(&id).is_some() {
                return Poll::Ready(Some(result));
            }
        }
        if let Some(cx) = cx {
            state.waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }

    /// Aborts every task and waits for them all to stop, discarding their outcomes.
    pub async fn shutdown(&mut self) {
        self.abort_all();
        while self.join_next().await.is_some() {}
    }

    /// Waits for every task and returns their outputs in the order they finished.
    ///
    /// # Panics
    ///
    /// * If any task fails to join (it was cancelled)
    pub async fn join_all(mut self) -> Vec<T> {
        let mut outputs = Vec::with_capacity(self.len());
        while let Some(outcome) = self.join_next().await {
            match outcome {
                Ok(output) => outputs.push(output),
                Err(error) => panic!("a JoinSet task failed to join: {error}"),
            }
        }
        outputs
    }
}

impl<T: Send + 'static> JoinSet<T> {
    /// Spawns `future` on the current runtime as a task of this set.
    ///
    /// # Panics
    ///
    /// * If no runtime is currently running
    pub fn spawn<F>(&mut self, future: F) -> AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        let reporter = self.reporter();
        let id = reporter.id;
        let handle = task::spawn(async move { reporter.complete(future.await) });
        self.insert(id, handle.abort_handle())
    }

    /// Runs blocking `func` as a task of this set (see [`task::spawn_blocking`]).
    ///
    /// Aborting it before it starts drops `func` unrun; once started it runs to completion.
    ///
    /// # Panics
    ///
    /// * If no runtime is currently running
    pub fn spawn_blocking<F>(&mut self, func: F) -> AbortHandle
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let reporter = self.reporter();
        let id = reporter.id;
        let handle = task::spawn_blocking(move || reporter.complete(func()));
        self.insert(id, handle.abort_handle())
    }
}

impl<T: 'static> JoinSet<T> {
    /// Spawns non-`Send` `future` on the current thread's runtime as a task of this set.
    ///
    /// # Panics
    ///
    /// * If no runtime is currently running
    pub fn spawn_local<F>(&mut self, future: F) -> AbortHandle
    where
        F: Future<Output = T> + 'static,
    {
        let reporter = self.reporter();
        let id = reporter.id;
        let handle = task::spawn_local(async move { reporter.complete(future.await) });
        self.insert(id, handle.abort_handle())
    }
}

impl<T> Drop for JoinSet<T> {
    fn drop(&mut self) {
        self.abort_all();
    }
}

impl<T: Send + 'static, F> Extend<F> for JoinSet<T>
where
    F: Future<Output = T> + Send + 'static,
{
    fn extend<I: IntoIterator<Item = F>>(&mut self, iter: I) {
        for future in iter {
            self.spawn(future);
        }
    }
}

impl<T: Send + 'static, F> FromIterator<F> for JoinSet<T>
where
    F: Future<Output = T> + Send + 'static,
{
    fn from_iter<I: IntoIterator<Item = F>>(iter: I) -> Self {
        let mut set = Self::new();
        set.extend(iter);
        set
    }
}
