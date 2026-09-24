//! Bounded channel capacity, reservation and cancellation contracts.
#![cfg(all(feature = "sync", feature = "macros", feature = "time"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use std::{future::Future, task::Context};
use switchy_async::sync::mpsc::bounded;

#[switchy_async::test(real_time)]
async fn timed_send_preserves_value_and_releases_reservation() {
    let (sender, mut receiver) = bounded::channel(1);
    sender
        .send_timeout(1, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert!(matches!(
        sender.send_timeout(2, std::time::Duration::ZERO).await,
        Err(bounded::error::SendTimeoutError::Timeout(2))
    ));
    assert_eq!(receiver.recv().await, Some(1));
    sender.send(3).await.unwrap();
    assert_eq!(receiver.recv().await, Some(3));
    receiver.close();
    assert!(matches!(
        sender.send_timeout(4, std::time::Duration::ZERO).await,
        Err(bounded::error::SendTimeoutError::Closed(4))
    ));
}

#[cfg(feature = "simulator")]
#[test]
fn timed_send_expires_only_after_simulated_clock_advances() {
    let (sender, mut receiver) = bounded::channel(1);
    sender.try_send(1).unwrap();
    let period = std::time::Duration::from_millis(switchy_time::simulator::step_multiplier());
    {
        let mut send = std::pin::pin!(sender.send_timeout(2, period));
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(send.as_mut().poll(&mut cx).is_pending());
        assert!(send.as_mut().poll(&mut cx).is_pending());
        let _ = switchy_time::simulator::next_step();
        assert!(matches!(
            send.as_mut().poll(&mut cx),
            std::task::Poll::Ready(Err(bounded::error::SendTimeoutError::Timeout(2)))
        ));
    }
    assert_eq!(receiver.try_recv().unwrap(), 1);
    assert!(receiver.try_recv().is_err());
    assert_eq!(sender.capacity(), 1);
}

#[switchy_async::test(real_time)]
async fn full_channel_backpressures_and_cancelled_send_is_not_delivered() {
    let (sender, mut receiver) = bounded::channel(1);
    sender.send(1).await.unwrap();
    assert!(matches!(
        sender.try_send(2),
        Err(bounded::error::TrySendError::Full(2))
    ));
    {
        let mut pending = std::pin::pin!(sender.send(3));
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(pending.as_mut().poll(&mut cx).is_pending());
    }
    assert_eq!(receiver.recv().await, Some(1));
    sender.send(4).await.unwrap();
    drop(sender);
    assert_eq!(receiver.recv().await, Some(4));
    assert_eq!(receiver.recv().await, None);
}

#[switchy_async::test(real_time)]
async fn close_drains_preexisting_permit_and_rejects_new_sends() {
    let (sender, mut receiver) = bounded::channel(1);
    let permit = sender.reserve().await.unwrap();
    assert_eq!(sender.capacity(), 0);
    receiver.close();
    assert!(matches!(
        sender.try_send(2),
        Err(bounded::error::TrySendError::Closed(2))
    ));
    permit.send(1);
    assert_eq!(receiver.recv().await, Some(1));
    assert_eq!(receiver.recv().await, None);
}

#[switchy_async::test(real_time)]
async fn dropping_reserved_capacity_restores_it() {
    let (sender, mut receiver) = bounded::channel(1);
    let permit = sender.clone().reserve_owned().await.unwrap();
    assert_eq!(sender.capacity(), 0);
    drop(permit);
    assert_eq!(sender.capacity(), 1);
    sender.send(1).await.unwrap();
    assert_eq!(receiver.recv().await, Some(1));
}
