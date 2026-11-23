//! A counting semaphore with tokio's API whose waiters are served first in, first out.
//!
//! As in tokio, it is fair: permits released go to the oldest waiter first, a waiter for several
//! permits holds those it was given while it waits for the rest, and a cancelled waiter returns
//! the permits it was given. Waiters are kept in one list ordered by when they started waiting
//! and woken in that order, so the order they are queued on the simulator's executor follows
//! from the run's schedule. [`super::Mutex`] and [`super::RwLock`] are built on it, as tokio's
//! are on its own.

use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Poll, Waker},
};

use super::blocking::{Wait, block_on};
use crate::simulator::{faults, waiters::wake};

/// Error returned from [`Semaphore::acquire`] and the other acquiring methods when the
/// semaphore is closed.
#[derive(Debug)]
pub struct AcquireError(());

impl fmt::Display for AcquireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "semaphore closed")
    }
}

impl std::error::Error for AcquireError {}

/// Error returned from [`Semaphore::try_acquire`] and the other `try_acquire*` methods.
#[derive(Debug, PartialEq, Eq)]
pub enum TryAcquireError {
    /// The semaphore was closed.
    Closed,
    /// The semaphore has no permits left.
    NoPermits,
}

impl fmt::Display for TryAcquireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "semaphore closed"),
            Self::NoPermits => write!(f, "no permits available"),
        }
    }
}

impl std::error::Error for TryAcquireError {}

/// A queued acquire: the permits it still needs and the waker of its task.
struct Waiter {
    remaining: usize,
    waker: Option<Waker>,
}

struct State {
    /// Permits not given to anyone.
    permits: usize,
    closed: bool,
    next: u64,
    /// Waiting acquires, oldest first. Kept after a close, so a cancelled one can return the
    /// permits it was given.
    queue: BTreeMap<u64, Waiter>,
}

impl State {
    /// Give `permits` to the waiters, oldest first, and keep what is left. Returns the wakers
    /// of the waiters now complete, oldest first.
    ///
    /// # Panics
    ///
    /// * If the permits kept would exceed [`Semaphore::MAX_PERMITS`]
    fn release(&mut self, mut permits: usize) -> Vec<Waker> {
        let mut wakers = Vec::new();
        if !self.closed {
            while permits > 0 {
                let Some(mut entry) = self.queue.first_entry() else {
                    break;
                };
                let waiter = entry.get_mut();
                let given = waiter.remaining.min(permits);
                waiter.remaining -= given;
                permits -= given;
                if waiter.remaining > 0 {
                    break;
                }
                if let Some(waker) = entry.remove().waker {
                    wakers.push(waker);
                }
            }
        }
        if permits > 0 {
            assert!(
                permits <= Semaphore::MAX_PERMITS,
                "cannot add more than MAX_PERMITS permits ({})",
                Semaphore::MAX_PERMITS
            );
            let total = self.permits + permits;
            assert!(
                total <= Semaphore::MAX_PERMITS,
                "number of added permits ({permits}) would overflow MAX_PERMITS ({})",
                Semaphore::MAX_PERMITS
            );
            self.permits = total;
        }
        wakers
    }

    /// Poll an acquire of `permits`, queued as `queued` (set when it joins the queue, cleared
    /// when it completes).
    fn poll_acquire(
        &mut self,
        queued: &mut Option<u64>,
        permits: usize,
        waker: &Waker,
    ) -> Poll<Result<(), AcquireError>> {
        // Closed: fail, still queued, so dropping the acquire returns any permits it was given.
        if self.closed {
            return Poll::Ready(Err(AcquireError(())));
        }
        let Some(id) = *queued else {
            let taken = self.permits.min(permits);
            self.permits -= taken;
            if taken == permits {
                return Poll::Ready(Ok(()));
            }
            let id = self.next;
            self.next += 1;
            self.queue.insert(
                id,
                Waiter {
                    remaining: permits - taken,
                    waker: Some(waker.clone()),
                },
            );
            *queued = Some(id);
            return Poll::Pending;
        };
        let Some(waiter) = self.queue.get_mut(&id) else {
            // Every permit it needed was given to it.
            *queued = None;
            return Poll::Ready(Ok(()));
        };
        let taken = self.permits.min(waiter.remaining);
        waiter.remaining -= taken;
        self.permits -= taken;
        if waiter.remaining == 0 {
            self.queue.remove(&id);
            *queued = None;
            return Poll::Ready(Ok(()));
        }
        match &mut waiter.waker {
            Some(current) if current.will_wake(waker) => {}
            slot => *slot = Some(waker.clone()),
        }
        Poll::Pending
    }

