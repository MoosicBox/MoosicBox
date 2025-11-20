#![cfg(all(feature = "_any_backend", feature = "macros", feature = "sync"))]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use switchy_async::sync::Barrier;

#[switchy_async::test]
async fn test_barrier_basic_functionality() {
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = vec![];

    for i in 0..3 {
        let b = barrier.clone();
        handles.push(switchy_async::task::spawn(async move {
            let result = b.wait().await;
            (i, result.is_leader())
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();

    // Exactly one leader
    let leader_count = results.iter().filter(|(_, is_leader)| *is_leader).count();
    assert_eq!(leader_count, 1);

    // All tasks completed
    assert_eq!(results.len(), 3);
}

#[switchy_async::test]
async fn test_barrier_reusability() {
    let barrier = Arc::new(Barrier::new(2));

    // First cycle
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let (r1, r2) = switchy_async::join!(
        async move { b1.wait().await },
        async move { b2.wait().await }
    );

    assert_ne!(r1.is_leader(), r2.is_leader());

    // Second cycle
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let (r3, r4) = switchy_async::join!(
        async move { b1.wait().await },
        async move { b2.wait().await }
    );

    assert_ne!(r3.is_leader(), r4.is_leader());

    // Third cycle
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let (r5, r6) = switchy_async::join!(
        async move { b1.wait().await },
        async move { b2.wait().await }
    );

    assert_ne!(r5.is_leader(), r6.is_leader());
}

#[switchy_async::test]
async fn test_barrier_single_task() {
    let barrier = Barrier::new(1);
    let result = barrier.wait().await;
    assert!(result.is_leader());
}

#[switchy_async::test]
async fn test_barrier_large_group() {
    let n = 10;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = vec![];

    for i in 0..n {
        let b = barrier.clone();
        handles.push(switchy_async::task::spawn(async move {
            let result = b.wait().await;
            (i, result.is_leader())
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();

    // Exactly one leader
    let leader_count = results.iter().filter(|(_, is_leader)| *is_leader).count();
    assert_eq!(leader_count, 1);

    // All tasks completed
    assert_eq!(results.len(), n);
}

#[switchy_async::test]
async fn test_barrier_synchronization() {
    let barrier = Arc::new(Barrier::new(3));
    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    for i in 0..3 {
        let b = barrier.clone();
        let c = counter.clone();
        handles.push(switchy_async::task::spawn(async move {
            // Phase 1: Do work before barrier
            let before = c.fetch_add(1, Ordering::SeqCst);

            // Wait at barrier
            let result = b.wait().await;

            // Phase 2: Do work after barrier
            let after = c.fetch_add(1, Ordering::SeqCst);

            (i, before, after, result.is_leader())
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();

    // All "before" work should complete before any "after" work
    let max_before = results
        .iter()
        .map(|(_, before, _, _)| *before)
        .max()
        .unwrap();
    let min_after = results.iter().map(|(_, _, after, _)| *after).min().unwrap();
    assert!(
        max_before < min_after,
        "Before work: max={}, After work: min={}",
        max_before,
        min_after
    );

    // Exactly one leader
    let leader_count = results
        .iter()
        .filter(|(_, _, _, is_leader)| *is_leader)
        .count();
    assert_eq!(leader_count, 1);
}

#[switchy_async::test]
async fn test_barrier_with_async_work() {
    let barrier = Arc::new(Barrier::new(2));
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));

    let b1 = barrier.clone();
    let r1 = results.clone();
    let task1 = switchy_async::task::spawn(async move {
        // Simulate async work
        switchy_async::task::yield_now().await;
        r1.lock().unwrap().push("task1_before".to_string());

        let result = b1.wait().await;

        r1.lock().unwrap().push("task1_after".to_string());
        result.is_leader()
    });

    let b2 = barrier.clone();
    let r2 = results.clone();
    let task2 = switchy_async::task::spawn(async move {
        // Simulate async work
        switchy_async::task::yield_now().await;
        r2.lock().unwrap().push("task2_before".to_string());

        let result = b2.wait().await;

        r2.lock().unwrap().push("task2_after".to_string());
        result.is_leader()
    });

    let (leader1, leader2) = futures::future::join(task1, task2).await;
    let leader1 = leader1.unwrap();
    let leader2 = leader2.unwrap();

    // Exactly one leader
    assert_ne!(leader1, leader2);

    // Check that all "before" events happened before all "after" events
    let events = results.lock().unwrap().clone();
    assert_eq!(events.len(), 4);

    let before_count = events
        .iter()
        .take(2)
        .filter(|s| s.contains("before"))
        .count();
    let after_count = events
        .iter()
        .skip(2)
        .filter(|s| s.contains("after"))
        .count();
    assert_eq!(before_count, 2);
    assert_eq!(after_count, 2);
}

#[switchy_async::test]
async fn test_barrier_multiple_cycles() {
    let barrier = Arc::new(Barrier::new(3));
    let cycle_results = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Run 3 cycles
    for cycle in 0..3 {
        let mut handles = vec![];

        for task_id in 0..3 {
            let b = barrier.clone();
            let results = cycle_results.clone();

            handles.push(switchy_async::task::spawn(async move {
                let result = b.wait().await;

                results
                    .lock()
                    .unwrap()
                    .push((cycle, task_id, result.is_leader()));
                result.is_leader()
            }));
        }

        let leaders: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect();

        // Exactly one leader per cycle
        let leader_count = leaders.iter().filter(|&&x| x).count();
        assert_eq!(leader_count, 1);
    }

    let all_results = cycle_results.lock().unwrap().clone();
    assert_eq!(all_results.len(), 9); // 3 cycles * 3 tasks

    // Check each cycle had exactly one leader
    for cycle in 0..3 {
        let cycle_leaders = all_results
            .iter()
            .filter(|(c, _, _)| *c == cycle)
            .filter(|(_, _, is_leader)| *is_leader)
            .count();
        assert_eq!(cycle_leaders, 1);
    }
}

#[switchy_async::test]
async fn test_barrier_stress() {
    // Test with a larger number of tasks
    let n = 20;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = vec![];

    for i in 0..n {
        let b = barrier.clone();
        handles.push(switchy_async::task::spawn(async move {
            // Add some randomness with yields
            for _ in 0..i % 3 {
                switchy_async::task::yield_now().await;
            }

            let result = b.wait().await;
            (i, result.is_leader())
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();

    // Exactly one leader
    let leader_count = results.iter().filter(|(_, is_leader)| *is_leader).count();
    assert_eq!(leader_count, 1);

    // All tasks completed
    assert_eq!(results.len(), n);
}

#[test]
#[should_panic(expected = "barrier size must be positive")]
fn test_barrier_zero_size_panics() {
    let _ = Barrier::new(0);
}
