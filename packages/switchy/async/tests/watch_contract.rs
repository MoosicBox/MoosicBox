//! Watch channel state and cancellation contracts for both executors.
#![cfg(all(feature = "sync", feature = "macros", feature = "time"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{future::Future, task::Context};
use switchy_async::sync::watch;

#[switchy_async::test(real_time)]
async fn biased_watch_cancellation_wins_over_ready_work() {
    let (sender, mut receiver) = watch::channel(false);
    sender.send_replace(true);
    let operation = std::pin::pin!(std::future::ready(7));
    let result = switchy_async::select! {
        biased;
        () = async {
            while !*receiver.borrow() {
                if receiver.changed().await.is_err() { break; }
            }
        } => None,
        value = operation => Some(value),
    };
    assert_eq!(result, None);
}

#[switchy_async::test(real_time)]
async fn closed_watch_cancels_pending_work() {
    let (sender, mut receiver) = watch::channel(false);
    drop(sender);
    let operation = std::pin::pin!(std::future::pending::<()>());
    switchy_async::select! {
        biased;
        result = receiver.changed() => assert!(result.is_err()),
        () = operation => panic!("pending work cannot complete"),
    }
}

#[switchy_async::test(real_time)]
async fn updates_coalesce_and_final_unseen_value_precedes_close() {
    let (sender, mut receiver) = watch::channel(0);
    sender.send_replace(1);
    sender.send_replace(2);
    drop(sender);
    receiver.changed().await.unwrap();
    assert_eq!(*receiver.borrow_and_update(), 2);
    assert!(receiver.changed().await.is_err());
}

#[switchy_async::test(real_time)]
async fn cancelled_change_does_not_mark_update_seen() {
    let (sender, mut receiver) = watch::channel(false);
    {
        let mut changed = std::pin::pin!(receiver.changed());
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(changed.as_mut().poll(&mut cx).is_pending());
    }
    sender.send_replace(true);
    assert!(receiver.has_changed().unwrap());
    receiver.changed().await.unwrap();
    assert!(*receiver.borrow());
    assert!(!receiver.has_changed().unwrap());
}

#[switchy_async::test(real_time)]
async fn late_subscriber_sees_replacement_without_rebroadcast() {
    let (sender, receiver) = watch::channel(false);
    drop(receiver);
    sender.send_replace(true);
    let mut receiver = sender.subscribe();
    assert!(*receiver.borrow_and_update());
    assert!(!receiver.has_changed().unwrap());
}
