//! Test demonstrating that Barrier works identically across simulator and tokio runtimes.
//! This test can run with either backend and produces the same results.

#![cfg(all(feature = "_any_backend", feature = "macros", feature = "sync"))]

use std::sync::Arc;
use switchy_async::sync::Barrier;

#[switchy_async::test]
async fn test_barrier_cross_runtime_compatibility() {
    // This exact same code works with both simulator and tokio runtimes
    let barrier = Arc::new(Barrier::new(4));
    let mut handles = vec![];

    for i in 0..4 {
        let b = barrier.clone();
        handles.push(switchy_async::task::spawn(async move {
            // Simulate some work
            for _ in 0..i {
                switchy_async::task::yield_now().await;
            }

            println!("Task {} reaching barrier", i);
            let result = b.wait().await;
            println!("Task {} passed barrier (leader: {})", i, result.is_leader());

            (i, result.is_leader())
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();

    // Verify exactly one leader
    let leader_count = results.iter().filter(|(_, is_leader)| *is_leader).count();
    assert_eq!(leader_count, 1);

    // Verify all tasks completed
    assert_eq!(results.len(), 4);

    println!("Barrier test completed successfully with one leader");
}

#[switchy_async::test]
async fn test_barrier_reuse_across_runtime() {
    // Test barrier reuse works the same way across runtimes
    let barrier = Arc::new(Barrier::new(2));

    // First use
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let (r1, r2) = switchy_async::join!(
        async move { b1.wait().await },
        async move { b2.wait().await }
    );

    assert_ne!(r1.is_leader(), r2.is_leader());

    // Reuse the same barrier
    let b3 = barrier.clone();
    let b4 = barrier.clone();
    let (r3, r4) = switchy_async::join!(
        async move { b3.wait().await },
        async move { b4.wait().await }
    );

    assert_ne!(r3.is_leader(), r4.is_leader());

    println!("Barrier reuse test completed successfully");
}
