//! Simulated blocking synchronization. Every operation is a switch point.

pub mod atomic;
pub mod mpsc;

use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::{LockResult, PoisonError, TryLockError, TryLockResult},
    time::Duration,
};

use super::{
    Object, Op, ThreadIndex, WaitKey,
    coverage::{self, Site},
    deadline_after, new_key, scheduler, touch,
};

fn std_lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A mutual exclusion lock whose waits are scheduled by the simulator.
pub struct Mutex<T: ?Sized> {
    key: u64,
    owner: std::sync::Mutex<Option<ThreadIndex>>,
    data: std::sync::Mutex<T>,
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mutex")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> Mutex<T> {
    /// A new unlocked mutex.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            key: new_key(),
            owner: std::sync::Mutex::new(None),
            data: std::sync::Mutex::new(value),
        }
    }

    /// The value, consuming the mutex.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock
    pub fn into_inner(self) -> LockResult<T> {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Acquire the lock, blocking the simulated thread until it is free.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock; the guard is still returned
    #[track_caller]
    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        let _site = coverage::enter();
        let scheduler = scheduler();
        let me = scheduler.me();
        loop {
            scheduler.switch_point(Op::Lock);
            if self.try_acquire(me) {
                return self.guard(Site::caller());
            }
            scheduler.wait(Op::Lock, WaitKey::Lock(self.key), None);
        }
    }

    /// Acquire the lock if it is free.
    ///
    /// # Errors
    ///
    /// * [`TryLockError::WouldBlock`] if another thread holds it
    /// * [`TryLockError::Poisoned`] if a thread panicked while holding it
    #[track_caller]
    pub fn try_lock(&self) -> TryLockResult<MutexGuard<'_, T>> {
        let _site = coverage::enter();
        let scheduler = scheduler();
        scheduler.switch_point(Op::Lock);
        if self.try_acquire(scheduler.me()) {
            self.guard(Site::caller()).map_err(TryLockError::Poisoned)
        } else {
            Err(TryLockError::WouldBlock)
        }
    }

    /// Whether a thread panicked while holding the lock.
    pub fn is_poisoned(&self) -> bool {
        self.data.is_poisoned()
    }

    /// A mutable reference to the value; no locking is needed.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock
    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.data.get_mut()
    }

    fn try_acquire(&self, me: ThreadIndex) -> bool {
        touch(Object::Lock(self.key), true);
        let mut owner = std_lock(&self.owner);
        if owner.is_some() {
            return false;
        }
        *owner = Some(me);
        true
    }

    fn guard(&self, site: Site) -> LockResult<MutexGuard<'_, T>> {
        match self.data.lock() {
            Ok(guard) => Ok(MutexGuard {
                mutex: self,
                guard: Some(guard),
                site,
            }),
            Err(poisoned) => Err(PoisonError::new(MutexGuard {
                mutex: self,
                guard: Some(poisoned.into_inner()),
                site,
            })),
        }
    }

    /// Release the lock without a switch point (for condition variables, which must start
    /// waiting before any other thread runs).
    fn release(&self) {
        *std_lock(&self.owner) = None;
        scheduler().wake_all_on(WaitKey::Lock(self.key));
    }
}

/// Holds a [`Mutex`] locked; unlocking on drop is a switch point.
pub struct MutexGuard<'a, T: ?Sized> {
    mutex: &'a Mutex<T>,
    guard: Option<std::sync::MutexGuard<'a, T>>,
    /// Where the lock was taken: the site of its unlocking, for coverage.
    site: Site,
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.guard.as_ref().expect("guard is held until drop")
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.guard.as_mut().expect("guard is held until drop")
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        let _site = coverage::enter_at(self.site);
        drop(self.guard.take());
        self.mutex.release();
        if !std::thread::panicking() {
            scheduler().switch_point(Op::Unlock);
        }
    }
}

impl<'a, T: ?Sized> MutexGuard<'a, T> {
    /// Unlock without a switch point, returning the mutex.
    fn unlock_quietly(mut self) -> &'a Mutex<T> {
        drop(self.guard.take());
        self.mutex.release();
        let mutex = self.mutex;
        std::mem::forget(self);
        mutex
    }
}

