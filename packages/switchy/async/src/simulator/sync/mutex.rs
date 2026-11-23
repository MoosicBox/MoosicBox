//! An async mutex with tokio's API whose waiters are served first in, first out.
//!
//! As tokio's, it is a [`Semaphore`] with one permit around the value: a guard holds the
//! permit, and waiting tasks get the lock in the order they started waiting.

use std::{
    cell::UnsafeCell,
    error::Error,
    fmt,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use super::{
    blocking::{Wait, block_on},
    semaphore::{Acquire, Semaphore},
};

/// An async mutual exclusion lock, held across `.await`s by its guards.
///
/// Mirrors `tokio::sync::Mutex`: tasks waiting for it get it in the order they started
/// waiting, and a cancelled wait gives up its place.
pub struct Mutex<T: ?Sized> {
    semaphore: Semaphore,
    value: UnsafeCell<T>,
}

// SAFETY: as tokio's: the value is reached only through a guard, which holds the only permit,
// so it is shared between threads only as a `T` moved or lent exclusively.
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
// SAFETY: see above.
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}
// SAFETY: a shared guard gives `&T` only.
unsafe impl<T: ?Sized + Send + Sync> Sync for MutexGuard<'_, T> {}
// SAFETY: a shared guard gives `&T` only.
unsafe impl<T: ?Sized + Send + Sync> Sync for OwnedMutexGuard<T> {}
// SAFETY: the guard holds the permit for exclusive access to the mapped value.
unsafe impl<'a, T: ?Sized + Sync + 'a> Sync for MappedMutexGuard<'a, T> {}
// SAFETY: the guard holds the permit for exclusive access to the mapped value.
unsafe impl<'a, T: ?Sized + Send + 'a> Send for MappedMutexGuard<'a, T> {}
// SAFETY: the guard holds the permit for exclusive access to the mapped value.
unsafe impl<T: ?Sized + Send + Sync, U: ?Sized + Send + Sync> Sync for OwnedMappedMutexGuard<T, U> {}
// SAFETY: the guard holds the permit for exclusive access to the mapped value.
unsafe impl<T: ?Sized + Send, U: ?Sized + Send> Send for OwnedMappedMutexGuard<T, U> {}

/// Error returned from `try_lock` (and [`super::RwLock`]'s `try_read` and `try_write`) when
/// the lock is held.
#[derive(Debug)]
pub struct TryLockError(pub(super) ());

impl fmt::Display for TryLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "operation would block")
    }
}

impl Error for TryLockError {}

impl<T: ?Sized> Mutex<T> {
    /// A new, unlocked mutex holding `value`.
    #[must_use]
    pub const fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            semaphore: Semaphore::new(1),
            value: UnsafeCell::new(value),
        }
    }

    /// A new, unlocked mutex holding `value`, usable in a `static`.
    #[must_use]
    pub const fn const_new(value: T) -> Self
    where
        T: Sized,
    {
        Self::new(value)
    }

    /// Waits for the lock.
    ///
    /// Waiters get the lock in the order they started waiting. Cancelling the wait gives up its
    /// place.
    // Generic: `Send` whenever `T: Send`, as with tokio's mutex.
    #[allow(clippy::future_not_send)]
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        acquire(&self.semaphore).await;
        MutexGuard { lock: self }
    }

    /// Waits for the lock, blocking the calling thread (a simulated thread waits on the run's
    /// scheduler).
    pub fn blocking_lock(&self) -> MutexGuard<'_, T> {
        block_on(Wait::Lock, self.lock())
    }

    /// Waits for the lock, blocking the calling thread, for a guard that owns the mutex.
    pub fn blocking_lock_owned(self: Arc<Self>) -> OwnedMutexGuard<T> {
        block_on(Wait::Lock, self.lock_owned())
    }

    /// Waits for the lock, for a guard that owns the mutex (through an [`Arc`]).
    // Generic: `Send` whenever `T: Send`, as with tokio's mutex.
    #[allow(clippy::future_not_send)]
    pub async fn lock_owned(self: Arc<Self>) -> OwnedMutexGuard<T> {
        acquire(&self.semaphore).await;
        OwnedMutexGuard { lock: self }
    }

    /// Takes the lock if it is free now.
    ///
    /// # Errors
    ///
    /// * If the lock is held
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, TryLockError> {
        match self.semaphore.try_take(1) {
            Ok(()) => Ok(MutexGuard { lock: self }),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Takes the lock if it is free now, for a guard that owns the mutex.
    ///
    /// # Errors
    ///
    /// * If the lock is held
    pub fn try_lock_owned(self: Arc<Self>) -> Result<OwnedMutexGuard<T>, TryLockError> {
        match self.semaphore.try_take(1) {
            Ok(()) => Ok(OwnedMutexGuard { lock: self }),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// The value, through exclusive access to the mutex (no locking needed).
    pub const fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    /// The value, consuming the mutex.
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.value.into_inner()
    }
}

/// Waits for the mutex's only permit.
async fn acquire(semaphore: &Semaphore) {
    Acquire::new(semaphore, 1)
        .await
        .unwrap_or_else(|_| unreachable!("a mutex's semaphore is never closed"));
}

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Mutex");
        match self.try_lock() {
            Ok(inner) => d.field("data", &&*inner),
            Err(_) => d.field("data", &format_args!("<locked>")),
        };
        d.finish()
    }
}

