//! Compare the root driver to futures' executor without entering either Switchy runtime.

#![cfg(any(feature = "owned-driver", feature = "_any_backend"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
// Borrowing non-Send state across a poll is part of the contract under test.
#![allow(clippy::future_not_send)]

use std::{
    future::poll_fn,
    rc::Rc,
    sync::mpsc,
    task::{Poll, Waker},
    thread,
    time::Duration,
};

// Run each potentially parking probe on a disposable thread: a lost wake must
// fail the test, not leave the harness indefinitely waiting on a join.
fn bounded(probe: impl FnOnce() + Send + 'static) {
    let (done, result) = mpsc::channel();
    let thread = thread::spawn(move || {
        probe();
        done.send(()).unwrap();
    });
    result.recv_timeout(Duration::from_secs(10)).unwrap();
    thread.join().unwrap();
}

async fn wake_contract() {
    let local = Rc::new(7);
    let mut polls = 0;
    let value = poll_fn(|cx| {
        polls += 1;
        if polls == 1 {
            cx.waker().wake_by_ref();
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(Rc::clone(&local))
        }
    })
    .await;
    assert!(Rc::ptr_eq(&local, &value));

    let (register, registered) = mpsc::channel::<Waker>();
    let (deliver, delivery) = mpsc::channel();
    let notifier = thread::spawn(move || {
        let waker = registered.recv().unwrap();
        deliver.send(42).unwrap();
        waker.wake();
    });
    let mut register = Some(register);
    let value = poll_fn(|cx| {
        if let Some(tx) = register.take() {
            tx.send(cx.waker().clone()).unwrap();
            return Poll::Pending;
        }
        delivery.try_recv().map_or(Poll::Pending, Poll::Ready)
    })
    .await;
    notifier.join().unwrap();
    assert_eq!(value, 42);
}

#[cfg(all(feature = "io", feature = "_any_backend"))]
async fn io_contract() {
    use switchy_async::io::{AsyncReadExt as _, AsyncWrite as _};

    let (mut reader, mut writer) = tokio::io::duplex(1);
    let (start, registered) = mpsc::channel();
    let producer = thread::spawn(move || {
        registered.recv().unwrap();
        let mut cx = std::task::Context::from_waker(Waker::noop());
        let written = std::pin::Pin::new(&mut writer).poll_write(&mut cx, b"xy");
        assert!(matches!(written, Poll::Ready(Ok(1))));
        // Closing the writer leaves the buffered byte readable before EOF.
    });
    let mut buf = [0; 2];
    let mut read = Box::pin(reader.read(&mut buf));
    let mut start = Some(start);
    let count = poll_fn(|cx| {
        let result = read.as_mut().poll(cx);
        if let Some(tx) = start.take() {
            assert!(result.is_pending());
            tx.send(()).unwrap();
        }
        result
    })
    .await
    .unwrap();
    drop(read);
    assert_eq!(count, 1);
    assert_eq!(buf[0], b'x');
    producer.join().unwrap();
    assert_eq!(reader.read(&mut buf).await.unwrap(), 0);
}

async fn contracts() {
    wake_contract().await;
    #[cfg(all(feature = "io", feature = "_any_backend"))]
    io_contract().await;
}

#[cfg(feature = "_any_backend")]
#[test]
fn baseline() {
    bounded(|| futures::executor::block_on(contracts()));
}

#[cfg(feature = "owned-driver")]
#[test]
fn owned() {
    bounded(|| switchy_async::owned::block_on(contracts()));
}
