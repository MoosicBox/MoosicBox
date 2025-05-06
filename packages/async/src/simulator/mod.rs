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
pub use ::futures::select;

#[cfg(feature = "macros")]
#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::runtime::Builder;

    use super::runtime::build_runtime;

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_await_time_future() {
        gimbal_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(super::time::sleep(Duration::from_millis(10)));

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_future() {
        gimbal_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                super::select! {
                    () = super::time::sleep(Duration::from_millis(10)) => {},
                }
            });

            runtime.wait().unwrap();
        });
    }

    #[cfg(feature = "time")]
    #[test_log::test]
    fn can_select_2_futures() {
        gimbal_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                super::select! {
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
        gimbal_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                super::select! {
                    () = super::time::sleep(Duration::from_millis(10)) => {},
                    () = super::time::sleep(Duration::from_millis(20)) => {
                        panic!("Should have selected other future");
                    },
                }
            });

            runtime.block_on(async move {
                super::select! {
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
        gimbal_time::simulator::with_real_time(|| {
            let runtime = build_runtime(&Builder::new()).unwrap();

            runtime.block_on(async move {
                super::select! {
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
                super::select! {
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
                super::select! {
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
                super::select! {
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
