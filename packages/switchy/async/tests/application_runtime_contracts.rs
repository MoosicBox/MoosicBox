//! Runtime contracts needed by applications that migrate between production and simulation.
//!
//! Run this target separately with `tokio,macros,sync,time` and
//! `simulator,macros,sync,time`. These tests do not certify host crash behavior.

#![cfg(all(
    feature = "_any_backend",
    feature = "macros",
    feature = "sync",
    feature = "time"
))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[switchy_async::test]
async fn timeout_drops_pending_work() {
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = DropSignal(dropped.clone());
    let work = async move {
        let _signal = signal;
        std::future::pending::<()>().await;
    };
    let timeout = switchy_async::time::timeout(Duration::from_millis(2), work);
    #[cfg(not(feature = "simulator"))]
    assert!(timeout.await.is_err());
    #[cfg(feature = "simulator")]
    {
        use std::future::Future as _;
        use std::task::{Context, Poll};

        // The bare runtime does not advance simulated time: the harness owns steps.
        // Poll explicitly so a missing deadline transition fails instead of hanging.
        let mut timeout = Box::pin(timeout);
        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        assert!(timeout.as_mut().poll(&mut context).is_pending());
        let step = switchy_time::simulator::current_step();
        let _ = switchy_time::simulator::set_step(step + 10);
        assert!(matches!(
            timeout.as_mut().poll(&mut context),
            Poll::Ready(Err(_))
        ));
        drop(timeout);
        let _ = switchy_time::simulator::set_step(step);
    }
    assert!(
        dropped.load(Ordering::SeqCst),
        "deadline must release pending work"
    );
}

#[switchy_async::test]
async fn ready_work_completes_before_deadline() {
    let result = switchy_async::time::timeout(Duration::from_secs(1), async { 42 }).await;
    assert_eq!(result.expect("ready work completes"), 42);
}

#[switchy_async::test]
async fn spawned_work_delivers_once_and_releases_owned_state() {
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = DropSignal(dropped.clone());
    let (sender, receiver) = switchy_async::sync::oneshot::channel();
    let handle = switchy_async::task::spawn(async move {
        let _signal = signal;
        switchy_async::task::yield_now().await;
        sender.send(42).expect("receiver remains alive");
    });
    assert_eq!(receiver.await.expect("delivery"), 42);
    handle.await.expect("join");
    assert!(dropped.load(Ordering::SeqCst));
}

#[switchy_async::test]
async fn abort_releases_pending_task_and_join_returns_error() {
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = DropSignal(dropped.clone());
    let (started, receiver) = switchy_async::sync::oneshot::channel();
    let handle = switchy_async::task::spawn(async move {
        let _signal = signal;
        started.send(()).expect("start observer");
        std::future::pending::<()>().await;
    });
    receiver.await.expect("task started");
    handle.abort();
    handle.abort();
    assert!(handle.await.is_err());
    assert!(dropped.load(Ordering::SeqCst));
}

#[switchy_async::test]
async fn abort_preserves_completed_result() {
    let handle = switchy_async::task::spawn(async { 42 });
    // Simulator caches the received result when checking completion.
    #[cfg(feature = "simulator")]
    let mut handle = handle;
    while !handle.is_finished() {
        switchy_async::task::yield_now().await;
    }
    handle.abort();
    assert_eq!(handle.await.expect("completed result"), 42);
}

#[cfg(feature = "simulator")]
#[switchy_async::test]
async fn local_task_can_spawn_and_join_another_local_task() {
    let value = std::rc::Rc::new(42);
    let parent = switchy_async::task::spawn_local(async move {
        let child = switchy_async::task::spawn_local(async move { *value });
        child.await.expect("nested child joins")
    });
    assert_eq!(parent.await.expect("parent joins"), 42);
}

#[cfg(feature = "simulator")]
#[switchy_async::test]
async fn cancelled_local_task_destructor_can_spawn_local_work() {
    struct SpawnOnDrop(Option<switchy_async::sync::oneshot::Sender<()>>);
    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            let sender = self.0.take().expect("drop once");
            drop(switchy_async::task::spawn_local(async move {
                let _ = sender.send(());
            }));
        }
    }
    let (sender, receiver) = switchy_async::sync::oneshot::channel();
    let cleanup = SpawnOnDrop(Some(sender));
    let (started, observed) = switchy_async::sync::oneshot::channel();
    let handle = switchy_async::task::spawn_local(async move {
        let _cleanup = cleanup;
        started.send(()).expect("observer");
        std::future::pending::<()>().await;
    });
    observed.await.expect("started");
    handle.abort();
    assert!(handle.await.is_err());
    receiver.await.expect("cleanup task completed");
}

#[cfg(feature = "simulator")]
#[switchy_async::test]
async fn abort_local_task_releases_non_send_state() {
    let owned = std::rc::Rc::new(());
    let weak = std::rc::Rc::downgrade(&owned);
    let (started, receiver) = switchy_async::sync::oneshot::channel();
    let handle = switchy_async::task::spawn_local(async move {
        started.send(()).expect("start observer");
        std::future::pending::<()>().await;
        drop(owned);
    });
    receiver.await.expect("local task started");
    handle.abort();
    assert!(handle.await.is_err());
    assert!(weak.upgrade().is_none());
}

#[switchy_async::test]
async fn dropping_join_handle_does_not_cancel_task() {
    let (sender, receiver) = switchy_async::sync::oneshot::channel();
    let handle = switchy_async::task::spawn(async move {
        switchy_async::task::yield_now().await;
        sender.send(42).expect("receiver alive");
    });
    drop(handle);
    assert_eq!(receiver.await.expect("detached task completes"), 42);
}

#[switchy_async::test]
async fn dropping_sender_wakes_receiver_with_closed_result() {
    let (sender, receiver) = switchy_async::sync::oneshot::channel::<()>();
    let handle = switchy_async::task::spawn(async move {
        switchy_async::task::yield_now().await;
        drop(sender);
    });
    assert!(receiver.await.is_err());
    handle.await.expect("join");
}
