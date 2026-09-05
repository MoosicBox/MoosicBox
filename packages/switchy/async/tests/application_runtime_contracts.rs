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

#[test]
fn mpsc_async_send_is_lazy_and_completes_on_first_poll() {
    use std::future::Future as _;
    use std::task::{Context, Poll};

    let (sender, mut receiver) = switchy_async::sync::mpsc::unbounded();
    let waker = futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);
    let abandoned = sender.send_async(1_u8);
    assert!(receiver.try_recv().is_err());
    drop(abandoned);
    assert!(receiver.try_recv().is_err());

    let mut send = std::pin::pin!(sender.send_async(2));
    assert!(receiver.try_recv().is_err());
    assert!(matches!(
        send.as_mut().poll(&mut context),
        Poll::Ready(Ok(()))
    ));
    assert_eq!(receiver.try_recv().expect("delivered on first poll"), 2);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn mpsc_async_send_observes_receiver_drop_before_first_poll() {
    use std::future::Future as _;
    use std::task::{Context, Poll};

    let (sender, receiver) = switchy_async::sync::mpsc::unbounded();
    let payload = String::from("undelivered");
    let mut send = std::pin::pin!(sender.send_async(payload));
    drop(receiver);
    let waker = futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);
    let Poll::Ready(Err(error)) = send.as_mut().poll(&mut context) else {
        panic!("closed unbounded send must fail on its first poll");
    };
    let switchy_async::sync::mpsc::SendError::Disconnected(payload) = error;
    assert_eq!(payload, "undelivered");
}

#[test]
fn mpmc_unpolled_send_is_lazy_and_releases_payload() {
    for (capacity, prefill) in [
        (None, false),
        (None, true),
        (Some(1), false),
        (Some(1), true),
        (Some(2), true),
    ] {
        let (sender, receiver) = capacity.map_or_else(
            switchy_async::sync::mpmc::unbounded,
            switchy_async::sync::mpmc::bounded,
        );
        let queued = Arc::new(String::from("queued"));
        // Cover empty, full, partially filled and unbounded queues.
        if prefill {
            sender.try_send(queued.clone()).unwrap();
        }
        let payload = Arc::new(String::from("never polled"));
        let weak = Arc::downgrade(&payload);
        let send = sender.send_async(payload);
        assert!(weak.upgrade().is_some());
        drop(send);
        assert!(weak.upgrade().is_none());
        if prefill {
            assert!(Arc::ptr_eq(&receiver.try_recv().unwrap(), &queued));
        }
        assert!(receiver.try_recv().is_err());
        drop(sender);
        let waker = futures::task::noop_waker();
        let mut context = std::task::Context::from_waker(&waker);
        assert!(matches!(
            receiver.poll_recv(&mut context),
            std::task::Poll::Ready(None)
        ));
    }
}

#[test]
fn mpmc_full_queue_send_yields_and_resumes_after_receive() {
    use std::future::Future as _;
    use std::task::{Context, Poll};

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(1_u8)).unwrap();
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak_signal = Arc::downgrade(&signal);
    let waker = std::task::Waker::from(signal);
    let mut context = Context::from_waker(&waker);
    let payload = Arc::new(2_u8);
    let payload_probe = Arc::downgrade(&payload);
    let mut send = Box::pin(sender.send_async(payload));
    assert!(send.as_mut().poll(&mut context).is_pending());
    let replacement = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak_replacement = Arc::downgrade(&replacement);
    let replacement_waker = std::task::Waker::from(replacement.clone());
    let mut replacement_context = Context::from_waker(&replacement_waker);
    assert!(send.as_mut().poll(&mut replacement_context).is_pending());
    drop(waker);
    assert!(weak_signal.upgrade().is_none());
    assert_eq!(*receiver.try_recv().unwrap(), 1);
    // Native sends notify on capacity release; cooperative sends may already
    // have scheduled a retry during the replacement poll.
    assert!(replacement.0.load(Ordering::SeqCst));
    assert!(matches!(
        send.as_mut().poll(&mut replacement_context),
        Poll::Ready(Ok(()))
    ));
    drop(replacement_waker);
    drop(replacement);
    assert!(weak_replacement.upgrade().is_none());
    let delivered = receiver.try_recv().unwrap();
    assert_eq!(*delivered, 2);
    assert!(std::ptr::eq(
        Arc::as_ptr(&delivered),
        payload_probe.as_ptr()
    ));
    drop(delivered);
    assert!(payload_probe.upgrade().is_none());
    // Reclamation must not depend on dropping the completed future.
    drop(send);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn mpmc_cancelled_full_queue_send_releases_payload_without_delivery() {
    use std::future::Future as _;

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(1_u8)).unwrap();
    let payload = Arc::new(2_u8);
    let weak = Arc::downgrade(&payload);
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak_signal = Arc::downgrade(&signal);
    let waker = std::task::Waker::from(signal);
    let mut context = std::task::Context::from_waker(&waker);
    let mut send = Box::pin(sender.send_async(payload));
    assert!(send.as_mut().poll(&mut context).is_pending());
    assert!(weak.upgrade().is_some());
    let replacement = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak_replacement = Arc::downgrade(&replacement);
    let replacement_waker = std::task::Waker::from(replacement);
    let mut replacement_context = std::task::Context::from_waker(&replacement_waker);
    assert!(send.as_mut().poll(&mut replacement_context).is_pending());
    drop(waker);
    assert!(weak_signal.upgrade().is_none());
    assert!(weak.upgrade().is_some());
    drop(send);
    drop(replacement_waker);
    assert!(weak_replacement.upgrade().is_none());
    assert!(weak.upgrade().is_none());
    assert_eq!(*receiver.try_recv().unwrap(), 1);
    assert!(receiver.try_recv().is_err());
    sender.try_send(Arc::new(3)).unwrap();
    assert_eq!(*receiver.try_recv().unwrap(), 3);
}

