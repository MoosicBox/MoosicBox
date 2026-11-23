//! An async reader-writer lock with tokio's API whose waiters are served first in, first out.
//!
//! As tokio's, it is a [`Semaphore`] with one permit per reader allowed at once: a reader takes
//! one, a writer takes them all. The semaphore is fair, so a waiting writer is not overtaken by
//! readers arriving after it, and waiting tasks get the lock in the order they started waiting.

use std::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use super::{
    TryLockError,
    blocking::{Wait, block_on},
    semaphore::{Acquire, Semaphore},
};

/// The most readers a lock allows at once.
const MAX_READS: u32 = u32::MAX >> 3;

/// An async reader-writer lock: many readers or one writer at a time.
///
/// Mirrors `tokio::sync::RwLock`: tasks waiting for it get it in the order they started
/// waiting, and a cancelled wait gives up its place.
pub struct RwLock<T: ?Sized> {
    max_reads: u32,
    semaphore: Semaphore,
    value: UnsafeCell<T>,
}

// SAFETY: as tokio's: the value is reached only through guards holding the semaphore's permits:
// one writer, or readers sharing `&T`.
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
// SAFETY: see above.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}
// SAFETY: a read guard gives `&T` only.
unsafe impl<T: ?Sized + Sync> Send for RwLockReadGuard<'_, T> {}
// SAFETY: a read guard gives `&T` only.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockReadGuard<'_, T> {}
// SAFETY: an owned read guard gives `&U` only, but can drop the lock (and its `T`).
unsafe impl<T: ?Sized + Send + Sync, U: ?Sized + Sync> Send for OwnedRwLockReadGuard<T, U> {}
// SAFETY: see above.
unsafe impl<T: ?Sized + Send + Sync, U: ?Sized + Send + Sync> Sync for OwnedRwLockReadGuard<T, U> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockWriteGuard<'_, T> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync> Sync for OwnedRwLockWriteGuard<T> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockMappedWriteGuard<'_, T> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync, U: ?Sized + Send + Sync> Sync
    for OwnedRwLockMappedWriteGuard<T, U>
{
}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync> Send for RwLockWriteGuard<'_, T> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync> Send for OwnedRwLockWriteGuard<T> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync> Send for RwLockMappedWriteGuard<'_, T> {}
// SAFETY: a write guard holds every permit.
unsafe impl<T: ?Sized + Send + Sync, U: ?Sized + Send + Sync> Send
    for OwnedRwLockMappedWriteGuard<T, U>
{
}

/// Waits for `permits` of the lock's semaphore, which is never closed.
async fn acquire(semaphore: &Semaphore, permits: u32) {
    Acquire::new(semaphore, permits as usize)
        .await
        .unwrap_or_else(|_| unreachable!("a lock's semaphore is never closed"));
}

