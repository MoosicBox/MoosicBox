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

#[switchy_async::test(real_time)]
async fn send_without_receivers_returns_the_value_but_replace_stores_it() {
    let (sender, receiver) = watch::channel(1);
    assert_eq!(sender.receiver_count(), 1);
    assert!(!sender.is_closed());
    drop(receiver);
    assert!(sender.is_closed());
    assert_eq!(sender.send(2).unwrap_err().0, 2);
    assert_eq!(*sender.borrow(), 1);
    assert_eq!(sender.send_replace(3), 1);
    assert_eq!(*sender.borrow(), 3);
    assert!(!sender.borrow().has_changed());
}

#[switchy_async::test(real_time)]
async fn unmodified_send_neither_notifies_nor_marks_changed() {
    let (sender, mut receiver) = watch::channel(5);
    assert!(!sender.send_if_modified(|value| *value == 0));
    assert!(!receiver.has_changed().unwrap());
    assert!(sender.send_if_modified(|value| {
        *value += 1;
        true
    }));
    assert!(receiver.has_changed().unwrap());
    let seen = receiver.borrow_and_update();
    assert!(seen.has_changed());
    assert_eq!(*seen, 6);
    drop(seen);
    sender.send_modify(|value| *value *= 2);
    receiver.changed().await.unwrap();
    assert_eq!(*receiver.borrow(), 12);
    assert!(!receiver.borrow().has_changed());
}

#[switchy_async::test(real_time)]
async fn mark_changed_and_unchanged_control_the_next_change() {
    let (sender, mut receiver) = watch::channel(0);
    receiver.mark_changed();
    assert!(receiver.has_changed().unwrap());
    receiver.changed().await.unwrap();
    sender.send_replace(1);
    receiver.mark_unchanged();
    assert!(!receiver.has_changed().unwrap());
    assert!(!receiver.borrow().has_changed());
}

#[switchy_async::test(real_time)]
async fn wait_for_checks_the_current_value_then_waits() {
    let (sender, mut receiver) = watch::channel(3);
    assert_eq!(*receiver.wait_for(|value| *value == 3).await.unwrap(), 3);
    let waiter = switchy_async::task::spawn(async move {
        let value = *receiver.wait_for(|value| *value >= 10).await.unwrap();
        (value, receiver)
    });
    for value in [4, 10, 11] {
        switchy_async::task::yield_now().await;
        sender.send_replace(value);
    }
    let (value, mut receiver) = waiter.await.unwrap();
    assert!(value >= 10, "{value}");
    sender.send_replace(1);
    drop(sender);
    // The last unseen value is still checked once the channel closes.
    assert_eq!(*receiver.wait_for(|value| *value == 1).await.unwrap(), 1);
    assert!(receiver.wait_for(|value| *value == 99).await.is_err());
}

#[switchy_async::test(real_time)]
async fn closed_waits_for_every_receiver() {
    let (sender, receiver) = watch::channel(());
    let second = sender.subscribe();
    let third = second.clone();
    assert_eq!(sender.receiver_count(), 3);
    assert!(second.same_channel(&receiver));
    assert!(!second.same_channel(&watch::channel(()).1));
    let other = sender.clone();
    assert_eq!(sender.sender_count(), 2);
    assert!(other.same_channel(&sender));
    drop(other);
    let dropper = switchy_async::task::spawn(async move {
        drop(receiver);
        switchy_async::task::yield_now().await;
        drop(second);
        switchy_async::task::yield_now().await;
        drop(third);
    });
    sender.closed().await;
    assert!(sender.is_closed());
    dropper.await.unwrap();
}

#[switchy_async::test(real_time)]
async fn subscriber_and_clone_inherit_seen_state() {
    let (sender, mut receiver) = watch::channel(0);
    sender.send_replace(1);
    let clone = receiver.clone();
    receiver.mark_unchanged();
    assert!(clone.has_changed().unwrap());
    assert!(!receiver.has_changed().unwrap());
    let subscriber = sender.subscribe();
    assert!(!subscriber.has_changed().unwrap());
    drop(sender);
    assert!(subscriber.is_closed());
    assert!(clone.has_changed().is_err());
    assert!(receiver.has_changed().is_err());
}

#[test]
fn futures_are_send_for_send_and_sync_values() {
    fn assert_send<T: Send>(_: &T) {}
    let (sender, mut receiver) = watch::channel(0_u32);
    assert_send(&sender.closed());
    assert_send(&receiver.changed());
    assert_send(&receiver.wait_for(|value| *value > 0));
}

/// A waker that records its index when woken.
#[cfg(feature = "simulator")]
struct Recorder {
    index: usize,
    log: std::sync::Arc<std::sync::Mutex<Vec<usize>>>,
}

#[cfg(feature = "simulator")]
impl std::task::Wake for Recorder {
    fn wake(self: std::sync::Arc<Self>) {
        self.log.lock().unwrap().push(self.index);
    }
}