    /// Whether an acquire of `permits`, queued as `queued`, would complete if polled now.
    fn ready(&self, queued: Option<u64>, permits: usize) -> bool {
        self.closed
            || queued.map_or(self.permits >= permits, |id| {
                self.queue
                    .get(&id)
                    .is_none_or(|waiter| self.permits >= waiter.remaining)
            })
    }
}

/// A counting semaphore: an async way to limit how many tasks do something at once.
///
/// Mirrors `tokio::sync::Semaphore`, waking waiters in the order they started waiting.
pub struct Semaphore {
    state: Mutex<State>,
}

impl Semaphore {
    /// The most permits a semaphore can hold.
    pub const MAX_PERMITS: usize = usize::MAX >> 3;

    /// A semaphore with `permits` permits.
    ///
    /// # Panics
    ///
    /// * If `permits` exceeds [`Self::MAX_PERMITS`]
    #[must_use]
    pub const fn new(permits: usize) -> Self {
        assert!(
            permits <= Self::MAX_PERMITS,
            "a semaphore may not have more than MAX_PERMITS permits"
        );
        Self {
            state: Mutex::new(State {
                permits,
                closed: false,
                next: 0,
                queue: BTreeMap::new(),
            }),
        }
    }

    /// A semaphore with `permits` permits, usable in a `static`.
    ///
    /// # Panics
    ///
    /// * If `permits` exceeds [`Self::MAX_PERMITS`]
    #[must_use]
    pub const fn const_new(permits: usize) -> Self {
        Self::new(permits)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The number of permits not held or given to a waiter.
    #[must_use]
    pub fn available_permits(&self) -> usize {
        self.state().permits
    }

    /// Adds `n` permits, giving them to waiters first, oldest first.
    ///
    /// # Panics
    ///
    /// * If the permits available would exceed [`Self::MAX_PERMITS`]
    pub fn add_permits(&self, n: usize) {
        self.release(n);
    }

    /// Removes up to `n` of the available permits, returning how many were removed.
    pub fn forget_permits(&self, n: usize) -> usize {
        let mut state = self.state();
        let forgotten = state.permits.min(n);
        state.permits -= forgotten;
        forgotten
    }

    pub(crate) fn release(&self, permits: usize) {
        if permits == 0 {
            return;
        }
        let wakers = self.state().release(permits);
        wake(wakers);
    }

    /// Take `permits` at once, if they are available.
    pub(crate) fn try_take(&self, permits: usize) -> Result<(), TryAcquireError> {
        assert!(
            permits <= Self::MAX_PERMITS,
            "a semaphore may not have more than MAX_PERMITS permits ({})",
            Self::MAX_PERMITS
        );
        let mut state = self.state();
        if state.closed {
            return Err(TryAcquireError::Closed);
        }
        if state.permits < permits {
            return Err(TryAcquireError::NoPermits);
        }
        state.permits -= permits;
        drop(state);
        Ok(())
    }

    /// Waits for a permit.
    ///
    /// Waiters get permits in the order they started waiting. Cancelling the wait gives up its
    /// place, and returns any permits it was given.
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
        Acquire::new(self, 1).await?;
        Ok(SemaphorePermit {
            semaphore: self,
            permits: 1,
        })
    }

    /// Waits for `n` permits; see [`Self::acquire`].
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub async fn acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, AcquireError> {
        Acquire::new(self, n as usize).await?;
        Ok(SemaphorePermit {
            semaphore: self,
            permits: n,
        })
    }