#[test]
fn mpmc_cancelled_send_after_capacity_release_preserves_single_ownership() {
    use std::future::Future as _;

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(1_u8)).unwrap();
    let payload = Arc::new(2_u8);
    let payload_probe = Arc::downgrade(&payload);
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let signal_probe = Arc::downgrade(&signal);
    let waker = std::task::Waker::from(signal.clone());
    let mut context = std::task::Context::from_waker(&waker);
    let mut send = Box::pin(sender.send_async(payload));
    assert!(send.as_mut().poll(&mut context).is_pending());
    assert_eq!(*receiver.try_recv().unwrap(), 1);
    // Capacity release may commit a native pending send before its next poll;
    // a cooperative backend may instead leave it cancellable until that poll.
    assert!(signal.0.load(Ordering::SeqCst));
    drop(send);
    drop(waker);
    drop(signal);
    assert!(signal_probe.upgrade().is_none());
    if let Ok(delivered) = receiver.try_recv() {
        assert_eq!(*delivered, 2);
        assert!(std::ptr::eq(
            Arc::as_ptr(&delivered),
            payload_probe.as_ptr()
        ));
    }
    assert!(payload_probe.upgrade().is_none());
    assert!(receiver.try_recv().is_err());
    sender.try_send(Arc::new(3)).unwrap();
    assert_eq!(*receiver.try_recv().unwrap(), 3);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn mpmc_cancelled_disconnected_send_releases_payload_and_task_state() {
    use std::future::Future as _;

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(1_u8)).unwrap();
    let payload = Arc::new(2_u8);
    let payload_probe = Arc::downgrade(&payload);
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let signal_probe = Arc::downgrade(&signal);
    let waker = std::task::Waker::from(signal.clone());
    let mut context = std::task::Context::from_waker(&waker);
    let mut send = Box::pin(sender.send_async(payload));
    assert!(send.as_mut().poll(&mut context).is_pending());
    drop(receiver);
    assert!(signal.0.load(Ordering::SeqCst));
    // Cancel instead of polling the disconnection error. The surviving sender
    // must not keep this future's payload or executor task alive.
    drop(send);
    drop(waker);
    drop(signal);
    assert!(payload_probe.upgrade().is_none());
    assert!(signal_probe.upgrade().is_none());
    let rejected = sender.try_send(Arc::new(3)).unwrap_err();
    assert_eq!(*rejected.into_inner(), 3);
}

#[test]
fn mpmc_pending_send_returns_payload_after_last_receiver_drops() {
    use std::future::Future as _;
    use std::task::Poll;

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(String::from("buffered"))).unwrap();
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak_signal = Arc::downgrade(&signal);
    let waker = std::task::Waker::from(signal);
    let mut context = std::task::Context::from_waker(&waker);
    let payload = Arc::new(String::from("pending"));
    let payload_probe = Arc::downgrade(&payload);
    let mut send = Box::pin(sender.send_async(payload));
    assert!(send.as_mut().poll(&mut context).is_pending());
    let replacement = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak_replacement = Arc::downgrade(&replacement);
    let replacement_waker = std::task::Waker::from(replacement.clone());
    let mut replacement_context = std::task::Context::from_waker(&replacement_waker);
    assert!(send.as_mut().poll(&mut replacement_context).is_pending());
    drop(waker);
    assert!(weak_signal.upgrade().is_none());
    drop(receiver);
    // Cooperative retries may already be scheduled by the replacement poll.
    assert!(replacement.0.load(Ordering::SeqCst));
    let Poll::Ready(Err(error)) = send.as_mut().poll(&mut replacement_context) else {
        panic!("pending send must observe disconnection");
    };
    assert_eq!(error.0.as_str(), "pending");
    assert!(std::ptr::eq(Arc::as_ptr(&error.0), payload_probe.as_ptr()));
    drop(error);
    assert!(payload_probe.upgrade().is_none());
    drop(replacement_waker);
    drop(replacement);
    assert!(weak_replacement.upgrade().is_none());
    // Completion must release task state before the future itself is dropped.
    drop(send);
}

