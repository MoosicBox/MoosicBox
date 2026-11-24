//! Caller-driven single-thread scheduler. Only task notifications cross threads.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    pin::{Pin, pin},
    rc::Rc,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

#[derive(Default)]
struct Ready {
    queue: VecDeque<u64>,
    driver: Option<Waker>,
}

impl Ready {
    fn next(&mut self, driver: &Waker) -> Option<u64> {
        let next = self.queue.pop_front();
        if next.is_none() {
            self.driver = Some(driver.clone());
        }
        next
    }
}

// Scheduler turns also wake block_on. Only poll the user's root when *its* waker fires.
struct RootWake {
    driver: Waker,
    ready: AtomicBool,
}

impl Wake for RootWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.ready.store(true, Ordering::Release);
        self.driver.wake_by_ref();
    }
}

struct Ticket {
    id: u64,
    ready: Weak<Mutex<Ready>>,
    queued: AtomicBool,
}

impl Wake for Ticket {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(ready) = self.ready.upgrade() {
            let driver = {
                let mut state = ready.lock().unwrap();
                if self.queued.swap(true, Ordering::Relaxed) {
                    return;
                }
                state.queue.push_back(self.id);
                state.driver.take()
            };
            if let Some(waker) = driver {
                waker.wake();
            }
        }
    }
}

/// Terminal outcome when a spawned task did not produce a value.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum JoinError {
    /// The executor shut down or the handle requested cancellation before completion.
    Cancelled,
    /// Polling the task unwound.
    Panicked,
}

struct Completion<T> {
    result: Option<Result<T, JoinError>>,
    waiter: Option<Waker>,
}

/// Local join future. Dropping it detaches the task; it does not request cancellation.
pub struct JoinHandle<T> {
    completion: Rc<RefCell<Completion<T>>>,
    abort: Rc<Cell<bool>>,
    ticket: Arc<Ticket>,
}

impl<T> JoinHandle<T> {
    /// Request cancellation at the next scheduling turn. Completed results are preserved.
    pub fn abort(&self) {
        self.abort.set(true);
        self.ticket.wake_by_ref();
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waiter = cx.waker().clone();
        let mut slot = self.completion.borrow_mut();
        if let Some(result) = slot.result.take() {
            return Poll::Ready(result);
        }
        let previous = slot.waiter.replace(waiter);
        drop(slot);
        drop(previous);
        Poll::Pending
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.completion.borrow_mut().waiter.take();
    }
}

trait Runnable {
    fn turn(&mut self, cx: &mut Context<'_>) -> bool;
}

struct Task<F: Future> {
    future: Option<Pin<Box<F>>>,
    completion: Rc<RefCell<Completion<F::Output>>>,
    abort: Rc<Cell<bool>>,
}

impl<F: Future> Task<F> {
    fn finish(&mut self, result: Result<F::Output, JoinError>) {
        // Release captures before publishing a terminal outcome, including on shutdown.
        self.future.take();
        let waiter = {
            let mut slot = self.completion.borrow_mut();
            slot.result = Some(result);
            slot.waiter.take()
        };
        if let Some(waiter) = waiter {
            waiter.wake();
        }
    }
}

impl<F: Future> Runnable for Task<F> {
    fn turn(&mut self, cx: &mut Context<'_>) -> bool {
        let result = if self.abort.get() {
            Err(JoinError::Cancelled)
        } else {
            match catch_unwind(AssertUnwindSafe(|| {
                self.future.as_mut().unwrap().as_mut().poll(cx)
            })) {
                Ok(Poll::Pending) => return false,
                Ok(Poll::Ready(value)) => Ok(value),
                Err(_) => Err(JoinError::Panicked),
            }
        };
        self.finish(result);
        true
    }
}

impl<F: Future> Drop for Task<F> {
    fn drop(&mut self) {
        if self.future.is_some() {
            self.finish(Err(JoinError::Cancelled));
        }
    }
}

/// A local FIFO ready queue. Tasks make progress only during `run_until`.
/// Dropping the executor cancels all unfinished tasks, including detached ones.
#[derive(Default)]
pub struct Executor {
    ready: Arc<Mutex<Ready>>,
    tasks: BTreeMap<u64, (Arc<Ticket>, Box<dyn Runnable>)>,
    next: u64,
}

impl Executor {
    /// Enqueue a local task without polling it inline.
    ///
    /// # Panics
    /// Panics if task identities are exhausted.
    pub fn spawn<F: Future + 'static>(&mut self, future: F) -> JoinHandle<F::Output> {
        let id = self.next;
        self.next = id.checked_add(1).expect("task identities exhausted");
        let ticket = Arc::new(Ticket {
            id,
            ready: Arc::downgrade(&self.ready),
            queued: AtomicBool::new(false),
        });
        let completion = Rc::new(RefCell::new(Completion {
            result: None,
            waiter: None,
        }));
        let abort = Rc::new(Cell::new(false));
        let task = Task {
            future: Some(Box::pin(future)),
            completion: completion.clone(),
            abort: abort.clone(),
        };
        self.tasks.insert(id, (ticket.clone(), Box::new(task)));
        ticket.wake_by_ref();
        JoinHandle {
            completion,
            abort,
            ticket,
        }
    }

    /// Drive the root and ready tasks on this thread until the root returns. Other
    /// tasks remain available to subsequent calls. This does not drive timers or IO.
    ///
    /// # Panics
    /// Propagates root panics and panics from user destructors.
    pub fn run_until<F: Future>(&mut self, root: F) -> F::Output {
        let mut root = pin!(root);
        let mut notification = None;
        super::block_on(poll_fn(|cx| {
            let (signal, waker) = notification.get_or_insert_with(|| {
                let signal = Arc::new(RootWake {
                    driver: cx.waker().clone(),
                    ready: AtomicBool::new(true),
                });
                let waker = Waker::from(signal.clone());
                (signal, waker)
            });
            if signal.ready.swap(false, Ordering::Acquire)
                && let Poll::Ready(value) = root.as_mut().poll(&mut Context::from_waker(waker))
            {
                return Poll::Ready(value);
            }
            let Some(id) = self.ready.lock().unwrap().next(cx.waker()) else {
                return Poll::Pending;
            };
            if let Some((ticket, task)) = self.tasks.get_mut(&id) {
                ticket.queued.store(false, Ordering::Relaxed);
                let waker = Waker::from(ticket.clone());
                if task.turn(&mut Context::from_waker(&waker)) {
                    // Ignore subsequent stale wakes. At most one already queued ID remains.
                    ticket.queued.store(true, Ordering::Relaxed);
                    self.tasks.remove(&id);
                }
            }
            cx.waker().wake_by_ref();
            Poll::Pending
        }))
    }
}
