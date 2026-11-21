//! Blocking in place and nesting `Handle::block_on` on the simulator, as on a multi-thread
//! tokio runtime.
#![cfg(feature = "simulator")]

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use switchy_async::{
    RuntimeFlavor,
    simulator::{
        runtime::{Handle, Runtime},
        task,
    },
};

#[test]
fn a_task_can_block_in_place_on_a_future_that_needs_other_tasks() {
    let runtime = Runtime::new();
    assert_eq!(
        runtime.handle().runtime_flavor(),
        RuntimeFlavor::MultiThread
    );
    let value = runtime.block_on(async {
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
        let produced = Arc::new(AtomicU32::new(0));
        let producer = Arc::clone(&produced);
        task::spawn(async move {
            producer.store(1, Ordering::SeqCst);
            tx.send(21).unwrap();
        });
        let handle = Handle::current();
        // A synchronous bridge inside a task: the nested block_on runs the producer.
        let value = task::block_in_place(|| handle.block_on(async { rx.await.unwrap() }));
        assert_eq!(produced.load(Ordering::SeqCst), 1);
        value * 2
    });
    assert_eq!(value, 42);
    runtime.wait().unwrap();
}

#[test]
fn a_spawned_task_can_nest_block_on_without_polling_itself() {
    let runtime = Runtime::new();
    let value = runtime.block_on(async {
        let outer = task::spawn(async {
            let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
            task::spawn(async move { tx.send(5).unwrap() });
            let handle = Handle::current();
            task::block_in_place(|| handle.block_on(async { rx.await.unwrap() })) + 1
        });
        outer.await.unwrap()
    });
    assert_eq!(value, 6);
    runtime.wait().unwrap();
}

#[test]
fn entering_a_runtime_makes_it_current_on_another_thread() {
    let runtime = Runtime::new();
    let handle = runtime.handle();
    let value = std::thread::spawn(move || {
        assert!(Handle::try_current().is_err());
        let guard = handle.enter();
        let current = Handle::current();
        let value = current.block_on(async { 9 });
        drop(guard);
        assert!(Handle::try_current().is_err());
        value
    })
    .join()
    .unwrap();
    assert_eq!(value, 9);
}