#[test]
fn mpmc_cancelled_sender_does_not_block_competing_send() {
    use std::future::Future as _;
    use std::task::Poll;

    for cancel_first in [false, true] {
        let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
        sender.try_send(0_u8).unwrap();
        let other_sender = sender.clone();
        let first_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let second_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let first_waker = std::task::Waker::from(first_signal.clone());
        let second_waker = std::task::Waker::from(second_signal.clone());
        let mut first_context = std::task::Context::from_waker(&first_waker);
        let mut second_context = std::task::Context::from_waker(&second_waker);
        let mut first = Box::pin(sender.send_async(1));
        let mut second = Box::pin(other_sender.send_async(2));
        assert!(first.as_mut().poll(&mut first_context).is_pending());
        assert!(second.as_mut().poll(&mut second_context).is_pending());
        let (mut survivor, expected, signal, mut context) = if cancel_first {
            drop(first);
            (second, 2, second_signal, second_context)
        } else {
            drop(second);
            (first, 1, first_signal, first_context)
        };
        assert_eq!(receiver.try_recv().unwrap(), 0);
        // Cooperative simulators may already have scheduled a retry; native
        // backends notify on freed capacity. Either must schedule the survivor.
        assert!(signal.0.load(Ordering::SeqCst));
        assert!(matches!(
            survivor.as_mut().poll(&mut context),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(receiver.try_recv().unwrap(), expected);
        assert!(receiver.try_recv().is_err());
        drop(survivor);
        drop(sender);
        drop(other_sender);
        assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
    }
}

#[test]
fn mpmc_cancellation_after_drain_preserves_survivor_progress() {
    use std::future::Future as _;
    use std::task::{Context, Poll, Waker};

    for cancel_first in [false, true] {
        let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
        sender.try_send(0_u8).unwrap();
        let other_sender = sender.clone();
        let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let waker = Waker::from(signal.clone());
        let mut context = Context::from_waker(&waker);
        let other_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let other_waker = Waker::from(other_signal.clone());
        let mut other_context = Context::from_waker(&other_waker);
        let mut first = Box::pin(sender.send_async(1));
        let mut second = Box::pin(other_sender.send_async(2));
        assert!(first.as_mut().poll(&mut context).is_pending());
        assert!(second.as_mut().poll(&mut other_context).is_pending());
        assert_eq!(receiver.try_recv().unwrap(), 0);
        let (mut survivor, expected, signal, mut context) = if cancel_first {
            drop(first);
            (second, 2, other_signal, other_context)
        } else {
            drop(second);
            (first, 1, signal, context)
        };
        // Native receive may have committed either pending send before repoll.
        // Drain any such values; cancellation need not retract committed data.
        let mut delivered = Vec::new();
        for _ in 0..2 {
            match receiver.try_recv() {
                Ok(value) => delivered.push(value),
                Err(switchy_async::sync::mpmc::TryRecvError::Empty) => break,
                Err(error) => panic!("unexpected receive error: {error:?}"),
            }
        }
        assert!(signal.0.load(Ordering::SeqCst));
        assert!(matches!(
            survivor.as_mut().poll(&mut context),
            Poll::Ready(Ok(()))
        ));
        drop(survivor);
        drop(sender);
        drop(other_sender);
        if let Ok(value) = receiver.try_recv() {
            delivered.push(value);
        }
        delivered.sort_unstable();
        assert!(delivered == vec![expected] || delivered == vec![1, 2]);
        assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
    }
}

#[test]
fn mpmc_cancelled_receive_does_not_consume_later_delivery() {
    use std::future::Future as _;
    use std::task::{Context, Poll, Waker};

    for (close_after_send, drop_cancelled_receiver, send_before_cancel) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, false),
        (false, false, true),
        (false, true, true),
        (true, false, true),
        (true, true, true),
    ] {
        let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
        let other_receiver = receiver.clone();
        let cancelled_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let live_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let cancelled_waker = Waker::from(cancelled_signal);
        let live_waker = Waker::from(live_signal.clone());
        let mut cancelled_context = Context::from_waker(&cancelled_waker);
        let mut live_context = Context::from_waker(&live_waker);
        let mut cancelled = Box::pin(receiver.recv_async());
        let mut live = Box::pin(other_receiver.recv_async());
        assert!(cancelled.as_mut().poll(&mut cancelled_context).is_pending());
        assert!(live.as_mut().poll(&mut live_context).is_pending());
        live_signal.0.store(false, Ordering::SeqCst);
        if send_before_cancel {
            sender.try_send(17_u8).unwrap();
        }
        drop(cancelled);
        let retained_receiver = if drop_cancelled_receiver {
            drop(receiver);
            None
        } else {
            Some(receiver)
        };
        if !send_before_cancel {
            sender.try_send(17_u8).unwrap();
        }
        // Send or cancellation must notify the survivor before closure can
        // supply an unrelated wake.
        assert!(live_signal.0.load(Ordering::SeqCst));
        let sender = if close_after_send {
            drop(sender);
            None
        } else {
            Some(sender)
        };
        assert_eq!(live.as_mut().poll(&mut live_context), Poll::Ready(Ok(17)));
        drop(live);
        assert!(other_receiver.try_recv().is_err());
        drop(sender);
        if let Some(receiver) = retained_receiver {
            assert_eq!(
                receiver.poll_recv(&mut cancelled_context),
                Poll::Ready(None)
            );
        }
        assert_eq!(
            other_receiver.poll_recv(&mut live_context),
            Poll::Ready(None)
        );
    }
}

