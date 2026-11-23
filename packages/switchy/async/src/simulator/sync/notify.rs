//! Notifies a waiting task, with tokio's API, waking waiters in a defined order.
//!
//! As tokio's: [`Notify::notify_one`] wakes the oldest waiter (or stores one permit if none
//! waits), [`Notify::notify_last`] the newest, and [`Notify::notify_waiters`] every waiter
//! (oldest first) and every [`Notified`] created before it, storing nothing. A waiter dropped
//! after `notify_one` picked it passes the notification on. Waiters are kept in one list ordered
//! by when they started waiting.

use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Poll, Waker},
};

use crate::simulator::faults;

/// Which waiter a single notification goes to.
#[derive(Debug, Clone, Copy)]
enum Strategy {
    /// The oldest.
    Fifo,
    /// The newest.
    Lifo,
}

struct State {
    /// A notification stored for the next waiter (at most one).
    permit: bool,
    /// How many times [`Notify::notify_waiters`] was called.
    calls: u64,
    next: u64,
    /// Waiting futures, oldest first, with their wakers (none for one only enabled).
    waiting: BTreeMap<u64, Option<Waker>>,
    /// Waiters [`Notify::notify_one`] or [`Notify::notify_last`] picked, not yet completed.
    picked: BTreeMap<u64, Strategy>,
}

impl State {
    /// Give one notification to a waiter picked by `strategy`, or store it. Returns the picked
    /// waiter's waker, to wake once the lock is released.
    fn notify(&mut self, strategy: Strategy) -> Option<Waker> {
        let entry = match strategy {
            Strategy::Fifo => self.waiting.first_entry(),
            Strategy::Lifo => self.waiting.last_entry(),
        };
        let Some(entry) = entry else {
            self.permit = true;
            return None;
        };
        let (id, waker) = entry.remove_entry();
        self.picked.insert(id, strategy);
        waker
    }
}

/// Notifies a single task, or every waiting task, to wake up.
///
/// Mirrors `tokio::sync::Notify`; see the [module](self) docs.
pub struct Notify {
    state: Mutex<State>,
}

