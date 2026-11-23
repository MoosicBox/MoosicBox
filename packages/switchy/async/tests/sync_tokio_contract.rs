//! The simulator's own synchronization primitives against tokio's (and tokio-util's
//! `CancellationToken`): each scenario runs once against each, polling by hand with wakers that
//! record when they are woken, and both must observe the same results in the same order,
//! including which waiters are woken and in what order.
#![cfg(all(feature = "simulator", feature = "sync", feature = "util"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};

use pretty_assertions::assert_eq;

/// What a scenario observed, in order.
#[derive(Clone, Default)]
struct Trace(Arc<Mutex<Vec<String>>>);

/// A waker that notes when it is woken.
struct Noting {
    trace: Trace,
    name: String,
}

impl Wake for Noting {
    fn wake(self: Arc<Self>) {
        self.trace.note(format!("wake {}", self.name));
    }
}

impl Trace {
    fn note(&self, line: impl Into<String>) {
        self.0.lock().unwrap().push(line.into());
    }

    fn waker(&self, name: &str) -> Waker {
        Waker::from(Arc::new(Noting {
            trace: self.clone(),
            name: name.to_string(),
        }))
    }

    /// Poll `future` once with the waker named `name`, noting whether it was ready.
    fn poll<F: Future + ?Sized>(&self, name: &str, future: Pin<&mut F>) -> Option<F::Output> {
        let waker = self.waker(name);
        let ready = future.poll(&mut Context::from_waker(&waker));
        self.note(format!(
            "poll {name}: {}",
            if ready.is_ready() { "ready" } else { "pending" }
        ));
        match ready {
            Poll::Ready(output) => Some(output),
            Poll::Pending => None,
        }
    }

    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// Defines each scenario twice, as `switchy::NAME` with the simulator's primitives and
/// `tokio_impl::NAME` with tokio's, each a `fn() -> Vec<String>` of what it observed.
macro_rules! scenarios {
    ($($scenario:item)*) => {
        mod switchy {
            use super::Trace;
            use std::{pin::Pin, sync::Arc};
            use switchy_async::sync as s;
            use switchy_async::util::CancellationToken;

            $(
                // Scenarios hold guards and permits on purpose, to observe the primitive while
                // they are held.
                #[allow(clippy::significant_drop_tightening)]
                $scenario
            )*
        }

        mod tokio_impl {
            use super::Trace;
            use std::{pin::Pin, sync::Arc};
            use tokio::sync as s;
            use tokio_util::sync::CancellationToken;

            $(
                // Scenarios hold guards and permits on purpose, to observe the primitive while
                // they are held.
                #[allow(clippy::significant_drop_tightening)]
                $scenario
            )*
        }
    };
}

/// A test per scenario, comparing what both observed.
macro_rules! compare {
    ($($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(switchy::$name(), tokio_impl::$name());
            }
        )*
    };
}

