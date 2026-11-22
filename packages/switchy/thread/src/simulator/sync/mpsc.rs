//! Simulated multi-producer, single-consumer channels. Every operation is a switch point.
//!
//! Matches `std::sync::mpsc`, except that a `sync_channel(0)` behaves like a channel with room
//! for one message rather than a rendezvous.

use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, PoisonError},
    time::{Duration, Instant},
};

pub use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError, TrySendError};

use super::super::{
    Object, Op, WaitKey,
    coverage::{self, Site},
    deadline_after, new_key, scheduler, touch,
};

struct Channel<T> {
    queue: VecDeque<T>,
    capacity: Option<usize>,
    senders: usize,
    receiver: bool,
}

struct Shared<T> {
    key: u64,
    channel: std::sync::Mutex<Channel<T>>,
}

impl<T> Shared<T> {
    fn channel(&self) -> std::sync::MutexGuard<'_, Channel<T>> {
        touch(Object::Channel(self.key), true);
        self.channel.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn new_channel<T>(capacity: Option<usize>) -> Arc<Shared<T>> {
    Arc::new(Shared {
        key: new_key(),
        channel: std::sync::Mutex::new(Channel {
            queue: VecDeque::new(),
            capacity,
            senders: 1,
            receiver: true,
        }),
    })
}

/// An unbounded channel.
#[must_use]
#[track_caller]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = new_channel(None);
    let site = Site::caller();
    (
        Sender {
            shared: Arc::clone(&shared),
            site,
        },
        Receiver { shared, site },
    )
}

/// A channel holding at most `bound` messages (at least one); senders block while it is full.
#[must_use]
#[track_caller]
pub fn sync_channel<T>(bound: usize) -> (SyncSender<T>, Receiver<T>) {
    let shared = new_channel(Some(bound.max(1)));
    let site = Site::caller();
    (
        SyncSender {
            shared: Arc::clone(&shared),
            site,
        },
        Receiver { shared, site },
    )
}

/// Sends on an unbounded [`channel`].
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
    /// Where it was made: the site of its dropping, for coverage.
    site: Site,
}

/// Sends on a bounded [`sync_channel`].
pub struct SyncSender<T> {
    shared: Arc<Shared<T>>,
    /// Where it was made: the site of its dropping, for coverage.
    site: Site,
}

/// Receives from a channel.
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    /// Where it was made: the site of its dropping, for coverage.
    site: Site,
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T> fmt::Debug for SyncSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncSender").finish_non_exhaustive()
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

fn clone_sender<T>(shared: &Arc<Shared<T>>) -> Arc<Shared<T>> {
    shared.channel().senders += 1;
    Arc::clone(shared)
}

fn drop_sender<T>(shared: &Shared<T>) {
    let last = {
        let mut channel = shared.channel();
        channel.senders -= 1;
        channel.senders == 0
    };
    if last {
        scheduler().wake_all_on(WaitKey::Recv(shared.key));
    }
}

/// Push `value` if there is room. `Err` hands the value back with whether the receiver is gone.
fn try_push<T>(shared: &Shared<T>, value: T) -> Result<(), (T, bool)> {
    let mut channel = shared.channel();
    if !channel.receiver {
        return Err((value, true));
    }
    if channel
        .capacity
        .is_some_and(|capacity| channel.queue.len() >= capacity)
    {
        return Err((value, false));
    }
    channel.queue.push_back(value);
    drop(channel);
    scheduler().wake_all_on(WaitKey::Recv(shared.key));
    Ok(())
}

impl<T> Clone for Sender<T> {
    #[track_caller]
    fn clone(&self) -> Self {
        let _site = coverage::enter();
        Self {
            shared: clone_sender(&self.shared),
            site: Site::caller(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let _site = coverage::enter_at(self.site);
        drop_sender(&self.shared);
    }
}

impl<T> Sender<T> {
    /// Send `value`; never blocks.
    ///
    /// # Errors
    ///
    /// * [`SendError`] with the value if the receiver is gone
    #[track_caller]
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let _site = coverage::enter();
        scheduler().switch_point(Op::Send);
        try_push(&self.shared, value).map_err(|(value, _)| SendError(value))
    }
}

impl<T> Clone for SyncSender<T> {
    #[track_caller]
    fn clone(&self) -> Self {
        let _site = coverage::enter();
        Self {
            shared: clone_sender(&self.shared),
            site: Site::caller(),
        }
    }
}

impl<T> Drop for SyncSender<T> {
    fn drop(&mut self) {
        let _site = coverage::enter_at(self.site);
        drop_sender(&self.shared);
    }
}

impl<T> SyncSender<T> {
    /// Send `value`, blocking while the channel is full.
    ///
    /// # Errors
    ///
    /// * [`SendError`] with the value if the receiver is gone
    #[track_caller]
    pub fn send(&self, mut value: T) -> Result<(), SendError<T>> {
        let _site = coverage::enter();
        let scheduler = scheduler();
        loop {
            scheduler.switch_point(Op::Send);
            match try_push(&self.shared, value) {
                Ok(()) => return Ok(()),
                Err((returned, true)) => return Err(SendError(returned)),
                Err((returned, false)) => value = returned,
            }
            scheduler.wait(Op::Send, WaitKey::Send(self.shared.key), None);
        }
    }

