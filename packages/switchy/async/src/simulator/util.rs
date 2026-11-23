//! Utility types for the simulator runtime.
//!
//! [`CancellationToken`] mirrors `tokio_util::sync::CancellationToken`: a token, its clones and
//! its child tokens (cancelled with it, but not cancelling it) in a tree. Tasks waiting for a
//! token's cancellation are kept in one list ordered by when they started waiting, and woken in
//! that order; a cancel wakes the token's own waiters first, then cancels its children in the
//! order they were created.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    task::{Context, Poll},
};

use futures::future::FusedFuture;

use super::{
    faults,
    waiters::{Waiters, wake},
};

struct NodeState {
    cancelled: bool,
    /// Child tokens, in the order they were created.
    children: Vec<Weak<Node>>,
    /// Tasks waiting for the cancellation.
    waiters: Waiters,
}

struct Node {
    state: Mutex<NodeState>,
    /// Keeps the chain from an ancestor to this node alive for as long as this node lives, so
    /// the ancestor's cancellation reaches it.
    _parent: Option<Arc<Self>>,
}

impl Node {
    const fn new(cancelled: bool, parent: Option<Arc<Self>>) -> Self {
        Self {
            state: Mutex::new(NodeState {
                cancelled,
                children: Vec::new(),
                waiters: Waiters::new(),
            }),
            _parent: parent,
        }
    }

    fn state(&self) -> MutexGuard<'_, NodeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Cancel this node: wake its waiters, oldest first, then cancel its children in the order
    /// they were created.
    fn cancel(&self) {
        let mut state = self.state();
        if state.cancelled {
            return;
        }
        state.cancelled = true;
        let wakers = state.waiters.take();
        let children = std::mem::take(&mut state.children);
        drop(state);
        wake(wakers);
        for child in children.iter().filter_map(Weak::upgrade) {
            child.cancel();
        }
    }
}

/// A token for cooperative cancellation: tasks wait for [`Self::cancelled`], and any holder
/// of the token (or of a clone) can [`Self::cancel`] it.
///
/// Mirrors `tokio_util::sync::CancellationToken`; see the [module](self) docs.
#[derive(Clone)]
pub struct CancellationToken {
    node: Arc<Node>,
}

impl CancellationToken {
    /// A new token, not cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            node: Arc::new(Node::new(false, None)),
        }
    }

    /// A child token: cancelled when this token is (at once, if it already is), but cancelling
    /// it does not cancel this token.
    #[must_use]
    pub fn child_token(&self) -> Self {
        let mut state = self.node.state();
        let child = Arc::new(Node::new(state.cancelled, Some(Arc::clone(&self.node))));
        if !state.cancelled {
            state.children.retain(|child| child.strong_count() > 0);
            state.children.push(Arc::downgrade(&child));
        }
        drop(state);
        Self { node: child }
    }

    /// Cancels the token and its children, waking every task waiting for it.
    pub fn cancel(&self) {
        self.node.cancel();
    }

    /// Whether the token was cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.node.state().cancelled
    }

    /// Waits for the token to be cancelled.
    ///
    /// Waiting tasks are woken in the order they started waiting.
    pub const fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        WaitForCancellationFuture {
            token: self,
            wait: Wait::new(),
        }
    }

    /// [`Self::cancelled`], owning the token.
    pub const fn cancelled_owned(self) -> WaitForCancellationFutureOwned {
        WaitForCancellationFutureOwned {
            token: self,
            wait: Wait::new(),
        }
    }

    /// A guard that cancels the token when dropped, unless disarmed.
    // As tokio_util's, not `must_use`: dropping the guard at once cancels the token.
    #[allow(clippy::must_use_candidate)]
    pub const fn drop_guard(self) -> DropGuard {
        DropGuard { inner: Some(self) }
    }

    /// A guard that cancels the token when dropped, unless disarmed, borrowing the token.
    // As tokio_util's, not `must_use`: dropping the guard at once cancels the token.
    #[allow(clippy::must_use_candidate)]
    pub const fn drop_guard_ref(&self) -> DropGuardRef<'_> {
        DropGuardRef { inner: Some(self) }
    }

    /// Runs `fut` to completion unless the token is cancelled first, in which case `fut` is
    /// dropped and `None` returned.
    ///
    /// An already cancelled token returns `None` at once. Afterwards, when both are ready at
    /// once, the future's completion wins.
    pub async fn run_until_cancelled<F: Future>(&self, fut: F) -> Option<F::Output> {
        if self.is_cancelled() {
            return None;
        }
        RunUntilCancelled {
            cancellation: self.cancelled(),
            future: fut,
        }
        .await
    }

    /// [`Self::run_until_cancelled`], owning the token.
    pub async fn run_until_cancelled_owned<F: Future>(self, fut: F) -> Option<F::Output> {
        self.run_until_cancelled(fut).await
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// One wait for a token's cancellation: its place in line, once it waited.
#[derive(Debug)]
struct Wait {
    waiter: Option<u64>,
    done: bool,
}

impl Wait {
    const fn new() -> Self {
        Self {
            waiter: None,
            done: false,
        }
    }

    fn poll(&mut self, node: &Node, cx: &Context<'_>) -> Poll<()> {
        if faults::spurious_pending_now(cx, || node.state().cancelled) {
            return Poll::Pending;
        }
        let mut state = node.state();
        if state.cancelled {
            state.waiters.remove(self.waiter.take());
            self.done = true;
            return Poll::Ready(());
        }
        state.waiters.register(&mut self.waiter, cx.waker());
        Poll::Pending
    }

    fn leave(&mut self, node: &Node) {
        if self.waiter.is_some() {
            node.state().waiters.remove(self.waiter.take());
        }
    }
}

/// The future [`CancellationToken::cancelled`] returns.
#[must_use = "futures do nothing unless polled"]
pub struct WaitForCancellationFuture<'a> {
    token: &'a CancellationToken,
    wait: Wait,
}