scenarios! {
    pub fn mutex_waiters_get_the_lock_in_order() -> Vec<String> {
        let t = Trace::default();
        let mutex = s::Mutex::new(0);
        let guard = mutex.try_lock().unwrap();
        let mut a = Box::pin(mutex.lock());
        let mut b = Box::pin(mutex.lock());
        let mut c = Box::pin(mutex.lock());
        t.poll("a", a.as_mut());
        t.poll("b", b.as_mut());
        t.poll("c", c.as_mut());
        t.note(format!("try_lock: {}", mutex.try_lock().is_ok()));
        t.note(format!("{mutex:?}"));
        drop(guard);
        t.note("released");
        t.poll("c", c.as_mut());
        t.note(format!("try_lock: {}", mutex.try_lock().is_ok()));
        let mut guard = t.poll("a", a.as_mut()).unwrap();
        *guard += 1;
        t.poll("b", b.as_mut());
        // A cancelled waiter gives up its place.
        drop(b);
        drop(guard);
        t.note("released");
        let guard = t.poll("c", c.as_mut()).unwrap();
        t.note(format!("{guard} {guard:?}"));
        drop(guard);
        t.note(format!("{mutex:?}"));
        t.lines()
    }

    pub fn mutex_lock_given_to_a_cancelled_waiter_passes_on() -> Vec<String> {
        let t = Trace::default();
        let mutex = s::Mutex::new(());
        let guard = mutex.try_lock().unwrap();
        let mut a = Box::pin(mutex.lock());
        let mut b = Box::pin(mutex.lock());
        t.poll("a", a.as_mut());
        t.poll("b", b.as_mut());
        drop(guard);
        drop(a);
        t.note("a dropped");
        t.poll("b", b.as_mut());
        t.lines()
    }

    pub fn mutex_guards_map_and_own() -> Vec<String> {
        let t = Trace::default();
        let mutex = Arc::new(s::Mutex::new((1, String::from("x"))));
        {
            let guard = mutex.try_lock().unwrap();
            t.note(format!("mutex: {}", std::ptr::eq(s::MutexGuard::mutex(&guard), Arc::as_ptr(&mutex))));
            let mut name = s::MutexGuard::map(guard, |value| &mut value.1);
            name.push('y');
            t.note(format!("{name} {name:?} locked: {}", mutex.try_lock().is_err()));
            let name = s::MappedMutexGuard::map(name, |name| name);
            t.note(format!("{name}"));
        }
        let owned = Arc::clone(&mutex).try_lock_owned().unwrap();
        t.note(format!("owned: {}", Arc::ptr_eq(s::OwnedMutexGuard::mutex(&owned), &mutex)));
        let Err(owned) = s::OwnedMutexGuard::try_map(owned, |v| (v.0 == 2).then_some(&mut v.0)) else {
            panic!("mapped");
        };
        let mut number = s::OwnedMutexGuard::map(owned, |value| &mut value.0);
        *number += 1;
        t.note(format!("{number} locked: {}", mutex.try_lock().is_err()));
        let number = s::OwnedMappedMutexGuard::map(number, |n| n);
        drop(number);
        let blocking = Arc::clone(&mutex);
        let seen = std::thread::spawn(move || blocking.blocking_lock().0).join().unwrap();
        t.note(format!("blocking: {seen}"));
        let owned = Arc::clone(&mutex).blocking_lock_owned();
        drop(owned);
        let mut mutex = Arc::try_unwrap(mutex).unwrap();
        mutex.get_mut().0 = 5;
        t.note(format!("{:?}", mutex.into_inner()));
        t.note(format!("{:?}", s::Mutex::<u8>::default()));
        let held = s::Mutex::new(0);
        let guard = held.try_lock().unwrap();
        let error = held.try_lock().err().unwrap();
        t.note(format!("{error} {error:?}"));
        drop(guard);
        t.lines()
    }

    pub fn rwlock_writer_is_not_overtaken() -> Vec<String> {
        let t = Trace::default();
        let lock = s::RwLock::new(1);
        let r1 = lock.try_read().unwrap();
        let r2 = lock.try_read().unwrap();
        let mut w = Box::pin(lock.write());
        t.poll("w", w.as_mut());
        // A reader arriving after a waiting writer waits behind it.
        t.note(format!("try_read: {}", lock.try_read().is_ok()));
        let mut r3 = Box::pin(lock.read());
        t.poll("r3", r3.as_mut());
        drop(r1);
        t.note("r1 dropped");
        drop(r2);
        t.note("r2 dropped");
        t.poll("r3", r3.as_mut());
        let mut write = t.poll("w", w.as_mut()).unwrap();
        *write = 2;
        t.note(format!("{lock:?}"));
        let read = write.downgrade();
        t.note(format!("downgraded: {read} {read:?} try_write: {}", lock.try_write().is_ok()));
        let r3 = t.poll("r3", r3.as_mut()).unwrap();
        t.note(format!("r3: {r3}"));
        drop((read, r3));
        let write = lock.try_write().unwrap();
        t.note(format!("try_read: {}", lock.try_read().is_ok()));
        drop(write);
        t.note(format!("{lock:?}"));
        let limited = s::RwLock::with_max_readers(0, 2);
        let a = limited.try_read().unwrap();
        let b = limited.try_read().unwrap();
        t.note(format!("third: {}", limited.try_read().is_ok()));
        drop((a, b));
        t.lines()
    }

    pub fn rwlock_writers_and_readers_alternate_in_order() -> Vec<String> {
        let t = Trace::default();
        let lock = s::RwLock::new(0);
        let held = lock.try_write().unwrap();
        let mut r1 = Box::pin(lock.read());
        let mut r2 = Box::pin(lock.read());
        let mut w = Box::pin(lock.write());
        let mut r3 = Box::pin(lock.read());
        t.poll("r1", r1.as_mut());
        t.poll("r2", r2.as_mut());
        t.poll("w", w.as_mut());
        t.poll("r3", r3.as_mut());
        drop(held);
        t.note("writer released");
        let a = t.poll("r1", r1.as_mut());
        let b = t.poll("r2", r2.as_mut());
        t.poll("w", w.as_mut());
        t.poll("r3", r3.as_mut());
        drop((a, b));
        t.note("readers released");
        let guard = t.poll("w", w.as_mut());
        t.poll("r3", r3.as_mut());
        drop(guard);
        t.note("writer released");
        t.poll("r3", r3.as_mut());
        t.lines()
    }

    pub fn rwlock_guards_map_and_own() -> Vec<String> {
        let t = Trace::default();
        let lock = Arc::new(s::RwLock::new((1, String::from("a"))));
        {
            let read = lock.try_read().unwrap();
            let name = s::RwLockReadGuard::map(read, |value| &value.1);
            let Ok(name) = s::RwLockReadGuard::try_map(name, |name| name.get(..1)) else {
                panic!("unmapped");
            };
            t.note(format!("{name} {name:?}"));
        }
        {
            let write = lock.try_write().unwrap();
            let mut name = s::RwLockWriteGuard::map(write, |value| &mut value.1);
            name.push('b');
            let Err(name) = s::RwLockMappedWriteGuard::try_map(name, |_| None::<&mut u8>) else {
                panic!("mapped");
            };
            t.note(format!("{name} locked: {}", lock.try_read().is_err()));
        }
        {
            let write = lock.try_write().unwrap();
            let number = s::RwLockWriteGuard::downgrade_map(write, |value| &value.0);
            t.note(format!("{number} write: {} read: {}", lock.try_write().is_ok(), lock.try_read().is_ok()));
        }
        {
            let write = lock.try_write().unwrap();
            let whole = s::RwLockWriteGuard::into_mapped(write);
            t.note(format!("{:?}", *whole));
        }
        let owned = Arc::clone(&lock).try_write_owned().unwrap();
        t.note(format!("owned: {}", Arc::ptr_eq(s::OwnedRwLockWriteGuard::rwlock(&owned), &lock)));
        let mut number = s::OwnedRwLockWriteGuard::map(owned, |value| &mut value.0);
        *number = 3;
        drop(number);
        let owned = Arc::clone(&lock).try_write_owned().unwrap();
        let read = owned.downgrade();
        let name = s::OwnedRwLockReadGuard::map(read, |value| &value.1);
        t.note(format!("{name} read: {} write: {}", lock.try_read().is_ok(), lock.try_write().is_ok()));
        drop(name);
        let owned = Arc::clone(&lock).try_read_owned().unwrap();
        t.note(format!("{}", owned.0));
        drop(owned);
        let blocking = Arc::clone(&lock);
        let seen = std::thread::spawn(move || {
            blocking.blocking_write().0 += 1;
            blocking.blocking_read().0
        })
        .join()
        .unwrap();
        t.note(format!("blocking: {seen}"));
        let mut lock = Arc::try_unwrap(lock).unwrap();
        lock.get_mut().0 = 9;
        t.note(format!("{:?}", lock.into_inner()));
        t.lines()
    }

    pub fn semaphore_assigns_permits_in_order() -> Vec<String> {
        let t = Trace::default();
        let semaphore = s::Semaphore::new(2);
        let held = semaphore.try_acquire().unwrap();
        t.note(format!("available {}", semaphore.available_permits()));
        // Wants two: takes the one available and waits for the other.
        let mut a = Box::pin(semaphore.acquire_many(2));
        let mut b = Box::pin(semaphore.acquire());
        t.poll("a", a.as_mut());
        t.poll("b", b.as_mut());
        t.note(format!("available {}", semaphore.available_permits()));
        let error = semaphore.try_acquire().unwrap_err();
        t.note(format!("{error:?} {error}"));
        drop(held);
        t.note("released");
        let mut a = t.poll("a", a.as_mut()).unwrap().unwrap();
        t.note(format!("a holds {}", a.num_permits()));
        let one = a.split(1).unwrap();
        t.note(format!("split {} {} {}", one.num_permits(), a.num_permits(), a.split(5).is_none()));
        drop(one);
        t.note("one released");
        let b = t.poll("b", b.as_mut()).unwrap().unwrap();
        let mut b = b;
        b.merge(a);
        t.note(format!("b holds {}", b.num_permits()));
        b.forget();
        t.note(format!("available {}", semaphore.available_permits()));
        semaphore.add_permits(3);
        t.note(format!("forgot {}", semaphore.forget_permits(5)));
        semaphore.add_permits(1);
        let many = semaphore.try_acquire_many(1).unwrap();
        t.note(format!("many {} available {}", many.num_permits(), semaphore.available_permits()));
        drop(many);
        t.note(format!("max {}", s::Semaphore::MAX_PERMITS));
        t.lines()
    }

    pub fn semaphore_cancelled_waiter_returns_its_permits() -> Vec<String> {
        let t = Trace::default();
        let semaphore = s::Semaphore::new(1);
        let mut a = Box::pin(semaphore.acquire_many(2));
        let mut b = Box::pin(semaphore.acquire());
        t.poll("a", a.as_mut());
        t.poll("b", b.as_mut());
        t.note(format!("available {}", semaphore.available_permits()));
        drop(a);
        t.note("a dropped");
        let b = t.poll("b", b.as_mut()).unwrap().unwrap();
        t.note(format!("available {}", semaphore.available_permits()));
        drop(b);
        t.note(format!("available {}", semaphore.available_permits()));
        t.lines()
    }

    pub fn semaphore_close_fails_waiters_in_order() -> Vec<String> {
        let t = Trace::default();
        let semaphore = Arc::new(s::Semaphore::new(1));
        let held = Arc::clone(&semaphore).try_acquire_owned().unwrap();
        t.note(format!("owned: {}", Arc::ptr_eq(held.semaphore(), &semaphore)));
        let mut a = Box::pin(semaphore.acquire());
        let mut b = Box::pin(Arc::clone(&semaphore).acquire_many_owned(1));
        t.poll("a", a.as_mut());
        t.poll("b", b.as_mut());
        semaphore.close();
        t.note(format!("closed {}", semaphore.is_closed()));
        let error = t.poll("a", a.as_mut()).unwrap().unwrap_err();
        t.note(format!("{error}"));
        t.note(format!("b: {}", t.poll("b", b.as_mut()).unwrap().is_err()));
        let error = semaphore.try_acquire().unwrap_err();
        t.note(format!("{error:?} {error}"));
        drop(held);
        t.note(format!("available {}", semaphore.available_permits()));
        let blocking = Arc::clone(&semaphore);
        let failed = std::thread::spawn(move || blocking.blocking_acquire().is_err()).join().unwrap();
        t.note(format!("blocking failed: {failed}"));
        let open = Arc::new(s::Semaphore::new(1));
        let blocking = Arc::clone(&open);
        let permits = std::thread::spawn(move || blocking.blocking_acquire_owned().unwrap().num_permits())
            .join()
            .unwrap();
        t.note(format!("blocking: {permits} {}", open.available_permits()));
        t.lines()
    }

    pub fn notify_one_and_last_pick_oldest_and_newest() -> Vec<String> {
        let t = Trace::default();
        let notify = s::Notify::new();
        let mut first = Box::pin(notify.notified());
        let mut second = Box::pin(notify.notified());
        let mut third = Box::pin(notify.notified());
        t.poll("a", first.as_mut());
        t.poll("b", second.as_mut());
        t.poll("c", third.as_mut());
        notify.notify_one();
        notify.notify_last();
        t.poll("b", second.as_mut());
        t.poll("a", first.as_mut());
        // c was picked; dropping it passes the notification on (to the newest waiter).
        drop(third);
        t.note("c dropped");
        t.poll("b", second.as_mut());
        // No waiter: one permit is stored, and two notifications coalesce.
        notify.notify_one();
        notify.notify_one();
        let mut fourth = Box::pin(notify.notified());
        let mut fifth = Box::pin(notify.notified());
        t.poll("d", fourth.as_mut());
        t.poll("e", fifth.as_mut());
        notify.notify_last();
        t.poll("e", fifth.as_mut());
        t.poll("a", first.as_mut());
        t.lines()
    }

    pub fn notify_waiters_reaches_created_futures_in_order() -> Vec<String> {
        let t = Trace::default();
        let notify = Arc::new(s::Notify::new());
        let mut a = Box::pin(notify.notified());
        let mut b = Box::pin(Arc::clone(&notify).notified_owned());
        let mut c = Box::pin(notify.notified());
        let mut unpolled = Box::pin(notify.notified());
        let mut enabled = Box::pin(notify.notified());
        t.poll("a", a.as_mut());
        t.poll("b", b.as_mut());
        t.poll("c", c.as_mut());
        t.note(format!("enable: {}", enabled.as_mut().enable()));
        notify.notify_waiters();
        t.note("notified waiters");
        t.poll("unpolled", unpolled.as_mut());
        t.poll("enabled", enabled.as_mut());
        t.poll("c", c.as_mut());
        t.poll("b", b.as_mut());
        t.poll("a", a.as_mut());
        // It stores no permit.
        let mut later = Box::pin(notify.notified());
        t.poll("later", later.as_mut());
        // An enabled waiter can be picked without a waker.
        let mut quiet = Box::pin(notify.notified());
        t.note(format!("enable: {}", quiet.as_mut().enable()));
        notify.notify_one();
        t.poll("later", later.as_mut());
        t.poll("quiet", quiet.as_mut());
        notify.notify_one();
        t.poll("later", later.as_mut());
        t.lines()
    }

    pub fn oneshot_delivers_once() -> Vec<String> {
        let t = Trace::default();
        let (sender, mut receiver) = s::oneshot::channel::<u32>();
        t.note(format!(
            "{:?} empty {} terminated {} closed {}",
            receiver.try_recv(),
            receiver.is_empty(),
            receiver.is_terminated(),
            sender.is_closed()
        ));
        t.poll("rx", Pin::new(&mut receiver));
        sender.send(5).unwrap();
        t.note(format!("empty {}", receiver.is_empty()));
        let value = t.poll("rx", Pin::new(&mut receiver)).unwrap();
        t.note(format!("{value:?} terminated {} empty {}", receiver.is_terminated(), receiver.is_empty()));
        t.note(format!("{:?}", receiver.try_recv()));

        let (sender, mut receiver) = s::oneshot::channel::<u32>();
        sender.send(6).unwrap();
        t.note(format!("{:?} {:?}", receiver.try_recv(), receiver.try_recv()));

        let (sender, mut receiver) = s::oneshot::channel::<u32>();
        t.poll("rx", Pin::new(&mut receiver));
        drop(sender);
        let error = t.poll("rx", Pin::new(&mut receiver)).unwrap().unwrap_err();
        t.note(format!("{error:?} {error}"));

        let (sender, mut receiver) = s::oneshot::channel::<u32>();
        drop(sender);
        let error = receiver.try_recv().unwrap_err();
        t.note(format!("{error:?} {error}"));

        let (sender, receiver) = s::oneshot::channel::<u32>();
        let thread = std::thread::spawn(move || receiver.blocking_recv());
        sender.send(9).unwrap();
        t.note(format!("blocking: {:?}", thread.join().unwrap()));
        t.lines()
    }

    pub fn oneshot_close_and_closed() -> Vec<String> {
        let t = Trace::default();
        let (mut sender, mut receiver) = s::oneshot::channel::<u32>();
        t.poll("closed", Box::pin(std::future::poll_fn(|cx| sender.poll_closed(cx))).as_mut());
        receiver.close();
        t.note(format!("is_closed {}", sender.is_closed()));
        t.poll("closed", Box::pin(sender.closed()).as_mut());
        t.note(format!("{:?}", sender.send(7)));
        t.note(format!("{:?}", receiver.try_recv()));

        let (mut sender, receiver) = s::oneshot::channel::<u32>();
        let mut closed = Box::pin(sender.closed());
        t.poll("closed", closed.as_mut());
        drop(receiver);
        t.poll("closed", closed.as_mut());

        let (sender, mut receiver) = s::oneshot::channel::<u32>();
        t.poll("rx", Pin::new(&mut receiver));
        receiver.close();
        t.note(format!("{:?}", sender.send(8)));
        let result = t.poll("rx", Pin::new(&mut receiver));
        t.note(format!("{result:?}"));
        t.lines()
    }

    pub fn broadcast_wakes_receivers_in_order_and_reports_lag() -> Vec<String> {
        let t = Trace::default();
        let (sender, mut first) = s::broadcast::channel::<u32>(3);
        let mut second = sender.subscribe();
        t.note(format!("receivers {} len {} empty {}", sender.receiver_count(), sender.len(), sender.is_empty()));
        {
            let mut a = Box::pin(first.recv());
            let mut b = Box::pin(second.recv());
            t.poll("first", a.as_mut());
            t.poll("second", b.as_mut());
            t.note(format!("sent to {:?}", sender.send(1)));
            t.note(format!("{:?}", t.poll("second", b.as_mut())));
            t.note(format!("{:?}", t.poll("first", a.as_mut())));
        }
        for value in 2..=7 {
            t.note(format!("sent to {:?}", sender.send(value)));
        }
        t.note(format!("len {} {} {}", sender.len(), first.len(), first.is_empty()));
        loop {
            let received = first.try_recv();
            t.note(format!("first {received:?}"));
            if received == Err(s::broadcast::error::TryRecvError::Empty) {
                break;
            }
        }
        t.note(format!("len {} {}", sender.len(), second.len()));
        let late = second.resubscribe();
        t.note(format!("late len {} receivers {}", late.len(), sender.receiver_count()));
        drop(late);
        t.note(format!("{:?}", second.try_recv()));
        t.note(format!("{:?}", second.try_recv()));
        drop(sender);
        t.note(format!("closed {}", first.is_closed()));
        loop {
            let received = second.try_recv();
            t.note(format!("second {received:?}"));
            if matches!(received, Err(s::broadcast::error::TryRecvError::Closed)) {
                break;
            }
        }
        t.note(format!("{:?}", first.try_recv()));
        let error = first.try_recv().unwrap_err();
        t.note(format!("{error}"));
        t.lines()
    }

    pub fn broadcast_senders_and_close() -> Vec<String> {
        let t = Trace::default();
        let sender = s::broadcast::Sender::<u32>::new(2);
        let error = sender.send(1).unwrap_err();
        t.note(format!("{} {error}", error.0));
        let mut receiver = sender.subscribe();
        t.note(format!("{:?}", sender.send(2)));
        let weak = sender.downgrade();
        t.note(format!("strong {} weak {}", weak.strong_count(), sender.weak_count()));
        let clone = weak.upgrade().unwrap();
        t.note(format!("strong {} same {}", sender.strong_count(), sender.same_channel(&clone)));
        drop(clone);
        {
            let mut closed = Box::pin(sender.closed());
            t.poll("closed", closed.as_mut());
            let mut waiting = Box::pin(receiver.recv());
            t.note(format!("{:?}", t.poll("rx", waiting.as_mut())));
            drop(waiting);
            let mut waiting = Box::pin(receiver.recv());
            t.poll("rx", waiting.as_mut());
            drop(waiting);
            drop(receiver);
            t.poll("closed", closed.as_mut());
        }
        let mut receiver = sender.subscribe();
        let mut waiting = Box::pin(receiver.recv());
        t.poll("rx", waiting.as_mut());
        drop(sender);
        t.note(format!("upgrade {}", weak.upgrade().is_some()));
        t.note(format!("{:?}", t.poll("rx", waiting.as_mut())));
        drop(waiting);
        t.note(format!("strong {}", receiver.sender_strong_count()));
        let (sender, mut receiver) = s::broadcast::channel::<u32>(1);
        let thread = std::thread::spawn(move || receiver.blocking_recv());
        while sender.receiver_count() > 0 && sender.send(4).is_err() {}
        t.note(format!("blocking {:?}", thread.join().unwrap()));
        t.lines()
    }

    pub fn cancellation_reaches_children_and_waiters_in_order() -> Vec<String> {
        let t = Trace::default();
        let root = CancellationToken::new();
        let child = root.child_token();
        let grandchild = child.child_token();
        let state = |t: &Trace| {
            t.note(format!(
                "root {} child {} grandchild {}",
                root.is_cancelled(),
                child.is_cancelled(),
                grandchild.is_cancelled()
            ));
        };
        let mut waiting = Box::pin(grandchild.cancelled());
        let mut owned = Box::pin(grandchild.clone().cancelled_owned());
        let mut on_root = Box::pin(root.cancelled());
        t.poll("waiting", waiting.as_mut());
        t.poll("owned", owned.as_mut());
        t.poll("root", on_root.as_mut());
        child.cancel();
        state(&t);
        t.poll("root", on_root.as_mut());
        t.poll("owned", owned.as_mut());
        t.poll("waiting", waiting.as_mut());
        root.cancel();
        t.poll("root", on_root.as_mut());
        t.note(format!("{root:?} {:?}", root.child_token().is_cancelled()));
        t.lines()
    }

    pub fn cancellation_runs_and_guards() -> Vec<String> {
        let t = Trace::default();
        let token = CancellationToken::default();
        let ran = t.poll("run", Box::pin(token.run_until_cancelled(std::future::ready(3))).as_mut());
        t.note(format!("{ran:?}"));
        let mut pending = Box::pin(token.run_until_cancelled(std::future::pending::<()>()));
        t.poll("pending", pending.as_mut());
        token.cancel();
        t.note(format!("{:?}", t.poll("pending", pending.as_mut())));
        let ran = t.poll("cancelled", Box::pin(token.run_until_cancelled(std::future::ready(4))).as_mut());
        t.note(format!("{ran:?}"));
        let owned = CancellationToken::new();
        let ran = t.poll("owned", Box::pin(owned.run_until_cancelled_owned(std::future::ready(5))).as_mut());
        t.note(format!("{ran:?}"));

        let guarded = CancellationToken::new();
        let guard = guarded.clone().drop_guard();
        t.note(format!("guard {}", guard.token().is_cancelled()));
        drop(guard);
        t.note(format!("dropped {}", guarded.is_cancelled()));
        let kept = CancellationToken::new();
        let token = kept.clone().drop_guard().disarm();
        t.note(format!("disarmed {} {}", kept.is_cancelled(), token.is_cancelled()));
        {
            let _guard = kept.drop_guard_ref();
        }
        t.note(format!("ref dropped {}", kept.is_cancelled()));
        let spared = CancellationToken::new();
        let _ = spared.drop_guard_ref().disarm();
        t.note(format!("ref disarmed {}", spared.is_cancelled()));
        t.lines()
    }
}

