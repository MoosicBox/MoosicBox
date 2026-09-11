#![cfg(any(feature = "native-directories", feature = "simulator"))]

use switchy_fs::directories;

#[cfg(not(feature = "simulator"))]
#[test]
fn native_locations_use_platform_conventions() {
    let home = directories::home_dir().expect("CI user has a home");
    assert!(home.is_absolute());
    let config = directories::config_dir().expect("CI user has config storage");
    let data = directories::data_dir().expect("CI user has data storage");
    assert!(config.is_absolute());
    assert!(data.is_absolute());
    assert!(directories::data_local_dir().unwrap().is_absolute());
    #[cfg(target_os = "macos")]
    assert_eq!(config, home.join("Library/Application Support"));
    #[cfg(any(windows, target_os = "macos"))]
    assert_eq!(config, data);
    #[cfg(unix)]
    assert_eq!(directories::data_local_dir(), Some(data));
}

#[cfg(all(unix, not(feature = "simulator")))]
#[test]
fn native_environment_isolated_in_child() {
    use std::os::unix::ffi::OsStringExt;
    const MARKER: &str = "SWITCHY_DIRECTORY_TEST_CHILD";
    if std::env::var_os(MARKER).is_some() {
        let home =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/virtual/home-\xff".to_vec()));
        assert_eq!(directories::home_dir(), Some(home.clone()));
        #[cfg(target_os = "macos")]
        assert_eq!(
            directories::config_dir(),
            Some(home.join("Library/Application Support"))
        );
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(directories::config_dir(), Some(home.join(".config")));
            assert_eq!(directories::data_dir(), Some("/explicit/data".into()));
        }
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native_environment_isolated_in_child"])
        .env(MARKER, "1")
        .env(
            "HOME",
            std::ffi::OsString::from_vec(b"/virtual/home-\xff".to_vec()),
        )
        .env("XDG_CONFIG_HOME", "relative")
        .env("XDG_DATA_HOME", "/explicit/data")
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(feature = "simulator")]
mod simulation {
    use super::directories;
    use std::{
        future::Future,
        pin::Pin,
        sync::Arc,
        task::{Context, Poll, Waker},
    };
    use switchy_fs::{
        directories::DirectoryLocations,
        simulator::{Filesystem, scope_filesystem, with_filesystem},
    };

    fn filesystem(home: &str) -> Arc<Filesystem> {
        Arc::new(Filesystem::with_directory_locations(DirectoryLocations {
            home: Some(home.into()),
            config: Some(format!("{home}/config").into()),
            data: Some(format!("{home}/data").into()),
            data_local: Some(format!("{home}/local").into()),
        }))
    }

    #[test]
    fn explicit_locations_and_nested_unwind_restore() {
        with_filesystem(&Arc::new(Filesystem::new()), || {
            assert_eq!(directories::home_dir(), None);
            assert_eq!(directories::config_dir(), None);
            assert_eq!(directories::data_dir(), None);
            assert_eq!(directories::data_local_dir(), None);
            with_filesystem(&filesystem("/outer"), || {
                assert_eq!(directories::config_dir(), Some("/outer/config".into()));
                assert_eq!(directories::data_dir(), Some("/outer/data".into()));
                assert_eq!(directories::data_local_dir(), Some("/outer/local".into()));
                let result = std::panic::catch_unwind(|| {
                    with_filesystem(&filesystem("/inner"), || {
                        assert_eq!(directories::home_dir(), Some("/inner".into()));
                        panic!("unwind scope");
                    })
                });
                assert!(result.is_err());
                switchy_fs::simulator::reset_fs();
                assert_eq!(directories::home_dir(), Some("/outer".into()));
                assert!(!switchy_fs::exists("/outer"));
                std::thread::spawn(|| assert_eq!(directories::home_dir(), None))
                    .join()
                    .unwrap();
            });
            assert_eq!(directories::home_dir(), None);
        });
    }

    struct CheckScope(&'static str);
    impl Future for CheckScope {
        type Output = ();
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
            assert_eq!(directories::home_dir(), Some(self.0.into()));
            Poll::Pending
        }
    }
    impl Drop for CheckScope {
        fn drop(&mut self) {
            assert_eq!(directories::home_dir(), Some(self.0.into()));
        }
    }

    #[test]
    fn interleaved_polls_and_cancellation_follow_filesystem() {
        with_filesystem(&Arc::new(Filesystem::new()), || {
            let mut context = Context::from_waker(Waker::noop());
            let mut a = scope_filesystem(filesystem("/a"), CheckScope("/a"));
            let mut b = scope_filesystem(filesystem("/b"), CheckScope("/b"));
            for _ in 0..2 {
                assert!(Pin::new(&mut a).poll(&mut context).is_pending());
                assert_eq!(directories::home_dir(), None);
                assert!(Pin::new(&mut b).poll(&mut context).is_pending());
                assert_eq!(directories::home_dir(), None);
            }
            drop(a);
            drop(b);
            assert_eq!(directories::home_dir(), None);
        });
    }

    #[cfg(feature = "simulator-real-fs")]
    #[test]
    fn real_io_scope_does_not_expose_host_directories() {
        with_filesystem(&filesystem("/simulated"), || {
            switchy_fs::simulator::with_real_fs(|| {
                assert_eq!(directories::home_dir(), Some("/simulated".into()));
            });
        });
    }
}
