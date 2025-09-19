//! Test switchy::unsync::join! access pattern

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_join_works() {
    use std::time::Duration;

    let (a, b, c) = switchy::unsync::join!(
        async {
            switchy::unsync::time::sleep(Duration::from_millis(5)).await;
            1
        },
        async {
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
            2
        },
        async {
            switchy::unsync::time::sleep(Duration::from_millis(15)).await;
            3
        }
    );

    assert_eq!(a + b + c, 6);
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_try_join_error_handling() {
    let result = switchy::unsync::try_join!(async { Ok::<_, String>(42) }, async {
        Err::<i32, _>("error".to_string())
    });

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "error".to_string());
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_join_concurrent_execution() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    let counter = Arc::new(AtomicU32::new(0));
    let c1 = counter.clone();
    let c2 = counter.clone();

    let (val1, val2) = switchy::unsync::join!(
        async move {
            c1.fetch_add(1, Ordering::SeqCst);
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
            c1.load(Ordering::SeqCst)
        },
        async move {
            c2.fetch_add(1, Ordering::SeqCst);
            switchy::unsync::time::sleep(Duration::from_millis(10)).await;
            c2.load(Ordering::SeqCst)
        }
    );

    // Both should see the counter at 2 since they run concurrently
    assert_eq!(val1, 2);
    assert_eq!(val2, 2);
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_try_join_success() {
    let result = switchy::unsync::try_join!(
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

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_join_with_tasks() {
    // Test that switchy::unsync::join! correctly awaits spawned tasks
    let (result1, result2) = switchy::unsync::join!(
        switchy::unsync::task::spawn(async { 42 }),
        switchy::unsync::task::spawn(async { "spawned" })
    );

    assert_eq!(result1.unwrap(), 42);
    assert_eq!(result2.unwrap(), "spawned");
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_join_empty() {
    // Test edge case: join! with no arguments
    switchy::unsync::join!();
}

#[cfg(feature = "async")]
#[switchy::unsync::test(real_time)]
async fn unsync_try_join_with_mixed_results() {
    // Test try_join! with mix of Ok and Err that all succeed
    let result = switchy::unsync::try_join!(
        async { Ok::<i32, String>(1) },
        async { Ok::<i32, String>(2) },
        async { Ok::<i32, String>(3) }
    );

    assert_eq!(result.unwrap(), (1, 2, 3));

    // Test try_join! where middle future fails
    let result = switchy::unsync::try_join!(
        async { Ok::<i32, String>(1) },
        async { Err::<i32, String>("middle failure".to_string()) },
        async { Ok::<i32, String>(3) }
    );

    assert_eq!(result.unwrap_err(), "middle failure");
}