/// Receivers waiting for a change are woken in the order they started waiting. tokio's watch
/// (with its `rt` or `macros` feature, as here) puts each waiter in one of eight shards chosen by
/// its thread-local generator, so its order varied from run to run.
#[cfg(feature = "simulator")]
#[test]
fn simulator_wakes_change_waiters_in_registration_order() {
    use std::sync::{Arc, Mutex};

    const WAITERS: usize = 24;
    // Registration order that is neither ascending nor descending.
    let order: Vec<usize> = (0..WAITERS).map(|i| (i * 7 + 3) % WAITERS).collect();
    for _ in 0..20 {
        let (sender, receiver) = watch::channel(0);
        let mut receivers: Vec<_> = (0..WAITERS).map(|_| receiver.clone()).collect();
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut waits: Vec<_> = receivers
            .iter_mut()
            .map(|receiver| Some(Box::pin(receiver.changed())))
            .collect();
        for &index in &order {
            let waker = Arc::new(Recorder {
                index,
                log: log.clone(),
            })
            .into();
            let mut cx = Context::from_waker(&waker);
            let wait = waits[index].as_mut().unwrap();
            assert!(wait.as_mut().poll(&mut cx).is_pending());
        }
        // A cancelled wait gives up its place.
        let cancelled = order[1];
        waits[cancelled] = None;
        sender.send_replace(1);
        let expected: Vec<usize> = order.iter().copied().filter(|&i| i != cancelled).collect();
        assert_eq!(*log.lock().unwrap(), expected);
    }
}

/// Senders waiting for the receivers to go are woken in the order they started waiting.
#[cfg(feature = "simulator")]
#[test]
fn simulator_wakes_closed_waiters_in_registration_order() {
    use std::sync::{Arc, Mutex};

    let (sender, receiver) = watch::channel(());
    let senders: Vec<_> = (0..8).map(|_| sender.clone()).collect();
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut waits: Vec<_> = senders
        .iter()
        .map(|sender| Box::pin(sender.closed()))
        .collect();
    let order = [5, 1, 7, 0, 3, 6, 2, 4];
    for index in order {
        let waker = Arc::new(Recorder {
            index,
            log: log.clone(),
        })
        .into();
        assert!(
            waits[index]
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    drop(receiver);
    assert_eq!(*log.lock().unwrap(), order);
}

/// Tasks of a seeded run that wait on one watch resume in the same order every time the seed
/// is run.
#[cfg(feature = "thread-simulator")]
#[test]
fn simulator_task_wake_order_follows_the_seed() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    fn order(seed: u64) -> Vec<usize> {
        let outcome = switchy_thread::simulator::run(seed, || {
            switchy_random::simulator::reset_rng();
            let runtime = switchy_async::simulator::runtime::Runtime::new();
            let order = runtime.block_on(async {
                let (sender, _) = watch::channel(0_u32);
                let waiting = Arc::new(AtomicUsize::new(0));
                let log = Arc::new(Mutex::new(Vec::new()));
                let tasks: Vec<_> = (0..16)
                    .map(|index| {
                        let mut receiver = sender.subscribe();
                        let waiting = waiting.clone();
                        let log = log.clone();
                        switchy_async::task::spawn(async move {
                            for _ in 0..3 {
                                waiting.fetch_add(1, Ordering::SeqCst);
                                receiver.changed().await.unwrap();
                                log.lock().unwrap().push(index);
                            }
                        })
                    })
                    .collect();
                for round in 1..=3 {
                    while waiting.load(Ordering::SeqCst) < 16 * round {
                        switchy_async::task::yield_now().await;
                    }
                    sender.send_replace(u32::try_from(round).unwrap());
                }
                for task in tasks {
                    task.await.unwrap();
                }
                Arc::try_unwrap(log).unwrap().into_inner().unwrap()
            });
            runtime.wait().unwrap();
            order
        });
        outcome
            .result
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    }
    for seed in 0..6 {
        let first = order(seed);
        assert_eq!(first.len(), 48);
        for _ in 0..3 {
            assert_eq!(first, order(seed), "seed {seed}");
        }
    }
}

/// A simulated thread holding the value lets the sending task's thread wait on the scheduler
/// rather than block, so the run finishes, the same way for a seed.
#[cfg(feature = "thread-simulator")]
#[test]
fn simulator_send_waits_for_a_simulated_thread_holding_the_value() {
    use std::time::Duration;

    fn run(seed: u64) -> (Vec<u32>, u64) {
        let outcome = switchy_thread::simulator::run(seed, || {
            switchy_random::simulator::reset_rng();
            let runtime = switchy_async::simulator::runtime::Runtime::new();
            let handle = runtime.handle();
            let values = runtime.block_on(async move {
                let (sender, receiver) = watch::channel(0_u32);
                let reader = handle.spawn_blocking(move || {
                    let mut values = Vec::new();
                    for _ in 0..4 {
                        let value = receiver.borrow();
                        switchy_thread::thread::sleep(Duration::from_millis(1));
                        values.push(*value);
                        drop(value);
                        switchy_thread::thread::yield_now();
                    }
                    values
                });
                for value in 1..=4 {
                    switchy_async::task::yield_now().await;
                    sender.send_replace(value);
                }
                reader.await.unwrap()
            });
            runtime.wait().unwrap();
            values
        });
        let trace = outcome.report.trace_hash;
        let values = outcome
            .result
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        (values, trace)
    }
    for seed in 0..10 {
        let first = run(seed);
        assert!(
            first.0.windows(2).all(|pair| pair[0] <= pair[1]),
            "{first:?}"
        );
        assert_eq!(first, run(seed), "seed {seed}");
    }
}
