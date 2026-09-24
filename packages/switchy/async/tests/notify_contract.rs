//! Executor-independent notification semantics for both selected backends.
#![cfg(feature = "sync")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{future::Future, task::Context};
use switchy_async::sync::Notify;

#[test]
fn one_permit_is_retained_and_coalesced() {
    let notify = Notify::new();
    notify.notify_one();
    notify.notify_one();
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut first = std::pin::pin!(notify.notified());
    assert!(first.as_mut().poll(&mut cx).is_ready());
    let mut second = std::pin::pin!(notify.notified());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    notify.notify_one();
    assert!(second.as_mut().poll(&mut cx).is_ready());
}

#[test]
fn broadcast_reaches_created_waiters_but_not_future_waiters() {
    let notify = Notify::new();
    let mut first = std::pin::pin!(notify.notified());
    let mut second = std::pin::pin!(notify.notified());
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(first.as_mut().poll(&mut cx).is_pending());
    notify.notify_waiters();
    assert!(first.as_mut().poll(&mut cx).is_ready());
    assert!(second.as_mut().poll(&mut cx).is_ready());
    let mut later = std::pin::pin!(notify.notified());
    assert!(later.as_mut().poll(&mut cx).is_pending());
}

#[test]
fn cancelling_selected_waiter_transfers_its_notification() {
    let notify = Notify::new();
    let mut first = Box::pin(notify.notified());
    let mut second = Box::pin(notify.notified());
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    notify.notify_one();
    drop(first);
    assert!(second.as_mut().poll(&mut cx).is_ready());
}
