//! Integration tests for join!/try_join! macros from external crate perspective

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn external_crate_join() {
    use std::time::Duration;

    // Test using switchy_async::join! from external crate
    let (a, b) = switchy_async::join!(
        async {
            switchy_async::time::sleep(Duration::from_millis(10)).await;
            42
        },
        async {
            switchy_async::time::sleep(Duration::from_millis(20)).await;
            "hello"
        }
    );

    assert_eq!(a, 42);
    assert_eq!(b, "hello");
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn external_crate_try_join() {
    // Test using switchy_async::try_join! from external crate
    let result = switchy_async::try_join!(async { Ok::<_, &str>(1) }, async { Ok::<_, &str>(2) });

    assert_eq!(result.unwrap(), (1, 2));
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_basic_functionality() {
    // Test basic join! functionality
    let (a, b, c) = switchy_async::join!(async { 1 }, async { "hello" }, async { true });

    assert_eq!(a, 1);
    assert_eq!(b, "hello");
    assert!(c);
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_concurrent_execution() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    let counter = Arc::new(AtomicU32::new(0));
    let c1 = counter.clone();
    let c2 = counter.clone();

    let (val1, val2) = switchy_async::join!(
        async move {
            c1.fetch_add(1, Ordering::SeqCst);
            switchy_async::time::sleep(Duration::from_millis(10)).await;
            c1.load(Ordering::SeqCst)
        },
        async move {
            c2.fetch_add(1, Ordering::SeqCst);
            switchy_async::time::sleep(Duration::from_millis(10)).await;
            c2.load(Ordering::SeqCst)
        }
    );

    // Both should see the counter at 2 since they run concurrently
    assert_eq!(val1, 2);
    assert_eq!(val2, 2);
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn try_join_success_case() {
    let result = switchy_async::try_join!(
        async { Ok::<_, &str>(1) },
        async { Ok::<_, &str>("hello") },
        async { Ok::<_, &str>(true) }
    );

    assert!(result.is_ok());
    let (a, b, c) = result.unwrap();
    assert_eq!(a, 1);
    assert_eq!(b, "hello");
    assert!(c);
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn try_join_error_case() {
    let result = switchy_async::try_join!(
        async { Err::<i32, _>("error occurred") },
        async { Ok::<_, &str>(2) },
        async { Ok::<_, &str>(3) }
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "error occurred");
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_with_task_spawn() {
    // Test that join! correctly awaits JoinHandles from spawn
    let (result1, result2) = switchy_async::join!(
        switchy_async::task::spawn(async { 42 }),
        switchy_async::task::spawn(async { "spawned" })
    );

    // Verify the spawned tasks completed successfully
    assert_eq!(result1.unwrap(), 42);
    assert_eq!(result2.unwrap(), "spawned");
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_spawned_tasks_run_concurrently() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    let counter = Arc::new(AtomicU32::new(0));
    let c1 = counter.clone();
    let c2 = counter.clone();

    // Spawn tasks that increment counter and return its value after both increment
    let (result1, result2) = switchy_async::join!(
        switchy_async::task::spawn(async move {
            c1.fetch_add(1, Ordering::SeqCst);
            // Small delay to ensure both tasks have time to increment
            switchy_async::time::sleep(Duration::from_millis(10)).await;
            c1.load(Ordering::SeqCst)
        }),
        switchy_async::task::spawn(async move {
            c2.fetch_add(1, Ordering::SeqCst);
            switchy_async::time::sleep(Duration::from_millis(10)).await;
            c2.load(Ordering::SeqCst)
        })
    );

    // Both should see counter = 2, proving they ran concurrently
    assert_eq!(result1.unwrap(), 2);
    assert_eq!(result2.unwrap(), 2);
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_preserves_order() {
    use std::time::Duration;

    // Test that join! returns results in the same order as inputs,
    // even when futures complete in different orders
    let (first, second, third) = switchy_async::join!(
        async {
            switchy_async::time::sleep(Duration::from_millis(30)).await;
            "first"
        },
        async {
            switchy_async::time::sleep(Duration::from_millis(10)).await;
            "second"
        },
        async {
            switchy_async::time::sleep(Duration::from_millis(20)).await;
            "third"
        }
    );

    // Order should be preserved despite different completion times
    assert_eq!(first, "first");
    assert_eq!(second, "second");
    assert_eq!(third, "third");
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn try_join_cancellation_behavior() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let completed = Arc::new(AtomicBool::new(false));
    let completed_clone = completed.clone();

    let result = switchy_async::try_join!(
        async {
            // This future fails quickly
            switchy_async::time::sleep(Duration::from_millis(5)).await;
            Err::<(), _>("quick error")
        },
        async move {
            // This future would take longer but should be cancelled
            switchy_async::time::sleep(Duration::from_millis(100)).await;
            completed_clone.store(true, Ordering::SeqCst);
            Ok::<(), &str>(())
        }
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "quick error");

    // Give extra time to make sure the second future really was cancelled
    switchy_async::time::sleep(Duration::from_millis(150)).await;
    assert!(
        !completed.load(Ordering::SeqCst),
        "Second future should have been cancelled"
    );
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_single_future() {
    let (result,) = switchy_async::join!(async { 42 });
    assert_eq!(result, 42);
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn join_many_futures() {
    let (a, b, c, d, e, f, g, h) = switchy_async::join!(
        async { 1 },
        async { 2 },
        async { 3 },
        async { 4 },
        async { 5 },
        async { 6 },
        async { 7 },
        async { 8 }
    );

    assert_eq!(a + b + c + d + e + f + g + h, 36);
}