compare!(
    mutex_waiters_get_the_lock_in_order,
    mutex_lock_given_to_a_cancelled_waiter_passes_on,
    mutex_guards_map_and_own,
    rwlock_writer_is_not_overtaken,
    rwlock_writers_and_readers_alternate_in_order,
    rwlock_guards_map_and_own,
    semaphore_assigns_permits_in_order,
    semaphore_cancelled_waiter_returns_its_permits,
    semaphore_close_fails_waiters_in_order,
    notify_one_and_last_pick_oldest_and_newest,
    notify_waiters_reaches_created_futures_in_order,
    oneshot_delivers_once,
    oneshot_close_and_closed,
    broadcast_wakes_receivers_in_order_and_reports_lag,
    broadcast_senders_and_close,
    cancellation_reaches_children_and_waiters_in_order,
    cancellation_runs_and_guards,
);

/// The futures and guards are `Send` exactly when tokio's are.
#[test]
fn futures_and_guards_are_send() {
    fn send<T: Send>(_: &T) {}
    let mutex = switchy_async::sync::Mutex::new(0);
    send(&mutex.lock());
    let lock = switchy_async::sync::RwLock::new(0);
    send(&lock.read());
    send(&lock.write());
    let semaphore = switchy_async::sync::Semaphore::new(1);
    send(&semaphore.acquire());
    let notify = switchy_async::sync::Notify::new();
    send(&notify.notified());
    let (sender, mut receiver) = switchy_async::sync::broadcast::channel::<u8>(1);
    send(&receiver.recv());
    send(&sender);
    let token = switchy_async::util::CancellationToken::new();
    send(&token.cancelled());
    send(&token.run_until_cancelled(async {}));
}