#[test]
fn mpmc_receivers_drain_closed_queue_before_stable_eof() {
    use std::task::{Context, Poll};

    let (sender, first) = switchy_async::sync::mpmc::bounded(2);
    let second = first.clone();
    sender.try_send(1_u8).unwrap();
    sender.try_send(2).unwrap();
    drop(sender);
    let waker = futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);
    assert_eq!(first.poll_recv(&mut context), Poll::Ready(Some(1)));
    assert_eq!(second.poll_recv(&mut context), Poll::Ready(Some(2)));
    for _ in 0..4 {
        assert_eq!(first.poll_recv(&mut context), Poll::Ready(None));
        assert_eq!(second.poll_recv(&mut context), Poll::Ready(None));
    }
    drop(first);
    assert_eq!(second.poll_recv(&mut context), Poll::Ready(None));
}

#[test]
fn mpmc_final_sender_drop_wakes_each_pending_receiver() {
    use std::task::{Context, Poll, Waker};

    let (sender, first) = switchy_async::sync::mpmc::bounded::<u8>(1);
    let other_sender = sender.clone();
    let second = first.clone();
    let first_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let second_signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let first_waker = Waker::from(first_signal.clone());
    let second_waker = Waker::from(second_signal.clone());
    let mut first_context = Context::from_waker(&first_waker);
    let mut second_context = Context::from_waker(&second_waker);
    assert_eq!(first.poll_recv(&mut first_context), Poll::Pending);
    assert_eq!(second.poll_recv(&mut second_context), Poll::Pending);
    drop(sender);
    assert_eq!(first.poll_recv(&mut first_context), Poll::Pending);
    assert_eq!(second.poll_recv(&mut second_context), Poll::Pending);
    first_signal.0.store(false, Ordering::SeqCst);
    second_signal.0.store(false, Ordering::SeqCst);
    drop(other_sender);
    assert!(first_signal.0.load(Ordering::SeqCst));
    assert!(second_signal.0.load(Ordering::SeqCst));
    assert_eq!(first.poll_recv(&mut first_context), Poll::Ready(None));
    assert_eq!(second.poll_recv(&mut second_context), Poll::Ready(None));
}

#[test]
fn mpsc_drains_buffer_before_reporting_closed() {
    let (sender, mut receiver) = switchy_async::sync::mpsc::unbounded();
    sender.send(1).expect("open receiver");
    sender.send(2).expect("open receiver");
    drop(sender);
    assert_eq!(receiver.try_recv().expect("first queued value"), 1);
    assert_eq!(receiver.try_recv().expect("second queued value"), 2);
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    assert_eq!(
        receiver.poll_recv(&mut context),
        std::task::Poll::Ready(None)
    );
}

#[test]
fn mpsc_rejects_send_after_receiver_drop() {
    let (sender, receiver) = switchy_async::sync::mpsc::unbounded::<u8>();
    drop(receiver);
    assert!(sender.send(1).is_err());
}

struct WakeSignal(AtomicBool);

impl std::task::Wake for WakeSignal {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn mpsc_pending_receiver_wakes_on_send_and_final_sender_drop() {
    use std::task::{Context, Poll, Waker};

    let (sender, mut receiver) = switchy_async::sync::mpsc::unbounded();
    let other_sender = sender.clone();
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let waker = Waker::from(signal.clone());
    let mut context = Context::from_waker(&waker);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Pending);
    sender.send(7).expect("open receiver");
    assert!(signal.0.swap(false, Ordering::SeqCst));
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(Some(7)));
    drop(sender);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Pending);
    // Only the final sender closes the channel; registering a receiver before
    // dropping it verifies wake delivery rather than merely observing closure.
    signal.0.store(false, Ordering::SeqCst);
    drop(other_sender);
    assert!(signal.0.load(Ordering::SeqCst));
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
}

