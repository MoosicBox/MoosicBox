//! Explicit filesystem ownership and scope restoration.
#![cfg(feature = "simulator")]
#[test]
fn pending_polls_and_cancellation_restore_scope() {
    use std::{
        future::Future as _,
        sync::Arc,
        task::{Context, Poll, Waker},
    };
    use switchy_fs::simulator::{Filesystem, scope_filesystem, with_filesystem};
    use switchy_fs::sync::{read, write};
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            write("cleanup", b"done").unwrap();
        }
    }
    let owner = Arc::new(Filesystem::new());
    let outside = Arc::new(Filesystem::new());
    let cleanup = Cleanup;
    let mut polls = 0;
    let work = std::future::poll_fn(move |_| {
        let _retained = &cleanup;
        polls += 1;
        write("polls", [polls]).unwrap();
        Poll::<()>::Pending
    });
    let mut scoped = Box::pin(scope_filesystem(owner.clone(), work));
    with_filesystem(&outside, || {
        let mut context = Context::from_waker(Waker::noop());
        assert!(scoped.as_mut().poll(&mut context).is_pending());
        assert!(scoped.as_mut().poll(&mut context).is_pending());
        assert!(!switchy_fs::exists("polls"));
        drop(scoped);
        assert!(!switchy_fs::exists("cleanup"));
    });
    with_filesystem(&owner, || {
        assert_eq!(read("polls").unwrap(), [2]);
        assert_eq!(read("cleanup").unwrap(), b"done");
    });
}

#[test]
fn namespaces_are_isolated_and_shareable() {
    use std::sync::Arc;
    use switchy_fs::simulator::{Filesystem, with_filesystem};
    use switchy_fs::sync::{read, write};
    let first = Arc::new(Filesystem::new());
    let second = Arc::new(Filesystem::new());
    with_filesystem(&first, || {
        write("data", b"first").unwrap();
        let result = std::panic::catch_unwind(|| {
            with_filesystem(&second, || {
                assert!(!switchy_fs::exists("data"));
                write("data", b"second").unwrap();
                panic!("restore scope");
            })
        });
        assert!(result.is_err());
        assert_eq!(read("data").unwrap(), b"first");
    });
    std::thread::spawn(move || {
        with_filesystem(&first, || {
            assert_eq!(read("data").unwrap(), b"first");
        })
    })
    .join()
    .unwrap();
    with_filesystem(&second, || assert_eq!(read("data").unwrap(), b"second"));
}
