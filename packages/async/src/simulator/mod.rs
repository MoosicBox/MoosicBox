pub mod futures;
pub mod runtime;
pub mod task;

#[cfg(feature = "io")]
pub mod io;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(feature = "time")]
pub mod time;
#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "macros")]
#[macro_export]
macro_rules! select {
    ($($tokens:tt)*) => {
        $crate::select_internal! {
            @path = $crate;
            $($tokens)*
        }
    };
}

#[cfg(feature = "macros")]
pub use select;

#[cfg(feature = "macros")]
#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::runtime::Builder;

    use super::runtime::build_runtime;

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_await_time_future() {
        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(super::time::sleep(Duration::from_millis(10)));

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_future() {
        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(10)) => {},
                }
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_future_with_auto_fusing() {
        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                // Test that our custom select! macro auto-fuses futures
                let sleep_future = super::time::sleep(Duration::from_millis(10));
                crate::select! {
                    () = sleep_future => {},
                }
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_with_stream_like_future() {
        use futures::{StreamExt, stream};

        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                // Test that our custom select! macro works with stream-like futures
                let mut stream = Box::new(stream::iter(vec![1, 2, 3]));
                let timeout = super::time::sleep(Duration::from_millis(100));

                crate::select! {
                    item = stream.next() => {
                        assert_eq!(item, Some(1));
                    },
                    () = timeout => {
                        panic!("Should have selected stream item");
                    },
                }
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_with_complex_patterns() {
        use futures::{StreamExt, stream};

        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                // Test complex patterns like the ones used in stream_utils
                let mut stream = Box::new(stream::iter(vec![Ok::<i32, &str>(42)]));
                let timeout1 = super::time::sleep(Duration::from_millis(100));
                let timeout2 = super::time::sleep(Duration::from_millis(200));

                let result = crate::select! {
                    item = stream.next() => item,
                    () = timeout1 => {
                        log::debug!("Timeout 1");
                        None
                    }
                    () = timeout2 => {
                        log::debug!("Timeout 2");
                        None
                    }
                };

                assert_eq!(result, Some(Ok(42)));
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_with_while_let_pattern() {
        use futures::{StreamExt, stream};

        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                // Test the while let pattern used in stream_utils
                let mut stream = Box::new(stream::iter(vec!["data1", "data2"]));
                let timeout1 = super::time::sleep(Duration::from_millis(100));
                let timeout2 = super::time::sleep(Duration::from_millis(200));

                let mut results = Vec::new();
                while let Some(item) = crate::select! {
                    resp = stream.next() => resp,
                    () = timeout1 => {
                        log::debug!("Timeout 1");
                        None
                    }
                    () = timeout2 => {
                        log::debug!("Timeout 2");
                        None
                    }
                } {
                    results.push(item);
                }

                assert_eq!(results.len(), 2);
                assert_eq!(results[0], "data1");
                assert_eq!(results[1], "data2");
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test(crate::internal_test(real_time))]
    async fn timeout_completes_before_deadline() {
        // Fast future should complete before timeout
        let result = super::time::timeout(
            Duration::from_millis(100),
            super::time::sleep(Duration::from_millis(10)),
        )
        .await;

        assert!(result.is_ok());
    }

    #[cfg(feature = "time")]
    #[test_log::test(crate::internal_test(real_time))]
    async fn timeout_expires_before_completion() {
        // Slow future should timeout
        let result = super::time::timeout(
            Duration::from_millis(10),
            super::time::sleep(Duration::from_millis(100)),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), super::time::Elapsed);
    }

    #[cfg(feature = "time")]
    #[test_log::test(crate::internal_test(real_time))]
    async fn timeout_works_with_select() {
        // Test timeout in select! branches
        crate::select! {
            result = super::time::timeout(
                Duration::from_millis(50),
                super::time::sleep(Duration::from_millis(10))
            ) => {
                assert!(result.is_ok());
            },
            () = super::time::sleep(Duration::from_millis(100)) => {
                panic!("Should have selected timeout branch");
            }
        }
    }

    #[cfg(feature = "time")]
    #[test_log::test(crate::internal_test(real_time))]
    async fn timeout_can_be_cancelled() {
        use std::future::pending;

        // Test that dropping timeout future works correctly
        let timeout_future = super::time::timeout(Duration::from_millis(100), pending::<()>());

        // Create and immediately drop the timeout
        #[allow(clippy::drop_non_drop)]
        drop(timeout_future);

        // Should not panic or cause issues
    }

    #[cfg(feature = "time")]
    #[test_log::test(crate::internal_test(real_time))]
    async fn timeout_into_inner_works() {
        use std::future::ready;

        // Test that into_inner returns the original future
        let original_future = ready(42);
        let timeout_future = super::time::timeout(Duration::from_millis(100), original_future);

        let inner_future = timeout_future.into_inner();
        let result = inner_future.await;
        assert_eq!(result, 42);
    }

    #[cfg(feature = "time")]
    #[test_log::test(crate::internal_test(real_time))]
    async fn test_new_macro_syntax_works() {
        use std::time::Duration;

        // Test that the new macro syntax works with real time
        let start = std::time::Instant::now();
        super::time::sleep(Duration::from_millis(10)).await;
        let elapsed = start.elapsed();

        // Should have actually slept for ~10ms
        assert!(elapsed >= Duration::from_millis(8)); // Allow some tolerance
        assert!(elapsed < Duration::from_millis(50)); // But not too much
    }

    #[cfg(feature = "time")]
    #[crate::internal_test]
    async fn test_simulated_time_behavior() {
        use std::time::Duration;

        // Test that without real_time, time doesn't advance automatically
        let start_time = switchy_time::now();

        // This would hang forever if we actually waited, but since we're in
        // simulated time mode, we can test the behavior differently
        let timeout_future =
            super::time::timeout(Duration::from_millis(10), std::future::pending::<()>());

        // The timeout should be created but time won't advance
        #[allow(clippy::drop_non_drop)]
        drop(timeout_future);

        let end_time = switchy_time::now();
        // Time should not have advanced since we're in simulation mode
        assert_eq!(start_time, end_time);
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_2_futures() {
        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(10)) => {},
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                }
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_2_futures_2_block_ons() {
        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(10)) => {},
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                }
            });

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                    () = super::time::sleep(Duration::from_millis(10)) => {},
                }
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_3_futures() {
        switchy_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(1)) => {},
                    () = super::time::sleep(Duration::from_millis(10)) => {
                        panic!("Should have selected other future");
                    },
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                }
            });

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(1)) => {},
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                    () = super::time::sleep(Duration::from_millis(10)) => {
                        panic!("Should have selected other future");
                    },
                }
            });

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                    () = super::time::sleep(Duration::from_millis(1)) => {},
                    () = super::time::sleep(Duration::from_millis(10)) => {
                        panic!("Should have selected other future");
                    },
                }
            });

            runtime.block_on(async move {
                crate::select! {
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                    () = super::time::sleep(Duration::from_millis(10)) => {
                        panic!("Should have selected other future");
                    },
                    () = super::time::sleep(Duration::from_millis(1)) => {},
                }
            });

            runtime.wait().unwrap();
        });
    }
}