/// Whether a timed condition variable wait timed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitTimeoutResult(bool);

impl WaitTimeoutResult {
    /// Whether the wait ended because its timeout elapsed.
    #[must_use]
    pub const fn timed_out(&self) -> bool {
        self.0
    }
}

/// A condition variable whose waits are scheduled by the simulator. There are no spurious
/// wakeups.
pub struct Condvar {
    key: u64,
}

impl fmt::Debug for Condvar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Condvar").field("key", &self.key).finish()
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

impl Condvar {
    /// A new condition variable.
    #[must_use]
    pub fn new() -> Self {
        Self { key: new_key() }
    }

    /// Release `guard`'s lock, wait for a notification, and lock again.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock; the guard is still returned
    #[track_caller]
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> LockResult<MutexGuard<'a, T>> {
        self.wait_until(guard, None).0
    }

    /// Wait while `condition` holds.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock; the guard is still returned
    #[track_caller]
    pub fn wait_while<'a, T, F: FnMut(&mut T) -> bool>(
        &self,
        mut guard: MutexGuard<'a, T>,
        mut condition: F,
    ) -> LockResult<MutexGuard<'a, T>> {
        while condition(&mut *guard) {
            guard = self.wait(guard)?;
        }
        Ok(guard)
    }

    /// Like [`Self::wait`], but for at most `timeout` of simulated time.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock; the guard is still returned
    #[track_caller]
    pub fn wait_timeout<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
    ) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)> {
        let (result, timed_out) = self.wait_until(guard, deadline_after(timeout));
        match result {
            Ok(guard) => Ok((guard, WaitTimeoutResult(timed_out))),
            Err(poisoned) => Err(PoisonError::new((
                poisoned.into_inner(),
                WaitTimeoutResult(timed_out),
            ))),
        }
    }

    /// Like [`Self::wait_while`], but for at most `timeout` of simulated time in all.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the lock; the guard is still returned
    #[track_caller]
    pub fn wait_timeout_while<'a, T, F: FnMut(&mut T) -> bool>(
        &self,
        mut guard: MutexGuard<'a, T>,
        timeout: Duration,
        mut condition: F,
    ) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)> {
        let deadline = deadline_after(timeout);
        loop {
            if !condition(&mut *guard) {
                return Ok((guard, WaitTimeoutResult(false)));
            }
            let (result, timed_out) = self.wait_until(guard, deadline);
            match result {
                Ok(next) if timed_out => {
                    guard = next;
                    let still = condition(&mut *guard);
                    return Ok((guard, WaitTimeoutResult(still)));
                }
                Ok(next) => guard = next,
                Err(poisoned) => {
                    return Err(PoisonError::new((
                        poisoned.into_inner(),
                        WaitTimeoutResult(timed_out),
                    )));
                }
            }
        }
    }

    #[track_caller]
    fn wait_until<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        deadline: Option<std::time::Instant>,
    ) -> (LockResult<MutexGuard<'a, T>>, bool) {
        let _site = coverage::enter();
        let scheduler = scheduler();
        let mutex = guard.unlock_quietly();
        let timed_out = scheduler.wait(Op::CondvarWait, WaitKey::Condvar(self.key), deadline);
        (mutex.lock(), timed_out)
    }

    /// Wake one waiting thread, chosen with the run's seed.
    #[track_caller]
    pub fn notify_one(&self) {
        let _site = coverage::enter();
        let scheduler = scheduler();
        scheduler.wake_one_on(WaitKey::Condvar(self.key));
        scheduler.switch_point(Op::CondvarNotify);
    }

    /// Wake every waiting thread.
    #[track_caller]
    pub fn notify_all(&self) {
        let _site = coverage::enter();
        let scheduler = scheduler();
        scheduler.wake_all_on(WaitKey::Condvar(self.key));
        scheduler.switch_point(Op::CondvarNotify);
    }
}