/// Exclusive access to a [`Mutex`]'s value, unlocking it when dropped.
#[must_use = "if unused the Mutex will immediately unlock"]
#[clippy::has_significant_drop]
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}

impl<'a, T: ?Sized> MutexGuard<'a, T> {
    /// A guard of part of the value, holding the lock.
    pub fn map<U: ?Sized, F>(mut this: Self, f: F) -> MappedMutexGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        let semaphore = &this.lock.semaphore;
        std::mem::forget(this);
        MappedMutexGuard {
            semaphore,
            data,
            marker: PhantomData,
        }
    }

    /// A guard of part of the value if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<U: ?Sized, F>(mut this: Self, f: F) -> Result<MappedMutexGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        let semaphore = &this.lock.semaphore;
        std::mem::forget(this);
        Ok(MappedMutexGuard {
            semaphore,
            data,
            marker: PhantomData,
        })
    }

    /// The mutex this guard locks.
    #[must_use]
    pub const fn mutex(this: &Self) -> &'a Mutex<T> {
        this.lock
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.semaphore.release(1);
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the mutex's only permit.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the mutex's only permit.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

/// Exclusive access to a [`Mutex`]'s value that owns the mutex (through an [`Arc`]),
/// unlocking it when dropped.
#[must_use = "if unused the Mutex will immediately unlock"]
#[clippy::has_significant_drop]
pub struct OwnedMutexGuard<T: ?Sized> {
    lock: Arc<Mutex<T>>,
}

impl<T: ?Sized> OwnedMutexGuard<T> {
    /// The mutex, without unlocking it.
    fn into_lock(self) -> Arc<Mutex<T>> {
        let this = ManuallyDrop::new(self);
        // SAFETY: read once from a guard that is never dropped.
        unsafe { std::ptr::read(&raw const this.lock) }
    }

    /// A guard of part of the value, holding the lock.
    pub fn map<U: ?Sized, F>(mut this: Self, f: F) -> OwnedMappedMutexGuard<T, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        OwnedMappedMutexGuard {
            data,
            lock: this.into_lock(),
        }
    }

    /// A guard of part of the value if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<U: ?Sized, F>(mut this: Self, f: F) -> Result<OwnedMappedMutexGuard<T, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        Ok(OwnedMappedMutexGuard {
            data,
            lock: this.into_lock(),
        })
    }

    /// The mutex this guard locks.
    #[must_use]
    pub const fn mutex(this: &Self) -> &Arc<Mutex<T>> {
        &this.lock
    }
}

impl<T: ?Sized> Drop for OwnedMutexGuard<T> {
    fn drop(&mut self) {
        self.lock.semaphore.release(1);
    }
}