    /// Takes a permit if one is available now.
    ///
    /// # Errors
    ///
    /// * [`TryAcquireError::Closed`] if the semaphore is closed
    /// * [`TryAcquireError::NoPermits`] if no permit is available
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        self.try_take(1)?;
        Ok(SemaphorePermit {
            semaphore: self,
            permits: 1,
        })
    }

    /// Takes `n` permits if they are available now.
    ///
    /// # Errors
    ///
    /// * [`TryAcquireError::Closed`] if the semaphore is closed
    /// * [`TryAcquireError::NoPermits`] if fewer than `n` permits are available
    pub fn try_acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        self.try_take(n as usize)?;
        Ok(SemaphorePermit {
            semaphore: self,
            permits: n,
        })
    }

    /// Waits for a permit, blocking the calling thread (a simulated thread waits on the run's
    /// scheduler).
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub fn blocking_acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
        block_on(Wait::Lock, self.acquire())
    }

    /// Waits for `n` permits, blocking the calling thread; see [`Self::blocking_acquire`].
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub fn blocking_acquire_many(&self, n: u32) -> Result<SemaphorePermit<'_>, AcquireError> {
        block_on(Wait::Lock, self.acquire_many(n))
    }

    /// Waits for a permit held by the returned value, which owns the semaphore.
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub async fn acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, AcquireError> {
        Acquire::new(&self, 1).await?;
        Ok(OwnedSemaphorePermit {
            semaphore: self,
            permits: 1,
        })
    }

    /// Waits for `n` permits held by the returned value, which owns the semaphore.
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub async fn acquire_many_owned(
        self: Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, AcquireError> {
        Acquire::new(&self, n as usize).await?;
        Ok(OwnedSemaphorePermit {
            semaphore: self,
            permits: n,
        })
    }

    /// Takes a permit if one is available now, held by the returned value.
    ///
    /// # Errors
    ///
    /// * [`TryAcquireError::Closed`] if the semaphore is closed
    /// * [`TryAcquireError::NoPermits`] if no permit is available
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        self.try_take(1)?;
        Ok(OwnedSemaphorePermit {
            semaphore: self,
            permits: 1,
        })
    }

    /// Takes `n` permits if they are available now, held by the returned value.
    ///
    /// # Errors
    ///
    /// * [`TryAcquireError::Closed`] if the semaphore is closed
    /// * [`TryAcquireError::NoPermits`] if fewer than `n` permits are available
    pub fn try_acquire_many_owned(
        self: Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        self.try_take(n as usize)?;
        Ok(OwnedSemaphorePermit {
            semaphore: self,
            permits: n,
        })
    }

    /// [`Self::acquire_owned`], blocking the calling thread.
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub fn blocking_acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, AcquireError> {
        block_on(Wait::Lock, self.acquire_owned())
    }

    /// [`Self::acquire_many_owned`], blocking the calling thread.
    ///
    /// # Errors
    ///
    /// * If the semaphore is closed
    pub fn blocking_acquire_many_owned(
        self: Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, AcquireError> {
        block_on(Wait::Lock, self.acquire_many_owned(n))
    }

    /// Closes the semaphore: every waiter fails, oldest first, and so does every later acquire.
    /// Permits held are still returned when dropped.
    pub fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        let wakers: Vec<Waker> = state
            .queue
            .values_mut()
            .filter_map(|waiter| waiter.waker.take())
            .collect();
        drop(state);
        wake(wakers);
    }

    /// Whether the semaphore is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.state().closed
    }
}

impl fmt::Debug for Semaphore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Semaphore")
            .field("permits", &self.available_permits())
            .finish()
    }
}

/// Waits for `permits` permits of a semaphore. Its place in line is taken on its first pending
/// poll and given up (returning any permits it was given) when it is dropped unfinished.
pub struct Acquire<'a> {
    semaphore: &'a Semaphore,
    permits: usize,
    /// Its id in the queue, once queued, until it completes.
    queued: Option<u64>,
}