impl<T: ?Sized> RwLock<T> {
    /// A new, unlocked lock holding `value`.
    #[must_use]
    pub const fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self::const_with_max_readers(value, MAX_READS)
    }

    /// A new, unlocked lock holding `value` that allows at most `max_reads` readers at once.
    ///
    /// # Panics
    ///
    /// * If `max_reads` is 0 or above `u32::MAX >> 3`
    #[must_use]
    pub fn with_max_readers(value: T, max_reads: u32) -> Self
    where
        T: Sized,
    {
        assert_ne!(max_reads, 0, "a RwLock may not be created with 0 readers");
        assert!(
            max_reads <= MAX_READS,
            "a RwLock may not be created with more than {MAX_READS} readers"
        );
        Self::const_with_max_readers(value, max_reads)
    }

    /// A new, unlocked lock holding `value`, usable in a `static`.
    #[must_use]
    pub const fn const_new(value: T) -> Self
    where
        T: Sized,
    {
        Self::new(value)
    }

    /// [`Self::with_max_readers`], usable in a `static`.
    ///
    /// # Panics
    ///
    /// * If `max_reads` is 0 or above `u32::MAX >> 3`
    #[must_use]
    pub const fn const_with_max_readers(value: T, max_reads: u32) -> Self
    where
        T: Sized,
    {
        assert!(max_reads != 0, "a RwLock may not be created with 0 readers");
        assert!(max_reads <= MAX_READS);
        Self {
            max_reads,
            semaphore: Semaphore::new(max_reads as usize),
            value: UnsafeCell::new(value),
        }
    }

    const fn read_guard(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard {
            semaphore: &self.semaphore,
            data: self.value.get(),
            marker: PhantomData,
        }
    }

    const fn write_guard(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard {
            permits: self.max_reads,
            semaphore: &self.semaphore,
            data: self.value.get(),
            marker: PhantomData,
        }
    }

    /// Waits for shared read access.
    ///
    /// Waiters get the lock in the order they started waiting, so a reader waits behind a
    /// writer that started waiting first. Cancelling the wait gives up its place.
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's lock.
    #[allow(clippy::future_not_send)]
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        acquire(&self.semaphore, 1).await;
        self.read_guard()
    }

    /// [`Self::read`], blocking the calling thread (a simulated thread waits on the run's
    /// scheduler).
    pub fn blocking_read(&self) -> RwLockReadGuard<'_, T> {
        block_on(Wait::Lock, self.read())
    }

    /// Waits for shared read access, for a guard that owns the lock (through an [`Arc`]).
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's lock.
    #[allow(clippy::future_not_send)]
    pub async fn read_owned(self: Arc<Self>) -> OwnedRwLockReadGuard<T> {
        acquire(&self.semaphore, 1).await;
        let data = self.value.get().cast_const();
        OwnedRwLockReadGuard {
            lock: self,
            data,
            marker: PhantomData,
        }
    }

    /// Takes shared read access if it is free now.
    ///
    /// # Errors
    ///
    /// * If a writer holds the lock or waits for it
    pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, TryLockError> {
        match self.semaphore.try_take(1) {
            Ok(()) => Ok(self.read_guard()),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Takes shared read access if it is free now, for a guard that owns the lock.
    ///
    /// # Errors
    ///
    /// * If a writer holds the lock or waits for it
    pub fn try_read_owned(self: Arc<Self>) -> Result<OwnedRwLockReadGuard<T>, TryLockError> {
        match self.semaphore.try_take(1) {
            Ok(()) => {
                let data = self.value.get().cast_const();
                Ok(OwnedRwLockReadGuard {
                    lock: self,
                    data,
                    marker: PhantomData,
                })
            }
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Waits for exclusive write access.
    ///
    /// Waiters get the lock in the order they started waiting. Cancelling the wait gives up its
    /// place.
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's lock.
    #[allow(clippy::future_not_send)]
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        acquire(&self.semaphore, self.max_reads).await;
        self.write_guard()
    }

    /// [`Self::write`], blocking the calling thread (a simulated thread waits on the run's
    /// scheduler).
    pub fn blocking_write(&self) -> RwLockWriteGuard<'_, T> {
        block_on(Wait::Lock, self.write())
    }

    /// Waits for exclusive write access, for a guard that owns the lock (through an [`Arc`]).
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's lock.
    #[allow(clippy::future_not_send)]
    pub async fn write_owned(self: Arc<Self>) -> OwnedRwLockWriteGuard<T> {
        acquire(&self.semaphore, self.max_reads).await;
        let data = self.value.get();
        OwnedRwLockWriteGuard {
            permits: self.max_reads,
            lock: self,
            data,
        }
    }

    /// Takes exclusive write access if it is free now.
    ///
    /// # Errors
    ///
    /// * If the lock is held
    pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, TryLockError> {
        match self.semaphore.try_take(self.max_reads as usize) {
            Ok(()) => Ok(self.write_guard()),
            Err(_) => Err(TryLockError(())),
        }
    }

    /// Takes exclusive write access if it is free now, for a guard that owns the lock.
    ///
    /// # Errors
    ///
    /// * If the lock is held
    pub fn try_write_owned(self: Arc<Self>) -> Result<OwnedRwLockWriteGuard<T>, TryLockError> {
        match self.semaphore.try_take(self.max_reads as usize) {
            Ok(()) => {
                let data = self.value.get();
                Ok(OwnedRwLockWriteGuard {
                    permits: self.max_reads,
                    lock: self,
                    data,
                })
            }
            Err(_) => Err(TryLockError(())),
        }
    }

    /// The value, through exclusive access to the lock (no locking needed).
    pub const fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    /// The value, consuming the lock.
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.value.into_inner()
    }
}