    /// Send `value` if there is room.
    ///
    /// # Errors
    ///
    /// * [`TrySendError::Full`] if the channel is full
    /// * [`TrySendError::Disconnected`] if the receiver is gone
    #[track_caller]
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        let _site = coverage::enter();
        scheduler().switch_point(Op::Send);
        try_push(&self.shared, value).map_err(|(value, disconnected)| {
            if disconnected {
                TrySendError::Disconnected(value)
            } else {
                TrySendError::Full(value)
            }
        })
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let _site = coverage::enter_at(self.site);
        let queued = {
            let mut channel = self.shared.channel();
            channel.receiver = false;
            std::mem::take(&mut channel.queue)
        };
        // Dropped outside the channel's lock: a message may itself hold a sender.
        drop(queued);
        scheduler().wake_all_on(WaitKey::Send(self.shared.key));
    }
}

enum Pop<T> {
    Value(T),
    Empty,
    Disconnected,
}

impl<T> Receiver<T> {
    fn pop(&self) -> Pop<T> {
        let mut channel = self.shared.channel();
        if let Some(value) = channel.queue.pop_front() {
            let bounded = channel.capacity.is_some();
            drop(channel);
            if bounded {
                scheduler().wake_all_on(WaitKey::Send(self.shared.key));
            }
            return Pop::Value(value);
        }
        if channel.senders == 0 {
            Pop::Disconnected
        } else {
            Pop::Empty
        }
    }

    /// Receive a message, blocking until one arrives.
    ///
    /// # Errors
    ///
    /// * [`RecvError`] once the channel is empty and every sender is gone
    #[track_caller]
    pub fn recv(&self) -> Result<T, RecvError> {
        let _site = coverage::enter();
        self.recv_here()
    }

    /// [`Self::recv`] at the calling thread's current site.
    fn recv_here(&self) -> Result<T, RecvError> {
        let scheduler = scheduler();
        loop {
            scheduler.switch_point(Op::Recv);
            match self.pop() {
                Pop::Value(value) => return Ok(value),
                Pop::Disconnected => return Err(RecvError),
                Pop::Empty => {}
            }
            scheduler.wait(Op::Recv, WaitKey::Recv(self.shared.key), None);
        }
    }

    /// Receive a message if one is ready.
    ///
    /// # Errors
    ///
    /// * [`TryRecvError::Empty`] if none is ready
    /// * [`TryRecvError::Disconnected`] once the channel is empty and every sender is gone
    #[track_caller]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let _site = coverage::enter();
        self.try_recv_here()
    }

    /// [`Self::try_recv`] at the calling thread's current site.
    fn try_recv_here(&self) -> Result<T, TryRecvError> {
        scheduler().switch_point(Op::Recv);
        match self.pop() {
            Pop::Value(value) => Ok(value),
            Pop::Empty => Err(TryRecvError::Empty),
            Pop::Disconnected => Err(TryRecvError::Disconnected),
        }
    }

    /// Receive a message, waiting at most `timeout` of simulated time.
    ///
    /// # Errors
    ///
    /// * [`RecvTimeoutError::Timeout`] if none arrived in time
    /// * [`RecvTimeoutError::Disconnected`] once the channel is empty and every sender is gone
    #[track_caller]
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let _site = coverage::enter();
        self.recv_deadline(deadline_after(timeout))
    }

    #[track_caller]
    fn recv_deadline(&self, deadline: Option<Instant>) -> Result<T, RecvTimeoutError> {
        let _site = coverage::enter();
        let scheduler = scheduler();
        loop {
            scheduler.switch_point(Op::Recv);
            match self.pop() {
                Pop::Value(value) => return Ok(value),
                Pop::Disconnected => return Err(RecvTimeoutError::Disconnected),
                Pop::Empty => {}
            }
            if scheduler.wait(Op::Recv, WaitKey::Recv(self.shared.key), deadline) {
                return match self.pop() {
                    Pop::Value(value) => Ok(value),
                    Pop::Disconnected => Err(RecvTimeoutError::Disconnected),
                    Pop::Empty => Err(RecvTimeoutError::Timeout),
                };
            }
        }
    }

    /// Iterate over messages, blocking for each, until every sender is gone.
    #[must_use]
    pub const fn iter(&self) -> Iter<'_, T> {
        Iter { receiver: self }
    }

    /// Iterate over the messages that are ready, without blocking.
    #[must_use]
    pub const fn try_iter(&self) -> TryIter<'_, T> {
        TryIter { receiver: self }
    }
}

/// Blocking iterator over a [`Receiver`]'s messages.
#[derive(Debug)]
pub struct Iter<'a, T> {
    receiver: &'a Receiver<T>,
}

impl<T> Iterator for Iter<'_, T> {
    type Item = T;

    /// Receives at the receiver's site (iterator adapters would hide the caller's).
    fn next(&mut self) -> Option<T> {
        let _site = coverage::enter_at(self.receiver.site);
        self.receiver.recv_here().ok()
    }
}

/// Non-blocking iterator over a [`Receiver`]'s ready messages.
#[derive(Debug)]
pub struct TryIter<'a, T> {
    receiver: &'a Receiver<T>,
}

impl<T> Iterator for TryIter<'_, T> {
    type Item = T;

    /// Receives at the receiver's site (iterator adapters would hide the caller's).
    fn next(&mut self) -> Option<T> {
        let _site = coverage::enter_at(self.receiver.site);
        self.receiver.try_recv_here().ok()
    }
}

/// Owning blocking iterator over a [`Receiver`]'s messages.
#[derive(Debug)]
pub struct IntoIter<T> {
    receiver: Receiver<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    /// Receives at the receiver's site (iterator adapters would hide the caller's).
    fn next(&mut self) -> Option<T> {
        let _site = coverage::enter_at(self.receiver.site);
        self.receiver.recv_here().ok()
    }
}

impl<T> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> IntoIter<T> {
        IntoIter { receiver: self }
    }
}

impl<'a, T> IntoIterator for &'a Receiver<T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
