//! Broadcast delivery and cancellation contracts for each selected executor.
#![cfg(all(feature = "sync", feature = "macros", feature = "time"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{future::Future, task::Context};
use switchy_async::sync::broadcast::{self, error::RecvError};

#[switchy_async::test(real_time)]
async fn lag_is_reported_then_retained_messages_drain_before_close() {
    let (sender, mut receiver) = broadcast::channel(2);
    sender.send(1).unwrap();
    sender.send(2).unwrap();
    sender.send(3).unwrap();
    drop(sender);
    assert_eq!(receiver.recv().await, Err(RecvError::Lagged(1)));
    assert_eq!(receiver.recv().await, Ok(2));
    assert_eq!(receiver.recv().await, Ok(3));
    assert_eq!(receiver.recv().await, Err(RecvError::Closed));
}

#[switchy_async::test(real_time)]
async fn late_subscription_does_not_replay_previous_messages() {
    let (sender, mut first) = broadcast::channel(2);
    sender.send(1).unwrap();
    let mut late = sender.subscribe();
    sender.send(2).unwrap();
    assert_eq!(first.recv().await, Ok(1));
    assert_eq!(late.recv().await, Ok(2));
    drop(sender);
    assert_eq!(late.recv().await, Err(RecvError::Closed));
}

#[switchy_async::test(real_time)]
async fn cancelled_receive_does_not_consume_next_message() {
    let (sender, mut receiver) = broadcast::channel(2);
    {
        let mut pending = std::pin::pin!(receiver.recv());
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(pending.as_mut().poll(&mut cx).is_pending());
    }
    sender.send(7).unwrap();
    assert_eq!(receiver.recv().await, Ok(7));
}