impl<T: ?Sized> Deref for OwnedMutexGuard<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the mutex's only permit.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for OwnedMutexGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the mutex's only permit.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for OwnedMutexGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for OwnedMutexGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

/// Exclusive access to part of a [`Mutex`]'s value (see [`MutexGuard::map`]), unlocking the
/// mutex when dropped.
#[must_use = "if unused the Mutex will immediately unlock"]
#[clippy::has_significant_drop]
pub struct MappedMutexGuard<'a, T: ?Sized> {
    semaphore: &'a Semaphore,
    data: *mut T,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> MappedMutexGuard<'a, T> {
    /// A guard of part of this part, holding the lock.
    pub fn map<U: ?Sized, F>(mut this: Self, f: F) -> MappedMutexGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        let semaphore = this.semaphore;
        std::mem::forget(this);
        MappedMutexGuard {
            semaphore,
            data,
            marker: PhantomData,
        }
    }

    /// A guard of part of this part if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<U: ?Sized, F>(mut this: Self, f: F) -> Result<MappedMutexGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        let semaphore = this.semaphore;
        std::mem::forget(this);
        Ok(MappedMutexGuard {
            semaphore,
            data,
            marker: PhantomData,
        })
    }
}

impl<T: ?Sized> Drop for MappedMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.semaphore.release(1);
    }
}

impl<T: ?Sized> Deref for MappedMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the mutex's only permit, and `data` points into its value.
        unsafe { &*self.data }
    }
}

impl<T: ?Sized> DerefMut for MappedMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the mutex's only permit, and `data` points into its value.
        unsafe { &mut *self.data }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MappedMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MappedMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

/// Exclusive access to part of a [`Mutex`]'s value that owns the mutex (see
/// [`OwnedMutexGuard::map`]), unlocking it when dropped.
#[must_use = "if unused the Mutex will immediately unlock"]
#[clippy::has_significant_drop]
pub struct OwnedMappedMutexGuard<T: ?Sized, U: ?Sized = T> {
    data: *mut U,
    lock: Arc<Mutex<T>>,
}

impl<T: ?Sized, U: ?Sized> OwnedMappedMutexGuard<T, U> {
    /// The mutex, without unlocking it.
    fn into_lock(self) -> Arc<Mutex<T>> {
        let this = ManuallyDrop::new(self);
        // SAFETY: read once from a guard that is never dropped.
        unsafe { std::ptr::read(&raw const this.lock) }
    }

    /// A guard of part of this part, holding the lock.
    pub fn map<S: ?Sized, F>(mut this: Self, f: F) -> OwnedMappedMutexGuard<T, S>
    where
        F: FnOnce(&mut U) -> &mut S,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        OwnedMappedMutexGuard {
            data,
            lock: this.into_lock(),
        }
    }

    /// A guard of part of this part if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<S: ?Sized, F>(mut this: Self, f: F) -> Result<OwnedMappedMutexGuard<T, S>, Self>
    where
        F: FnOnce(&mut U) -> Option<&mut S>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        Ok(OwnedMappedMutexGuard {
            data,
            lock: this.into_lock(),
        })
    }
}

impl<T: ?Sized, U: ?Sized> Drop for OwnedMappedMutexGuard<T, U> {
    fn drop(&mut self) {
        self.lock.semaphore.release(1);
    }
}

impl<T: ?Sized, U: ?Sized> Deref for OwnedMappedMutexGuard<T, U> {
    type Target = U;

    fn deref(&self) -> &U {
        // SAFETY: the guard holds the mutex's only permit, and `data` points into its value.
        unsafe { &*self.data }
    }
}

impl<T: ?Sized, U: ?Sized> DerefMut for OwnedMappedMutexGuard<T, U> {
    fn deref_mut(&mut self) -> &mut U {
        // SAFETY: the guard holds the mutex's only permit, and `data` points into its value.
        unsafe { &mut *self.data }
    }
}

impl<T: ?Sized, U: ?Sized + fmt::Debug> fmt::Debug for OwnedMappedMutexGuard<T, U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized, U: ?Sized + fmt::Display> fmt::Display for OwnedMappedMutexGuard<T, U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
