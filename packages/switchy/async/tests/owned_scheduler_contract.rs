//! Compare a caller-driven local queue with Tokio's native `LocalSet` and the simulator.
#![cfg(feature = "owned-driver")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::Cell,
    future::{pending, poll_fn},
    rc::Rc,
    sync::mpsc,
    task::{Poll, Waker},
    thread,
    time::Duration,
};
use switchy_async::owned::{self, Executor, JoinError};

fn bounded(probe: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        probe();
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("probe stalled or panicked");
    worker.join().unwrap();
}

#[cfg(feature = "tokio")]
struct Native(tokio::runtime::Runtime, tokio::task::LocalSet);
#[cfg(feature = "tokio")]
impl Native {
    fn spawn<F: Future + 'static>(&self, f: F) -> tokio::task::JoinHandle<F::Output> {
        self.1.spawn_local(f)
    }
    fn run_until<F: Future>(&self, f: F) -> F::Output {
        self.1.block_on(&self.0, f)
    }
}

macro_rules! lifecycle {
    ($rt:ident) => {{
        let (tx, rx) = mpsc::channel::<Waker>();
        let (release, released) = mpsc::channel();
        let notifier = thread::spawn(move || {
            let waker = rx.recv().unwrap();
            release.send(17).unwrap();
            waker.wake_by_ref();
            waker.wake();
        });
        let mut tx = Some(tx);
        let h = $rt.spawn(poll_fn(move |cx| {
            if let Some(tx) = tx.take() {
                tx.send(cx.waker().clone()).unwrap();
                return Poll::Pending;
            }
            released.try_recv().map_or(Poll::Pending, Poll::Ready)
        }));
        assert_eq!($rt.run_until(h).unwrap(), 17);
        notifier.join().unwrap();

        let ran = Rc::new(Cell::new(false));
        let task = ran.clone();
        drop($rt.spawn(async move {
            task.set(true);
        }));
        let capture = Rc::new(());
        let weak = Rc::downgrade(&capture);
        let started = Rc::new(Cell::new(false));
        let flag = started.clone();
        let h = $rt.spawn(async move {
            flag.set(true);
            pending::<()>().await;
            drop(capture);
        });
        $rt.run_until(poll_fn(|cx| {
            if started.get() && ran.get() {
                Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }));
        h.abort();
        h.abort();
        assert!($rt.run_until(h).is_err());
        assert!(weak.upgrade().is_none());

        let capture = Rc::new(());
        let weak = Rc::downgrade(&capture);
        let mut stale = None;
        let registered = Rc::new(std::cell::RefCell::new(None));
        let slot = registered.clone();
        let h = $rt.spawn(poll_fn(move |cx| {
            let _keep_alive = &capture;
            *slot.borrow_mut() = Some(cx.waker().clone());
            Poll::<()>::Pending
        }));
        $rt.run_until(poll_fn(|cx| {
            stale = registered.borrow_mut().take();
            if stale.is_some() {
                Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }));
        drop($rt);
        assert!(weak.upgrade().is_none());
        stale.unwrap().wake();
        assert!(owned::block_on(h).is_err());
    }};
}

#[test]
fn owned_lifecycle() {
    bounded(|| {
        let mut rt = Executor::default();
        lifecycle!(rt);
    });
}

#[cfg(feature = "tokio")]
#[test]
fn native_lifecycle() {
    bounded(|| {
        let rt = Native(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
            tokio::task::LocalSet::new(),
        );
        lifecycle!(rt);
    });
}

#[cfg(feature = "simulator")]
struct Sim(switchy_async::simulator::runtime::Runtime);
#[cfg(feature = "simulator")]
impl Sim {
    fn spawn<F: Future + 'static>(
        &self,
        f: F,
    ) -> switchy_async::simulator::runtime::JoinHandle<F::Output> {
        self.0.spawn_local(f)
    }
    fn run_until<F: Future>(&self, f: F) -> F::Output {
        self.0.block_on(f)
    }
}
#[cfg(feature = "simulator")]
impl Drop for Sim {
    fn drop(&mut self) {
        self.0.shutdown_now();
    }
}

#[cfg(feature = "simulator")]
#[test]
fn simulator_lifecycle() {
    bounded(|| {
        let rt = Sim(switchy_async::simulator::runtime::Runtime::new());
        lifecycle!(rt);
    });
}

#[test]
fn deterministic_ready_trace() {
    bounded(|| {
        let mut rt = Executor::default();
        let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut joins = Vec::new();
        for id in 0..3 {
            let events = trace.clone();
            let mut polls = 0;
            joins.push(rt.spawn(poll_fn(move |cx| {
                events.borrow_mut().push(id);
                polls += 1;
                if polls == 3 {
                    Poll::Ready(id)
                } else {
                    cx.waker().wake_by_ref();
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })));
        }
        for (id, h) in joins.into_iter().enumerate() {
            assert_eq!(rt.run_until(h), Ok(id));
        }
        assert_eq!(*trace.borrow(), [0, 1, 2, 0, 1, 2, 0, 1, 2]);
        let h = rt.spawn(async { panic!("task panic") });
        assert_eq!(rt.run_until(h), Err(JoinError::Panicked));
        let h = rt.spawn(async { 3 });
        let barrier = rt.spawn(async {});
        rt.run_until(barrier).unwrap();
        h.abort();
        assert_eq!(rt.run_until(h), Ok(3));
    });
}
