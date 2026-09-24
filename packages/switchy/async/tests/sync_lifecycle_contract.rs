//! Lock and single-delivery lifecycle contracts for the selected backend.
#![cfg(all(feature = "sync", feature = "macros", feature = "time"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{future::Future, task::Context};
use switchy_async::sync::{Mutex, oneshot};

#[switchy_async::test(real_time)]
async fn cancelled_mutex_waiter_does_not_block_next_waiter() {
    let mutex = Mutex::new(0);
    let guard = mutex.lock().await;
    let mut first = Box::pin(mutex.lock());
    let mut next = Box::pin(mutex.lock());
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(next.as_mut().poll(&mut cx).is_pending());
    drop(first);
    drop(guard);
    let mut guard = next.await;
    *guard = 7;
    drop(guard);
    assert_eq!(*mutex.lock().await, 7);
}

#[switchy_async::test(real_time)]
async fn oneshot_close_preserves_sent_value_and_rejects_later_send() {
    let (sender, mut receiver) = oneshot::channel();
    sender.send(7).unwrap();
    receiver.close();
    assert_eq!(receiver.await, Ok(7));

    let (sender, mut receiver) = oneshot::channel();
    receiver.close();
    assert_eq!(sender.send(9), Err(9));
    assert!(receiver.await.is_err());
}

#[switchy_async::test(real_time)]
async fn dropping_sender_wakes_pending_receiver_with_error() {
    let (sender, receiver) = oneshot::channel::<()>();
    let mut receiver = std::pin::pin!(receiver);
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(receiver.as_mut().poll(&mut cx).is_pending());
    drop(sender);
    assert!(receiver.await.is_err());
}