impl Notify {
    /// A new `Notify` with no permit stored.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                permit: false,
                calls: 0,
                next: 0,
                waiting: BTreeMap::new(),
                picked: BTreeMap::new(),
            }),
        }
    }

    /// A new `Notify`, usable in a `static`.
    #[must_use]
    pub const fn const_new() -> Self {
        Self::new()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits for a notification.
    ///
    /// The returned future completes when a notification is given to it, a permit is stored, or
    /// [`Self::notify_waiters`] is called after it was created (even before it is first polled).
    /// It joins the waiters when first polled (or [`Notified::enable`]d).
    pub fn notified(&self) -> Notified<'_> {
        Notified {
            notify: self,
            waiter: Waiter::new(self.state().calls),
        }
    }

    /// [`Self::notified`], owning the `Notify` (through an [`Arc`]).
    pub fn notified_owned(self: Arc<Self>) -> OwnedNotified {
        let waiter = Waiter::new(self.state().calls);
        OwnedNotified {
            notify: self,
            waiter,
        }
    }

    /// Notifies the oldest waiter, or stores a permit for the next one if none waits.
    pub fn notify_one(&self) {
        self.notify(Strategy::Fifo);
    }

    /// Notifies the newest waiter, or stores a permit for the next one if none waits.
    pub fn notify_last(&self) {
        self.notify(Strategy::Lifo);
    }

    fn notify(&self, strategy: Strategy) {
        let waker = self.state().notify(strategy);
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Notifies every waiter, oldest first, and every [`Notified`] created before this call.
    /// No permit is stored.
    pub fn notify_waiters(&self) {
        let mut state = self.state();
        state.calls = state.calls.wrapping_add(1);
        let wakers: Vec<Waker> = std::mem::take(&mut state.waiting)
            .into_values()
            .flatten()
            .collect();
        drop(state);
        for waker in wakers {
            waker.wake();
        }
    }
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Notify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state();
        f.debug_struct("Notify")
            .field("permit", &state.permit)
            .field("waiters", &state.waiting.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Not yet polled.
    Init,
    /// Waiting, with its id.
    Waiting(u64),
    Done,
}

/// The state of one wait, shared by [`Notified`] and [`OwnedNotified`].
#[derive(Debug)]
struct Waiter {
    phase: Phase,
    /// [`State::calls`] when it was created.
    calls: u64,
}

impl Waiter {
    const fn new(calls: u64) -> Self {
        Self {
            phase: Phase::Init,
            calls,
        }
    }

    /// Whether a poll would complete now.
    fn ready(&self, notify: &Notify) -> bool {
        let state = notify.state();
        match self.phase {
            Phase::Init => state.calls != self.calls || state.permit,
            Phase::Waiting(id) => state.calls != self.calls || state.picked.contains_key(&id),
            Phase::Done => true,
        }
    }

    fn poll(&mut self, notify: &Notify, waker: Option<&Waker>) -> Poll<()> {
        self.poll_locked(&mut notify.state(), waker)
    }

    fn poll_locked(&mut self, state: &mut State, waker: Option<&Waker>) -> Poll<()> {
        match self.phase {
            Phase::Init => {
                if state.calls != self.calls {
                    self.phase = Phase::Done;
                    return Poll::Ready(());
                }
                if state.permit {
                    state.permit = false;
                    self.phase = Phase::Done;
                    return Poll::Ready(());
                }
                let id = state.next;
                state.next += 1;
                state.waiting.insert(id, waker.cloned());
                self.phase = Phase::Waiting(id);
                Poll::Pending
            }
            Phase::Waiting(id) => {
                if state.picked.remove(&id).is_some() {
                    self.phase = Phase::Done;
                    return Poll::Ready(());
                }
                if state.calls != self.calls {
                    state.waiting.remove(&id);
                    self.phase = Phase::Done;
                    return Poll::Ready(());
                }
                if let (Some(waker), Some(slot)) = (waker, state.waiting.get_mut(&id)) {
                    match slot {
                        Some(current) if current.will_wake(waker) => {}
                        slot => *slot = Some(waker.clone()),
                    }
                }
                Poll::Pending
            }
            Phase::Done => Poll::Ready(()),
        }
    }

    fn poll_future(&mut self, notify: &Notify, cx: &Context<'_>) -> Poll<()> {
        if faults::spurious_pending_now(cx, || self.ready(notify)) {
            return Poll::Pending;
        }
        self.poll(notify, Some(cx.waker()))
    }

    /// Leave the waiters; a notification given to this wait goes to another.
    fn leave(&self, notify: &Notify) {
        let Phase::Waiting(id) = self.phase else {
            return;
        };
        let mut state = notify.state();
        state.waiting.remove(&id);
        let waker = state
            .picked
            .remove(&id)
            .and_then(|strategy| state.notify(strategy));
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// The future [`Notify::notified`] returns.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct Notified<'a> {
    notify: &'a Notify,
    waiter: Waiter,
}

impl Notified<'_> {
    /// Joins the waiters without waiting, so a later [`Notify::notify_one`] can pick this
    /// future. Returns whether it is already notified.
    // As tokio's, not `must_use`: it is often called for its effect alone.
    #[allow(clippy::must_use_candidate)]
    pub fn enable(self: Pin<&mut Self>) -> bool {
        let this = self.get_mut();
        this.waiter.poll(this.notify, None).is_ready()
    }
}

impl Future for Notified<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        this.waiter.poll_future(this.notify, cx)
    }
}

impl Drop for Notified<'_> {
    fn drop(&mut self) {
        self.waiter.leave(self.notify);
    }
}

/// The future [`Notify::notified_owned`] returns.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct OwnedNotified {
    notify: Arc<Notify>,
    waiter: Waiter,
}

impl OwnedNotified {
    /// Joins the waiters without waiting; see [`Notified::enable`].
    // As tokio's, not `must_use`: it is often called for its effect alone.
    #[allow(clippy::must_use_candidate)]
    pub fn enable(self: Pin<&mut Self>) -> bool {
        let this = self.get_mut();
        this.waiter.poll(&this.notify, None).is_ready()
    }
}

impl Future for OwnedNotified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        this.waiter.poll_future(&this.notify, cx)
    }
}

impl Drop for OwnedNotified {
    fn drop(&mut self) {
        self.waiter.leave(&self.notify);
    }
}