impl<T> From<T> for RwLock<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Default> Default for RwLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("RwLock");
        match self.try_read() {
            Ok(inner) => d.field("data", &&*inner),
            Err(_) => d.field("data", &format_args!("<locked>")),
        };
        d.finish()
    }
}

/// Implements `Deref` (and, for `mut`, `DerefMut`), `Debug` and `Display` for a guard whose
/// `data` points at its target.
macro_rules! guard_traits {
    ([$($generics:tt)*] $guard:ty => mut $target:ty) => {
        guard_traits!([$($generics)*] $guard => $target);

        impl<$($generics)*> DerefMut for $guard {
            fn deref_mut(&mut self) -> &mut $target {
                // SAFETY: the guard holds every permit, and `data` points into the lock's value.
                unsafe { &mut *self.data }
            }
        }
    };
    ([$($generics:tt)*] $guard:ty => $target:ty) => {
        impl<$($generics)*> Deref for $guard {
            type Target = $target;

            fn deref(&self) -> &$target {
                // SAFETY: the guard holds the permits its access needs, and `data` points into
                // the lock's value.
                unsafe { &*self.data }
            }
        }

        impl<$($generics)*> fmt::Debug for $guard
        where
            $target: fmt::Debug,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(&**self, f)
            }
        }

        impl<$($generics)*> fmt::Display for $guard
        where
            $target: fmt::Display,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&**self, f)
            }
        }
    };
}

/// Shared read access to a [`RwLock`]'s value (or part of it), released when dropped.
#[must_use = "if unused the RwLock will immediately unlock"]
#[clippy::has_significant_drop]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    semaphore: &'a Semaphore,
    data: *const T,
    marker: PhantomData<&'a T>,
}

impl<'a, T: ?Sized> RwLockReadGuard<'a, T> {
    /// A guard of part of the value, holding the read access.
    pub fn map<F, U: ?Sized>(this: Self, f: F) -> RwLockReadGuard<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        let data = std::ptr::from_ref(f(&*this));
        let semaphore = this.semaphore;
        std::mem::forget(this);
        RwLockReadGuard {
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
    pub fn try_map<F, U: ?Sized>(this: Self, f: F) -> Result<RwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        let Some(data) = f(&*this).map(std::ptr::from_ref) else {
            return Err(this);
        };
        let semaphore = this.semaphore;
        std::mem::forget(this);
        Ok(RwLockReadGuard {
            semaphore,
            data,
            marker: PhantomData,
        })
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.semaphore.release(1);
    }
}

