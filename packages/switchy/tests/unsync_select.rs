//! Test switchy::unsync::select! access pattern
//!
//! Note: These tests use only features that work in both Tokio and simulator modes.
//! Some Tokio-specific features like `else` branches and conditional guards
//! are not available in simulator mode.

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_returns_first_completed_future() {
    use std::time::Duration;

    let result = switchy::unsync::select! {
        val = async {
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
            1
        } => val,
        _ = async {
            switchy::unsync::time::sleep(Duration::from_millis(100)).await;
        } => unreachable!("slower future should not complete first"),
    };

    assert_eq!(result, 1);
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_cancels_other_branches() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let slow_started = Arc::new(AtomicBool::new(false));
    let slow_started_clone = slow_started.clone();

    let result = switchy::unsync::select! {
        val = async {
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
            42
        } => val,
        _ = async move {
            slow_started_clone.store(true, Ordering::SeqCst);
            switchy::unsync::time::sleep(Duration::from_millis(500)).await;
            panic!("slow branch should not complete");
        } => unreachable!(),
    };

    assert_eq!(result, 42);
    // The slow branch should have started but not completed
    assert!(slow_started.load(Ordering::SeqCst));
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_with_immediate_completion() {
    // Test select! when one branch completes immediately
    let result = switchy::unsync::select! {
        val = async { 100 } => val,
        _ = switchy::unsync::time::sleep(std::time::Duration::from_secs(10)) => {
            unreachable!("sleep should not complete first")
        }
    };

    assert_eq!(result, 100);
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_loop_pattern() {
    use std::time::Duration;

    let mut counter = 0;
    let target = 3;

    loop {
        switchy::unsync::select! {
            _ = async {
                switchy::unsync::time::sleep(Duration::from_millis(5)).await;
            } => {
                counter += 1;
                if counter >= target {
                    break;
                }
            }
        }
    }

    assert_eq!(counter, target);
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_with_reference_capture() {
    use std::time::Duration;

    let data = [1, 2, 3];

    let result = switchy::unsync::select! {
        _ = async {
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
        } => data.len(),
        _ = async {
            switchy::unsync::time::sleep(Duration::from_millis(100)).await;
        } => unreachable!(),
    };

    assert_eq!(result, 3);
    // data should still be accessible after select!
    assert_eq!(data.len(), 3);
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_multiple_same_time() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    // Test that when both futures are ready at approximately the same time,
    // one of them is selected (verifies no deadlock/hang occurs)
    let counter = Arc::new(AtomicU32::new(0));
    let c1 = counter.clone();
    let c2 = counter.clone();

    // Run this test multiple times to verify consistency
    for _ in 0..3 {
        switchy::unsync::select! {
            _ = async {
                c1.fetch_add(1, Ordering::SeqCst);
                switchy::unsync::time::sleep(Duration::from_millis(5)).await;
            } => {},
            _ = async {
                c2.fetch_add(1, Ordering::SeqCst);
                switchy::unsync::time::sleep(Duration::from_millis(5)).await;
            } => {},
        }
    }

    // Counter should have been incremented at least 3 times (once per iteration)
    // and at most 6 times (both branches could start before one completes)
    let final_count = counter.load(Ordering::SeqCst);
    assert!(
        (3..=6).contains(&final_count),
        "Counter should be between 3 and 6, got {final_count}"
    );
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_select_with_different_return_types() {
    use std::time::Duration;

    // Test that select! correctly handles branches with the same return type
    // but different value types in the futures
    let result: String = switchy::unsync::select! {
        val = async {
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
            "fast".to_string()
        } => val,
        val = async {
            switchy::unsync::time::sleep(Duration::from_millis(100)).await;
            "slow".to_string()
        } => val,
    };

    assert_eq!(result, "fast");
}
