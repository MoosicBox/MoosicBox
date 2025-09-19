//! Simple integration test for join!/try_join! macros

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn basic_join_test() {
    // Test basic join! functionality
    let (a, b) = switchy_async::join!(async { 42 }, async { "hello" });

    assert_eq!(a, 42);
    assert_eq!(b, "hello");
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn basic_try_join_success() {
    // Test basic try_join! success case
    let result = switchy_async::try_join!(async { Ok::<_, &str>(1) }, async { Ok::<_, &str>(2) });

    assert_eq!(result.unwrap(), (1, 2));
}

#[cfg(all(feature = "macros", feature = "time"))]
#[switchy_async::test(real_time)]
async fn basic_try_join_error() {
    // Test basic try_join! error case
    let result =
        switchy_async::try_join!(async { Err::<i32, _>("error") }, async { Ok::<_, &str>(2) });

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "error");
}
