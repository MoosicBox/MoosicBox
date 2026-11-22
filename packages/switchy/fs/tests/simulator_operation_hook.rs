//! The per-run operation hook runs before every simulated operation, outside any lock.
#![cfg(feature = "simulator")]

use std::{
    io::{Read as _, Write as _},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use switchy_fs::{
    simulator::{Filesystem, OpClass, OperationHook, set_operation_hook, with_filesystem},
    sync::{Dir, OpenOptions, metadata, read_dir_sorted, remove_file, rename, write},
};
use switchy_simulation::{Context, with_context};

/// Run `work` in a fresh simulation run with its own filesystem.
fn simulate<T>(work: impl FnOnce(&Arc<Filesystem>) -> T) -> T {
    with_context(Context::new(1), || {
        let filesystem = Arc::new(Filesystem::new());
        with_filesystem(&filesystem, || work(&filesystem))
    })
}

/// Install a hook recording every class it sees.
fn record() -> Arc<Mutex<Vec<OpClass>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    set_operation_hook(Some(Arc::new(move |class| {
        sink.lock().unwrap().push(class);
    })));
    seen
}

fn take(seen: &Mutex<Vec<OpClass>>) -> Vec<OpClass> {
    std::mem::take(&mut *seen.lock().unwrap())
}

#[test]
fn hook_sees_every_operation_class() {
    simulate(|_| {
        let seen = record();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open("/file")
            .unwrap();
        assert_eq!(take(&seen), [OpClass::Open]);

        file.write_all(b"data").unwrap();
        assert_eq!(take(&seen), [OpClass::Write]);

        file.sync_all().unwrap();
        assert_eq!(take(&seen), [OpClass::Sync]);

        file.set_len(2).unwrap();
        assert_eq!(take(&seen), [OpClass::SetLen]);

        drop(file);
        assert_eq!(take(&seen), []);

        let mut contents = Vec::new();
        OpenOptions::new()
            .read(true)
            .open("/file")
            .unwrap()
            .read_to_end(&mut contents)
            .unwrap();
        assert_eq!(contents, b"da");
        let classes = take(&seen);
        assert_eq!(classes[0], OpClass::Open);
        assert!(classes[1..].iter().all(|class| *class == OpClass::Read));

        metadata("/file").unwrap();
        assert_eq!(take(&seen), [OpClass::Metadata]);

        rename("/file", "/moved").unwrap();
        assert_eq!(take(&seen), [OpClass::Rename]);

        read_dir_sorted("/").unwrap();
        assert_eq!(take(&seen), [OpClass::DirOp]);

        let dir = Dir::open("/").unwrap();
        dir.metadata_at("moved").unwrap();
        dir.rename_at("moved", &dir, "back").unwrap();
        dir.remove_file_at("back").unwrap();
        assert_eq!(
            take(&seen),
            [
                OpClass::Open,
                OpClass::Metadata,
                OpClass::Rename,
                OpClass::DirOp
            ]
        );

        write("/again", b"x").unwrap();
        remove_file("/again").unwrap();
        assert_eq!(
            take(&seen),
            [OpClass::Open, OpClass::Write, OpClass::DirOp]
        );
    });
}

#[test]
fn hook_runs_outside_the_filesystem_lock() {
    simulate(|filesystem| {
        write("/file", b"data").unwrap();
        let calls = Arc::new(Mutex::new(0_u32));
        let counter = Arc::clone(&calls);
        let other = Arc::clone(filesystem);
        set_operation_hook(Some(Arc::new(move |_| {
            *counter.lock().unwrap() += 1;
            // Another thread can use the namespace while the hook runs (as when a
            // scheduler switches threads here); it would block if the hook held the lock.
            let (sender, receiver) = mpsc::channel();
            let other = Arc::clone(&other);
            std::thread::spawn(move || {
                let len = with_filesystem(&other, || metadata("/file").map(|m| m.len()));
                let _ = sender.send(len);
            });
            let len = receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("another thread must not block on the filesystem lock");
            // Operations the hook performs itself do not re-enter it.
            assert_eq!(metadata("/file").unwrap().len(), len.unwrap());
        })));

        assert_eq!(metadata("/file").unwrap().len(), 4);
        write("/file", b"more").unwrap();
        assert_eq!(*calls.lock().unwrap(), 3);
    });
}

#[test]
fn hook_belongs_to_one_run() {
    let calls = Arc::new(Mutex::new(0_u32));
    let hook: OperationHook = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_| *calls.lock().unwrap() += 1)
    };
    let run = Context::new(1);
    with_context(Arc::clone(&run), || {
        assert!(set_operation_hook(Some(Arc::clone(&hook))).is_none());
    });

    // Another run (and a plain thread) never sees it.
    with_context(Context::new(1), || {
        write("/file", b"x").unwrap();
    });
    std::thread::spawn(|| write("/file", b"x").unwrap())
        .join()
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), 0);

    // A thread joining the run (as a simulated thread does) shares it.
    std::thread::spawn({
        let run = Arc::clone(&run);
        move || with_context(run, || write("/file", b"x").unwrap())
    })
    .join()
    .unwrap();
    assert_eq!(*calls.lock().unwrap(), 2);

    with_context(run, || {
        let previous = set_operation_hook(None).expect("hook was installed");
        assert!(Arc::ptr_eq(&previous, &hook));
        write("/file", b"x").unwrap();
    });
    assert_eq!(*calls.lock().unwrap(), 2);
}