impl<'a> Acquire<'a> {
    /// # Panics
    ///
    /// * If `permits` exceeds [`Semaphore::MAX_PERMITS`]
    pub(crate) fn new(semaphore: &'a Semaphore, permits: usize) -> Self {
        assert!(
            permits <= Semaphore::MAX_PERMITS,
            "a semaphore may not have more than MAX_PERMITS permits ({})",
            Semaphore::MAX_PERMITS
        );
        Self {
            semaphore,
            permits,
            queued: None,
        }
    }
}

impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if faults::spurious_pending_now(cx, || {
            this.semaphore.state().ready(this.queued, this.permits)
        }) {
            return Poll::Pending;
        }
        this.semaphore
            .state()
            .poll_acquire(&mut this.queued, this.permits, cx.waker())
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        let Some(id) = self.queued.take() else {
            return;
        };
        let mut state = self.semaphore.state();
        let remaining = state.queue.remove(&id).map_or(0, |waiter| waiter.remaining);
        let given = self.permits - remaining;
        let wakers = if given > 0 {
            state.release(given)
        } else {
            Vec::new()
        };
        drop(state);
        wake(wakers);
    }
}

/// Permits of a [`Semaphore`], returned to it when dropped.
#[must_use]
#[clippy::has_significant_drop]
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
    permits: u32,
}

impl<'a> SemaphorePermit<'a> {
    /// Keeps the permits from being returned: the semaphore has that many fewer for good.
    pub fn forget(mut self) {
        self.permits = 0;
    }

    /// Adds the permits of `other` to these.
    ///
    /// # Panics
    ///
    /// * If the permits are from different semaphores, or their sum overflows
    pub fn merge(&mut self, mut other: Self) {
        assert!(
            std::ptr::eq(self.semaphore, other.semaphore),
            "merging permits from different semaphore instances"
        );
        self.permits = self
            .permits
            .checked_add(other.permits)
            .expect("number of permits overflowed");
        other.permits = 0;
    }

    /// Splits `n` of these permits off into a new value, if there are that many.
    #[must_use]
    pub fn split(&mut self, n: usize) -> Option<Self> {
        let n = u32::try_from(n).ok()?;
        if n > self.permits {
            return None;
        }
        self.permits -= n;
        Some(Self {
            semaphore: self.semaphore,
            permits: n,
        })
    }

    /// The semaphore these permits are from.
    #[must_use]
    pub const fn semaphore(&self) -> &'a Semaphore {
        self.semaphore
    }

    /// How many permits this holds.
    #[must_use]
    pub const fn num_permits(&self) -> usize {
        self.permits as usize
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.semaphore.release(self.permits as usize);
    }
}

/// Permits of a [`Semaphore`] that own it (through an [`Arc`]), returned when dropped.
#[must_use]
#[clippy::has_significant_drop]
#[derive(Debug)]
pub struct OwnedSemaphorePermit {
    semaphore: Arc<Semaphore>,
    permits: u32,
}

impl OwnedSemaphorePermit {
    /// Keeps the permits from being returned: the semaphore has that many fewer for good.
    pub fn forget(mut self) {
        self.permits = 0;
    }

    /// Adds the permits of `other` to these.
    ///
    /// # Panics
    ///
    /// * If the permits are from different semaphores, or their sum overflows
    pub fn merge(&mut self, mut other: Self) {
        assert!(
            Arc::ptr_eq(&self.semaphore, &other.semaphore),
            "merging permits from different semaphore instances"
        );
        self.permits = self
            .permits
            .checked_add(other.permits)
            .expect("number of permits overflowed");
        other.permits = 0;
    }

    /// Splits `n` of these permits off into a new value, if there are that many.
    #[must_use]
    pub fn split(&mut self, n: usize) -> Option<Self> {
        let n = u32::try_from(n).ok()?;
        if n > self.permits {
            return None;
        }
        self.permits -= n;
        Some(Self {
            semaphore: Arc::clone(&self.semaphore),
            permits: n,
        })
    }

    /// The semaphore these permits are from.
    #[must_use]
    pub const fn semaphore(&self) -> &Arc<Semaphore> {
        &self.semaphore
    }

    /// How many permits this holds.
    #[must_use]
    pub const fn num_permits(&self) -> usize {
        self.permits as usize
    }
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        self.semaphore.release(self.permits as usize);
    }
}