impl fmt::Debug for WaitForCancellationFuture<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitForCancellationFuture").finish()
    }
}

impl Future for WaitForCancellationFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        this.wait.poll(&this.token.node, cx)
    }
}

impl FusedFuture for WaitForCancellationFuture<'_> {
    fn is_terminated(&self) -> bool {
        self.wait.done
    }
}

impl Drop for WaitForCancellationFuture<'_> {
    fn drop(&mut self) {
        self.wait.leave(&self.token.node);
    }
}

/// The future [`CancellationToken::cancelled_owned`] returns.
#[must_use = "futures do nothing unless polled"]
pub struct WaitForCancellationFutureOwned {
    token: CancellationToken,
    wait: Wait,
}

impl fmt::Debug for WaitForCancellationFutureOwned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitForCancellationFutureOwned").finish()
    }
}

impl Future for WaitForCancellationFutureOwned {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        this.wait.poll(&this.token.node, cx)
    }
}

impl FusedFuture for WaitForCancellationFutureOwned {
    fn is_terminated(&self) -> bool {
        self.wait.done
    }
}

impl Drop for WaitForCancellationFutureOwned {
    fn drop(&mut self) {
        self.wait.leave(&self.token.node);
    }
}

pin_project_lite::pin_project! {
    /// Polls the future first, then the cancellation (see
    /// [`CancellationToken::run_until_cancelled`]).
    struct RunUntilCancelled<'a, F> {
        cancellation: WaitForCancellationFuture<'a>,
        #[pin]
        future: F,
    }
}

impl<F: Future> Future for RunUntilCancelled<'_, F> {
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if let Poll::Ready(output) = this.future.poll(cx) {
            return Poll::Ready(Some(output));
        }
        if Pin::new(this.cancellation).poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}

/// Cancels its token when dropped, unless [`Self::disarm`]ed.
#[derive(Debug)]
pub struct DropGuard {
    inner: Option<CancellationToken>,
}

impl DropGuard {
    /// The token this guard cancels.
    ///
    /// # Panics
    ///
    /// * Never: the token is held until the guard is disarmed or dropped
    #[must_use]
    pub const fn token(&self) -> &CancellationToken {
        self.inner.as_ref().expect("the guard holds its token")
    }

    /// The token, without cancelling it.
    ///
    /// # Panics
    ///
    /// * Never: the token is held until the guard is disarmed or dropped
    // As tokio_util's, not `must_use`: it is often called only to disarm the guard.
    #[allow(clippy::must_use_candidate)]
    pub fn disarm(mut self) -> CancellationToken {
        self.inner.take().expect("the guard holds its token")
    }
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        if let Some(token) = &self.inner {
            token.cancel();
        }
    }
}

/// Cancels its borrowed token when dropped, unless [`Self::disarm`]ed.
#[derive(Debug)]
pub struct DropGuardRef<'a> {
    inner: Option<&'a CancellationToken>,
}

impl<'a> DropGuardRef<'a> {
    /// The token this guard cancels.
    ///
    /// # Panics
    ///
    /// * Never: the token is held until the guard is disarmed or dropped
    #[must_use]
    pub const fn token(&self) -> &CancellationToken {
        self.inner.expect("the guard holds its token")
    }

    /// The token, without cancelling it.
    ///
    /// # Panics
    ///
    /// * Never: the token is held until the guard is disarmed or dropped
    // As tokio_util's, not `must_use`: it is often called only to disarm the guard.
    #[allow(clippy::must_use_candidate)]
    pub fn disarm(mut self) -> &'a CancellationToken {
        self.inner.take().expect("the guard holds its token")
    }
}

impl Drop for DropGuardRef<'_> {
    fn drop(&mut self) {
        if let Some(token) = self.inner {
            token.cancel();
        }
    }
}