/// A reader-writer lock whose waits are scheduled by the simulator.
pub struct RwLock<T: ?Sized> {
    key: u64,
    /// Readers holding the lock, or `None` while a writer holds it.
    state: std::sync::Mutex<Option<usize>>,
    data: std::sync::RwLock<T>,
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RwLock")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl<T: Default> Default for RwLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> RwLock<T> {
    /// A new unlocked lock.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            key: new_key(),
            state: std::sync::Mutex::new(Some(0)),
            data: std::sync::RwLock::new(value),
        }
    }

    /// The value, consuming the lock.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the write lock
    pub fn into_inner(self) -> LockResult<T> {
        self.data.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Acquire shared read access.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the write lock; the guard is still returned
    #[track_caller]
    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
        let _site = coverage::enter();
        let scheduler = scheduler();
        loop {
            scheduler.switch_point(Op::Lock);
            scheduler.touch(Object::Lock(self.key), true);
            {
                let mut state = std_lock(&self.state);
                if let Some(readers) = state.as_mut() {
                    *readers += 1;
                    break;
                }
            }
            scheduler.wait(Op::Lock, WaitKey::Lock(self.key), None);
        }
        match self.data.read() {
            Ok(guard) => Ok(RwLockReadGuard {
                lock: self,
                guard: Some(guard),
                site: Site::caller(),
            }),
            Err(poisoned) => Err(PoisonError::new(RwLockReadGuard {
                lock: self,
                guard: Some(poisoned.into_inner()),
                site: Site::caller(),
            })),
        }
    }

    /// Acquire exclusive write access.
    ///
    /// # Errors
    ///
    /// * If a thread panicked while holding the write lock; the guard is still returned
    #[track_caller]
    pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
        let _site = coverage::enter();
        let scheduler = scheduler();
        loop {
            scheduler.switch_point(Op::Lock);
            scheduler.touch(Object::Lock(self.key), true);
            {
                let mut state = std_lock(&self.state);
                if *state == Some(0) {
                    *state = None;
                    break;
                }
            }
            scheduler.wait(Op::Lock, WaitKey::Lock(self.key), None);
        }
        match self.data.write() {
            Ok(guard) => Ok(RwLockWriteGuard {
                lock: self,
                guard: Some(guard),
                site: Site::caller(),
            }),
            Err(poisoned) => Err(PoisonError::new(RwLockWriteGuard {
                lock: self,
                guard: Some(poisoned.into_inner()),
                site: Site::caller(),
            })),
        }
    }

    /// Whether a thread panicked while holding the write lock.
    pub fn is_poisoned(&self) -> bool {
        self.data.is_poisoned()
    }
}

/// Shared access to a [`RwLock`].
pub struct RwLockReadGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    guard: Option<std::sync::RwLockReadGuard<'a, T>>,
    /// Where the lock was taken: the site of its unlocking, for coverage.
    site: Site,
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.guard.as_ref().expect("guard is held until drop")
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        let _site = coverage::enter_at(self.site);
        drop(self.guard.take());
        {
            let mut state = std_lock(&self.lock.state);
            if let Some(readers) = state.as_mut() {
                *readers -= 1;
            }
        }
        let scheduler = scheduler();
        scheduler.wake_all_on(WaitKey::Lock(self.lock.key));
        if !std::thread::panicking() {
            scheduler.switch_point(Op::Unlock);
        }
    }
}

/// Exclusive access to a [`RwLock`].
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    guard: Option<std::sync::RwLockWriteGuard<'a, T>>,
    /// Where the lock was taken: the site of its unlocking, for coverage.
    site: Site,
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.guard.as_ref().expect("guard is held until drop")
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.guard.as_mut().expect("guard is held until drop")
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        let _site = coverage::enter_at(self.site);
        drop(self.guard.take());
        *std_lock(&self.lock.state) = Some(0);
        let scheduler = scheduler();
        scheduler.wake_all_on(WaitKey::Lock(self.lock.key));
        if !std::thread::panicking() {
            scheduler.switch_point(Op::Unlock);
        }
    }
}