guard_traits!([T: ?Sized] RwLockReadGuard<'_, T> => T);

/// Exclusive write access to a [`RwLock`]'s value, released when dropped.
#[must_use = "if unused the RwLock will immediately unlock"]
#[clippy::has_significant_drop]
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    permits: u32,
    semaphore: &'a Semaphore,
    data: *mut T,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> RwLockWriteGuard<'a, T> {
    /// The guard's permits and lock, without releasing them.
    const fn into_parts(self) -> (u32, &'a Semaphore) {
        let parts = (self.permits, self.semaphore);
        std::mem::forget(self);
        parts
    }

    /// A guard of part of the value, holding the write access.
    pub fn map<F, U: ?Sized>(mut this: Self, f: F) -> RwLockMappedWriteGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        let (permits, semaphore) = this.into_parts();
        RwLockMappedWriteGuard {
            permits,
            semaphore,
            data,
            marker: PhantomData,
        }
    }

    /// Releases the write access for read access to part of the value, atomically (no writer
    /// can take the lock in between).
    pub fn downgrade_map<F, U: ?Sized>(this: Self, f: F) -> RwLockReadGuard<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        let data = std::ptr::from_ref(f(&*this));
        let (permits, semaphore) = this.into_parts();
        semaphore.release(permits as usize - 1);
        RwLockReadGuard {
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
    pub fn try_map<F, U: ?Sized>(mut this: Self, f: F) -> Result<RwLockMappedWriteGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        let (permits, semaphore) = this.into_parts();
        Ok(RwLockMappedWriteGuard {
            permits,
            semaphore,
            data,
            marker: PhantomData,
        })
    }

    /// [`Self::downgrade_map`] if `f` returns part of the value, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_downgrade_map<F, U: ?Sized>(this: Self, f: F) -> Result<RwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        let Some(data) = f(&*this).map(std::ptr::from_ref) else {
            return Err(this);
        };
        let (permits, semaphore) = this.into_parts();
        semaphore.release(permits as usize - 1);
        Ok(RwLockReadGuard {
            semaphore,
            data,
            marker: PhantomData,
        })
    }

    /// This guard as a mapped one of the whole value.
    pub fn into_mapped(this: Self) -> RwLockMappedWriteGuard<'a, T> {
        Self::map(this, |value| value)
    }

    /// Releases the write access for read access, atomically (no writer can take the lock in
    /// between).
    pub fn downgrade(self) -> RwLockReadGuard<'a, T> {
        let data = self.data.cast_const();
        let (permits, semaphore) = self.into_parts();
        semaphore.release(permits as usize - 1);
        RwLockReadGuard {
            semaphore,
            data,
            marker: PhantomData,
        }
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.semaphore.release(self.permits as usize);
    }
}

guard_traits!([T: ?Sized] RwLockWriteGuard<'_, T> => mut T);

/// Exclusive write access to part of a [`RwLock`]'s value (see [`RwLockWriteGuard::map`]),
/// released when dropped.
#[must_use = "if unused the RwLock will immediately unlock"]
#[clippy::has_significant_drop]
pub struct RwLockMappedWriteGuard<'a, T: ?Sized> {
    permits: u32,
    semaphore: &'a Semaphore,
    data: *mut T,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> RwLockMappedWriteGuard<'a, T> {
    const fn into_parts(self) -> (u32, &'a Semaphore) {
        let parts = (self.permits, self.semaphore);
        std::mem::forget(self);
        parts
    }

    /// A guard of part of this part, holding the write access.
    pub fn map<F, U: ?Sized>(mut this: Self, f: F) -> RwLockMappedWriteGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        let (permits, semaphore) = this.into_parts();
        RwLockMappedWriteGuard {
            permits,
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
    pub fn try_map<F, U: ?Sized>(mut this: Self, f: F) -> Result<RwLockMappedWriteGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        let (permits, semaphore) = this.into_parts();
        Ok(RwLockMappedWriteGuard {
            permits,
            semaphore,
            data,
            marker: PhantomData,
        })
    }
}

impl<T: ?Sized> Drop for RwLockMappedWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.semaphore.release(self.permits as usize);
    }
}