#[test]
fn mpsc_accepts_borrowed_payload_and_remains_send_sync() {
    fn send_sync<T: Send + Sync>(_: &T) {}
    let text = String::from("borrowed");
    let (sender, mut receiver) = switchy_async::sync::mpsc::unbounded();
    send_sync(&sender);
    send_sync(&receiver);
    sender.send(text.as_str()).expect("borrowed payload");
    assert_eq!(receiver.try_recv().unwrap(), "borrowed");
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_rendezvous_wakes_each_registered_receiver() {
    use std::future::Future as _;
    use std::task::{Context, Poll, Waker};
    let (sender, receiver) = switchy_async::sync::mpmc::bounded(0);
    let other = receiver.clone();
    let first = Arc::new(WakeSignal(AtomicBool::new(false)));
    let second = Arc::new(WakeSignal(AtomicBool::new(false)));
    let first_waker = Waker::from(first.clone());
    let second_waker = Waker::from(second.clone());
    let mut first_context = Context::from_waker(&first_waker);
    let mut second_context = Context::from_waker(&second_waker);
    assert_eq!(receiver.poll_recv(&mut first_context), Poll::Pending);
    assert_eq!(other.poll_recv(&mut second_context), Poll::Pending);
    let mut send = Box::pin(sender.send_async(9));
    assert!(send.as_mut().poll(&mut first_context).is_pending());
    assert!(first.0.load(Ordering::SeqCst));
    assert!(second.0.load(Ordering::SeqCst));
    assert_eq!(other.poll_recv(&mut second_context), Poll::Ready(Some(9)));
    assert!(matches!(
        send.as_mut().poll(&mut first_context),
        Poll::Ready(Ok(()))
    ));
    drop(send);
    drop(sender);
    assert_eq!(receiver.poll_recv(&mut first_context), Poll::Ready(None));
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_bounded_backpressure_resumes_after_drain() {
    use std::future::Future as _;
    use std::task::{Context, Poll, Waker};
    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(1).unwrap();
    assert!(matches!(
        sender.try_send(2),
        Err(switchy_async::sync::mpmc::TrySendError::Full(2))
    ));
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let waker = Waker::from(signal.clone());
    let mut context = Context::from_waker(&waker);
    let mut send = Box::pin(sender.send_async(3));
    assert!(send.as_mut().poll(&mut context).is_pending());
    assert_eq!(receiver.try_recv().unwrap(), 1);
    assert!(signal.0.load(Ordering::SeqCst));
    assert!(matches!(
        send.as_mut().poll(&mut context),
        Poll::Ready(Ok(()))
    ));
    assert_eq!(receiver.try_recv().unwrap(), 3);
    assert!(receiver.try_recv().is_err());
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_cancelled_pending_send_does_not_deliver() {
    use std::future::Future as _;
    use std::task::Context;
    for capacity in [0, 1] {
        let (sender, receiver) = switchy_async::sync::mpmc::bounded(capacity);
        if capacity == 1 {
            sender.try_send(1).unwrap();
        }
        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        let mut send = Box::pin(sender.send_async(2));
        assert!(send.as_mut().poll(&mut context).is_pending());
        drop(send);
        if capacity == 1 {
            assert_eq!(receiver.try_recv().unwrap(), 1);
        }
        assert!(
            receiver.try_recv().is_err(),
            "cancelled value must not remain queued"
        );
        drop(sender);
        assert_eq!(
            receiver.poll_recv(&mut context),
            std::task::Poll::Ready(None)
        );
    }
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_mixed_receive_apis_do_not_reserve_values_for_pollers() {
    use std::future::Future as _;
    use std::task::{Context, Poll};
    let (sender, receiver) = switchy_async::sync::mpmc::unbounded();
    let other = receiver.clone();
    let waker = futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Pending);
    sender.send(1).unwrap();
    assert_eq!(other.recv().unwrap(), 1);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Pending);
    let mut receive = Box::pin(other.recv_async());
    assert!(receive.as_mut().poll(&mut context).is_pending());
    drop(receive);
    sender.try_send(2).unwrap();
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(Some(2)));
    drop(other);
    drop(sender);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_blocking_rendezvous_notifies_pending_poller() {
    use std::task::{Context, Poll, Waker};
    struct Notify(std::sync::mpsc::Sender<()>);
    impl std::task::Wake for Notify {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let (sender, receiver) = switchy_async::sync::mpmc::bounded(0);
    let (notification, notified) = std::sync::mpsc::channel();
    let waker = Waker::from(Arc::new(Notify(notification)));
    let mut context = Context::from_waker(&waker);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Pending);
    let thread = std::thread::spawn(move || sender.send(19));
    let woke = notified.recv_timeout(Duration::from_secs(5));
    let value = receiver.poll_recv(&mut context);
    // Release a blocked sender before asserting, including on the failure path.
    drop(receiver);
    let result = thread.join().expect("sender thread");
    assert!(woke.is_ok(), "blocking send must notify a pending poller");
    assert_eq!(value, Poll::Ready(Some(19)));
    assert!(result.is_ok());
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_waker_can_reenter_receiver_clone_registration() {
    use std::task::{Context, Poll, Waker};
    struct Reenter {
        receiver: switchy_async::sync::mpmc::Receiver<u8>,
        called: AtomicBool,
    }
    impl std::task::Wake for Reenter {
        fn wake(self: Arc<Self>) {
            // This acquires the registry lock: wake dispatch must release it.
            drop(self.receiver.clone());
            self.called.store(true, Ordering::SeqCst);
        }
    }
    let (sender, receiver) = switchy_async::sync::mpmc::unbounded();
    let callback = Arc::new(Reenter {
        receiver: receiver.clone(),
        called: AtomicBool::new(false),
    });
    let waker = Waker::from(callback.clone());
    let mut context = Context::from_waker(&waker);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Pending);
    sender.try_send(1).unwrap();
    assert!(callback.called.load(Ordering::SeqCst));
    assert_eq!(receiver.try_recv().unwrap(), 1);
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_ready_poll_releases_task_waker() {
    use std::task::{Context, Poll, Waker};
    let (sender, receiver) = switchy_async::sync::mpmc::unbounded();
    sender.try_send(1).unwrap();
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak = Arc::downgrade(&signal);
    let waker = Waker::from(signal);
    let mut context = Context::from_waker(&waker);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(Some(1)));
    drop(waker);
    assert!(weak.upgrade().is_none(), "ready value retained task waker");
    drop(sender);
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak = Arc::downgrade(&signal);
    let waker = Waker::from(signal);
    let mut context = Context::from_waker(&waker);
    assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
    drop(waker);
    assert!(
        weak.upgrade().is_none(),
        "closed receiver retained task waker"
    );
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_receiver_drop_wakes_pending_sender_and_returns_payload() {
    use std::future::Future as _;
    use std::task::{Context, Poll, Waker};
    for capacity in [0, 1] {
        let (sender, receiver) = switchy_async::sync::mpmc::bounded(capacity);
        if capacity == 1 {
            sender.try_send(1).unwrap();
        }
        let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
        let waker = Waker::from(signal.clone());
        let mut context = Context::from_waker(&waker);
        let mut send = Box::pin(sender.send_async(7));
        assert!(send.as_mut().poll(&mut context).is_pending());
        signal.0.store(false, Ordering::SeqCst);
        drop(receiver);
        assert!(signal.0.load(Ordering::SeqCst));
        match send.as_mut().poll(&mut context) {
            Poll::Ready(Err(error)) => assert_eq!(error.0, 7),
            result => panic!("expected disconnected send with payload, got {result:?}"),
        }
    }
}

#[test]
fn mpsc_repoll_replaces_previous_task_waker() {
    use std::task::{Context, Poll, Waker};
    let (sender, mut receiver) = switchy_async::sync::mpsc::unbounded();
    let first = Arc::new(WakeSignal(AtomicBool::new(false)));
    let second = Arc::new(WakeSignal(AtomicBool::new(false)));
    let first_waker = Waker::from(first.clone());
    let second_waker = Waker::from(second.clone());
    assert_eq!(
        receiver.poll_recv(&mut Context::from_waker(&first_waker)),
        Poll::Pending
    );
    assert_eq!(
        receiver.poll_recv(&mut Context::from_waker(&second_waker)),
        Poll::Pending
    );
    sender.send(3).unwrap();
    assert!(!first.0.load(Ordering::SeqCst));
    assert!(second.0.load(Ordering::SeqCst));
    assert_eq!(receiver.try_recv().unwrap(), 3);
}

#[test]
fn mpsc_dropped_pending_receiver_releases_task_waker() {
    use std::task::{Context, Poll, Waker};
    let (sender, mut receiver) = switchy_async::sync::mpsc::unbounded::<u8>();
    let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
    let weak = Arc::downgrade(&signal);
    let waker = Waker::from(signal);
    assert_eq!(
        receiver.poll_recv(&mut Context::from_waker(&waker)),
        Poll::Pending
    );
    drop(waker);
    assert!(
        weak.upgrade().is_some(),
        "pending poll must retain its waker"
    );
    drop(receiver);
    assert!(
        weak.upgrade().is_none(),
        "sender retained abandoned task state"
    );
    assert!(sender.send(1).is_err());
}

#[cfg(not(feature = "simulator"))]
#[test]
fn mpmc_send_and_close_racing_registration_do_not_lose_wakeup() {
    use std::task::{Context, Poll, Waker};
    // Native scheduling exercises the production boundary; this is not a
    // deterministic interleaving explorer. Joining publishes the sender's
    // completed notification before the wake assertion, without sleeping.
    for deliver in [false, true] {
        for _ in 0..128 {
            let (sender, receiver) = switchy_async::sync::mpmc::unbounded();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let other = barrier.clone();
            let thread = std::thread::spawn(move || {
                other.wait();
                if deliver {
                    sender.try_send(5).unwrap();
                    Some(sender)
                } else {
                    drop(sender);
                    None
                }
            });
            let signal = Arc::new(WakeSignal(AtomicBool::new(false)));
            let waker = Waker::from(signal.clone());
            let mut context = Context::from_waker(&waker);
            barrier.wait();
            let initial = receiver.poll_recv(&mut context);
            // Keep the sender alive in the delivery case so a later close
            // notification cannot conceal a missing send notification.
            let sender = thread.join().expect("sender thread");
            let expected = if deliver { Some(5) } else { None };
            if initial.is_pending() {
                assert!(
                    signal.0.load(Ordering::SeqCst),
                    "pending receiver missed send/close notification"
                );
                assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(expected));
            } else {
                assert_eq!(initial, Poll::Ready(expected));
            }
            drop(sender);
            assert_eq!(receiver.poll_recv(&mut context), Poll::Ready(None));
        }
    }
}

#[cfg(all(feature = "tokio", feature = "simulator"))]
#[test]
fn additive_backend_features_select_simulator_runtime() {
    fn is_simulator(_: &switchy_async::simulator::runtime::Runtime) {}
    let runtime = switchy_async::Builder::new().build().unwrap();
    is_simulator(&runtime);
    runtime.wait().unwrap();
}

#[switchy_async::test]
async fn zero_interval_period_is_rejected() {
    assert!(std::panic::catch_unwind(|| switchy_async::time::interval(Duration::ZERO)).is_err());
}

#[switchy_async::test]
async fn interval_missed_ticks_retain_scheduled_deadlines() {
    #[cfg(feature = "simulator")]
    let period = Duration::from_millis(switchy_time::simulator::step_multiplier());
    #[cfg(not(feature = "simulator"))]
    let period = Duration::from_millis(10);
    assert!(!period.is_zero());
    let mut interval = switchy_async::time::interval(period);
    let first = interval.tick().await;

    #[cfg(feature = "simulator")]
    let original_step = switchy_time::simulator::current_step();
    #[cfg(feature = "simulator")]
    let _ = switchy_time::simulator::set_step(original_step + 3);
    #[cfg(not(feature = "simulator"))]
    switchy_async::time::sleep(period * 3).await;

    // Poll explicitly: missing catch-up must fail rather than hang the simulator.
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    for index in 1..=3 {
        assert_eq!(
            interval.poll_tick(&mut context),
            std::task::Poll::Ready(first + period * index)
        );
    }
    #[cfg(feature = "simulator")]
    let _ = switchy_time::simulator::set_step(original_step);
}

#[switchy_async::test]
async fn interval_reset_discards_immediately_due_tick() {
    use std::future::Future as _;

    // A long period keeps this pending without relying on tight host timing.
    let mut interval = switchy_async::time::interval(Duration::from_hours(24));
    interval.reset();
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    {
        let mut tick = Box::pin(interval.tick());
        assert!(tick.as_mut().poll(&mut context).is_pending());
    }
    // Cancelling the borrowed future must not manufacture a ready tick.
    assert!(interval.poll_tick(&mut context).is_pending());
}

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
async fn timeout_prefers_ready_work_when_deadline_is_also_elapsed() {
    use std::future::Future as _;
    use std::task::Poll;

    let ready = Arc::new(AtomicBool::new(false));
    let inner_ready = ready.clone();
    let work = std::future::poll_fn(move |_| {
        if inner_ready.load(Ordering::SeqCst) {
            Poll::Ready(73)
        } else {
            Poll::Pending
        }
    });
    #[cfg(feature = "simulator")]
    let duration = Duration::from_millis(switchy_time::simulator::step_multiplier());
    #[cfg(not(feature = "simulator"))]
    let duration = Duration::from_millis(10);
    assert!(!duration.is_zero());
    let mut timeout = Box::pin(switchy_async::time::timeout(duration, work));
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    assert!(timeout.as_mut().poll(&mut context).is_pending());

    #[cfg(feature = "simulator")]
    let original_step = switchy_time::simulator::current_step();
    #[cfg(feature = "simulator")]
    let _ = switchy_time::simulator::set_step(original_step + 2);
    #[cfg(not(feature = "simulator"))]
    switchy_async::time::sleep(duration * 2).await;

    // Do not poll the timeout between deadline expiry and making work ready.
    ready.store(true, Ordering::SeqCst);
    assert!(matches!(
        timeout.as_mut().poll(&mut context),
        Poll::Ready(Ok(73))
    ));
    drop(timeout);
    #[cfg(feature = "simulator")]
    let _ = switchy_time::simulator::set_step(original_step);
}

#[switchy_async::test]
async fn zero_timeout_allows_immediately_ready_work() {
    assert_eq!(
        switchy_async::time::timeout(Duration::ZERO, async { 17 })
            .await
            .unwrap(),
        17
    );
}

#[switchy_async::test]
async fn zero_timeout_releases_pending_work() {
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = DropSignal(dropped.clone());
    let work = async move {
        let _signal = signal;
        std::future::pending::<()>().await;
    };
    assert!(
        switchy_async::time::timeout(Duration::ZERO, work)
            .await
            .is_err()
    );
    assert!(dropped.load(Ordering::SeqCst));
}

#[switchy_async::test]
async fn ready_work_completes_before_deadline() {
    let result = switchy_async::time::timeout(Duration::from_secs(1), async { 42 }).await;
    assert_eq!(result.expect("ready work completes"), 42);
}

#[switchy_async::test]
async fn bounded_mpmc_tasks_make_progress_under_backpressure() {
    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    let (started, start) = switchy_async::sync::oneshot::channel();
    let producer = switchy_async::task::spawn(async move {
        sender.send_async(0_u8).await.unwrap();
        started.send(()).unwrap();
        for value in 1..16 {
            sender.send_async(value).await.unwrap();
        }
    });
    start.await.unwrap();
    // Give the producer an opportunity to encounter the full queue before drain.
    switchy_async::task::yield_now().await;
    for expected in 0..16 {
        assert_eq!(receiver.recv_async().await.unwrap(), expected);
        switchy_async::task::yield_now().await;
    }
    producer.await.unwrap();
    assert!(receiver.recv_async().await.is_err());
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
async fn abort_blocked_sender_releases_payload_without_delivery() {
    use std::future::Future as _;

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(1_u8)).unwrap();
    let payload = Arc::new(2_u8);
    let payload_probe = Arc::downgrade(&payload);
    let producer = sender.clone();
    let (started, observed) = switchy_async::sync::oneshot::channel();
    let handle = switchy_async::task::spawn(async move {
        let mut send = Box::pin(producer.send_async(payload));
        let mut started = Some(started);
        std::future::poll_fn(|cx| {
            let result = send.as_mut().poll(cx);
            if let Some(started) = started.take() {
                assert!(result.is_pending());
                started.send(()).expect("observer remains alive");
            }
            result
        })
        .await
        .expect("receiver remains alive");
    });
    observed.await.expect("send polled while queue full");
    handle.abort();
    assert!(handle.await.is_err());
    assert!(payload_probe.upgrade().is_none());
    assert_eq!(*receiver.try_recv().unwrap(), 1);
    assert!(receiver.try_recv().is_err());
    sender.try_send(Arc::new(3)).unwrap();
    assert_eq!(*receiver.try_recv().unwrap(), 3);
    assert!(receiver.try_recv().is_err());
}

#[switchy_async::test]
async fn timeout_blocked_sender_releases_payload_without_delivery() {
    use std::future::Future as _;

    let (sender, receiver) = switchy_async::sync::mpmc::bounded(1);
    sender.try_send(Arc::new(1_u8)).unwrap();
    let payload = Arc::new(2_u8);
    let payload_probe = Arc::downgrade(&payload);
    let mut polled = false;
    let timeout = switchy_async::time::timeout(Duration::from_millis(10), async {
        let mut send = Box::pin(sender.send_async(payload));
        std::future::poll_fn(|cx| {
            polled = true;
            let result = send.as_mut().poll(cx);
            assert!(result.is_pending(), "queue remains full until timeout");
            result
        })
        .await
    });
    #[cfg(not(feature = "simulator"))]
    let result = timeout.await;
    #[cfg(feature = "simulator")]
    let result = {
        let mut timeout = Box::pin(timeout);
        let waker = futures::task::noop_waker();
        let mut context = std::task::Context::from_waker(&waker);
        assert!(timeout.as_mut().poll(&mut context).is_pending());
        // The harness owns clock advancement; the bare runtime does not.
        let step = switchy_time::simulator::current_step();
        let _ = switchy_time::simulator::set_step(step + 100);
        let result = timeout.as_mut().poll(&mut context);
        drop(timeout);
        let _ = switchy_time::simulator::set_step(step);
        let std::task::Poll::Ready(result) = result else {
            panic!("blocked send deadline must complete after clock advancement");
        };
        result
    };
    assert!(polled, "deadline must exercise a blocked send");
    assert!(result.is_err());
    assert!(payload_probe.upgrade().is_none());
    assert_eq!(*receiver.try_recv().unwrap(), 1);
    assert!(receiver.try_recv().is_err());
    sender.try_send(Arc::new(3)).unwrap();
    assert_eq!(*receiver.try_recv().unwrap(), 3);
    assert!(receiver.try_recv().is_err());
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
