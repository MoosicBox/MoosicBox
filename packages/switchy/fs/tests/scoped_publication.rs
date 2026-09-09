//! No-fault publication uses the same filesystem operations in isolated scopes.
#![cfg(feature = "simulator")]

#[test]
fn scoped_publication_is_repeatable() {
    use std::{
        future::Future as _,
        io::Write as _,
        sync::Arc,
        task::{Context, Poll, Waker},
    };
    use switchy_fs::{
        simulator::{Filesystem, scope_filesystem},
        sync::{File, OpenOptions, read, rename_file},
    };
    fn run() -> Vec<u8> {
        let owner = Arc::new(Filesystem::new());
        let mut future = Box::pin(scope_filesystem(owner, async {
            let lock = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open("owner")
                .unwrap()
                .try_lock_exclusive()
                .unwrap();
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open("pending")
                .unwrap();
            file.write_all(b"snapshot").unwrap();
            file.sync_all().unwrap();
            drop(file);
            rename_file("pending", "state").unwrap();
            drop(lock);
            let _next = File::open("owner").unwrap().try_lock_exclusive().unwrap();
            read("state").unwrap()
        }));
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(bytes) => bytes,
            Poll::Pending => panic!("synchronous filesystem operations must complete"),
        }
    }
    assert_eq!(run(), b"snapshot");
    assert_eq!(run(), run());
}