guard_traits!([T: ?Sized] RwLockMappedWriteGuard<'_, T> => mut T);

/// Shared read access to a [`RwLock`]'s value (or part of it) that owns the lock (through an
/// [`Arc`]), released when dropped.
#[must_use = "if unused the RwLock will immediately unlock"]
#[clippy::has_significant_drop]
pub struct OwnedRwLockReadGuard<T: ?Sized, U: ?Sized = T> {
    lock: Arc<RwLock<T>>,
    data: *const U,
    marker: PhantomData<T>,
}

impl<T: ?Sized, U: ?Sized> OwnedRwLockReadGuard<T, U> {
    /// The lock, without releasing the read access.
    fn into_lock(self) -> Arc<RwLock<T>> {
        let this = ManuallyDrop::new(self);
        // SAFETY: read once from a guard that is never dropped.
        unsafe { std::ptr::read(&raw const this.lock) }
    }

    /// A guard of part of the value, holding the read access.
    pub fn map<F, V: ?Sized>(this: Self, f: F) -> OwnedRwLockReadGuard<T, V>
    where
        F: FnOnce(&U) -> &V,
    {
        let data = std::ptr::from_ref(f(&*this));
        OwnedRwLockReadGuard {
            lock: this.into_lock(),
            data,
            marker: PhantomData,
        }
    }

    /// A guard of part of the value if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<F, V: ?Sized>(this: Self, f: F) -> Result<OwnedRwLockReadGuard<T, V>, Self>
    where
        F: FnOnce(&U) -> Option<&V>,
    {
        let Some(data) = f(&*this).map(std::ptr::from_ref) else {
            return Err(this);
        };
        Ok(OwnedRwLockReadGuard {
            lock: this.into_lock(),
            data,
            marker: PhantomData,
        })
    }

    /// The lock this guard holds.
    #[must_use]
    pub const fn rwlock(this: &Self) -> &Arc<RwLock<T>> {
        &this.lock
    }
}

impl<T: ?Sized, U: ?Sized> Drop for OwnedRwLockReadGuard<T, U> {
    fn drop(&mut self) {
        self.lock.semaphore.release(1);
    }
}

guard_traits!([T: ?Sized, U: ?Sized] OwnedRwLockReadGuard<T, U> => U);

/// Exclusive write access to a [`RwLock`]'s value that owns the lock (through an [`Arc`]),
/// released when dropped.
#[must_use = "if unused the RwLock will immediately unlock"]
#[clippy::has_significant_drop]
pub struct OwnedRwLockWriteGuard<T: ?Sized> {
    permits: u32,
    lock: Arc<RwLock<T>>,
    data: *mut T,
}

impl<T: ?Sized> OwnedRwLockWriteGuard<T> {
    /// The guard's permits and lock, without releasing them.
    fn into_parts(self) -> (u32, Arc<RwLock<T>>) {
        let this = ManuallyDrop::new(self);
        // SAFETY: read once from a guard that is never dropped.
        (this.permits, unsafe { std::ptr::read(&raw const this.lock) })
    }

    /// A guard of part of the value, holding the write access.
    pub fn map<F, U: ?Sized>(mut this: Self, f: F) -> OwnedRwLockMappedWriteGuard<T, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        let (permits, lock) = this.into_parts();
        OwnedRwLockMappedWriteGuard {
            permits,
            lock,
            data,
        }
    }

    /// Releases the write access for read access to part of the value, atomically.
    pub fn downgrade_map<F, U: ?Sized>(this: Self, f: F) -> OwnedRwLockReadGuard<T, U>
    where
        F: FnOnce(&T) -> &U,
    {
        let data = std::ptr::from_ref(f(&*this));
        let (permits, lock) = this.into_parts();
        lock.semaphore.release(permits as usize - 1);
        OwnedRwLockReadGuard {
            lock,
            data,
            marker: PhantomData,
        }
    }

    /// A guard of part of the value if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<F, U: ?Sized>(
        mut this: Self,
        f: F,
    ) -> Result<OwnedRwLockMappedWriteGuard<T, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        let (permits, lock) = this.into_parts();
        Ok(OwnedRwLockMappedWriteGuard {
            permits,
            lock,
            data,
        })
    }

    /// [`Self::downgrade_map`] if `f` returns part of the value, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_downgrade_map<F, U: ?Sized>(
        this: Self,
        f: F,
    ) -> Result<OwnedRwLockReadGuard<T, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        let Some(data) = f(&*this).map(std::ptr::from_ref) else {
            return Err(this);
        };
        let (permits, lock) = this.into_parts();
        lock.semaphore.release(permits as usize - 1);
        Ok(OwnedRwLockReadGuard {
            lock,
            data,
            marker: PhantomData,
        })
    }

    /// This guard as a mapped one of the whole value.
    pub fn into_mapped(this: Self) -> OwnedRwLockMappedWriteGuard<T> {
        Self::map(this, |value| value)
    }

    /// Releases the write access for read access, atomically (no writer can take the lock in
    /// between).
    pub fn downgrade(self) -> OwnedRwLockReadGuard<T> {
        let data = self.data.cast_const();
        let (permits, lock) = self.into_parts();
        lock.semaphore.release(permits as usize - 1);
        OwnedRwLockReadGuard {
            lock,
            data,
            marker: PhantomData,
        }
    }

    /// The lock this guard holds.
    #[must_use]
    pub const fn rwlock(this: &Self) -> &Arc<RwLock<T>> {
        &this.lock
    }
}

impl<T: ?Sized> Drop for OwnedRwLockWriteGuard<T> {
    fn drop(&mut self) {
        self.lock.semaphore.release(self.permits as usize);
    }
}

guard_traits!([T: ?Sized] OwnedRwLockWriteGuard<T> => mut T);

/// Exclusive write access to part of a [`RwLock`]'s value that owns the lock (see
/// [`OwnedRwLockWriteGuard::map`]), released when dropped.
#[must_use = "if unused the RwLock will immediately unlock"]
#[clippy::has_significant_drop]
pub struct OwnedRwLockMappedWriteGuard<T: ?Sized, U: ?Sized = T> {
    permits: u32,
    lock: Arc<RwLock<T>>,
    data: *mut U,
}

impl<T: ?Sized, U: ?Sized> OwnedRwLockMappedWriteGuard<T, U> {
    fn into_parts(self) -> (u32, Arc<RwLock<T>>) {
        let this = ManuallyDrop::new(self);
        // SAFETY: read once from a guard that is never dropped.
        (this.permits, unsafe { std::ptr::read(&raw const this.lock) })
    }

    /// A guard of part of this part, holding the write access.
    pub fn map<F, V: ?Sized>(mut this: Self, f: F) -> OwnedRwLockMappedWriteGuard<T, V>
    where
        F: FnOnce(&mut U) -> &mut V,
    {
        let data = std::ptr::from_mut(f(&mut *this));
        let (permits, lock) = this.into_parts();
        OwnedRwLockMappedWriteGuard {
            permits,
            lock,
            data,
        }
    }

    /// A guard of part of this part if `f` returns it, else this guard.
    ///
    /// # Errors
    ///
    /// * This guard, if `f` returns `None`
    pub fn try_map<F, V: ?Sized>(
        mut this: Self,
        f: F,
    ) -> Result<OwnedRwLockMappedWriteGuard<T, V>, Self>
    where
        F: FnOnce(&mut U) -> Option<&mut V>,
    {
        let Some(data) = f(&mut *this).map(std::ptr::from_mut) else {
            return Err(this);
        };
        let (permits, lock) = this.into_parts();
        Ok(OwnedRwLockMappedWriteGuard {
            permits,
            lock,
            data,
        })
    }

    /// The lock this guard holds.
    #[must_use]
    pub const fn rwlock(this: &Self) -> &Arc<RwLock<T>> {
        &this.lock
    }
}

impl<T: ?Sized, U: ?Sized> Drop for OwnedRwLockMappedWriteGuard<T, U> {
    fn drop(&mut self) {
        self.lock.semaphore.release(self.permits as usize);
    }
}

guard_traits!([T: ?Sized, U: ?Sized] OwnedRwLockMappedWriteGuard<T, U> => mut U);
