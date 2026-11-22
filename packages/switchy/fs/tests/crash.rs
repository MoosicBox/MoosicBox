//! Durability, crash and fault-injection semantics of the simulator.
#![cfg(all(feature = "sync", feature = "simulator"))]

use std::{
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    sync::Arc,
};

use switchy_fs::{
    simulator::{
        CrashPolicy, DirectoryOpLoss, Fault, FaultPlan, FileDataLoss, Filesystem, OpClass,
        Probability, sync, with_filesystem,
    },
    sync::OpenOptions,
};

fn create_new() -> OpenOptions {
    OpenOptions::new().write(true).create_new(true)
}

/// Run `work` in a fresh namespace with a fixed simulation seed.
fn scenario(seed: u64, work: impl FnOnce(&Arc<Filesystem>)) {
    switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
        let filesystem = Arc::new(Filesystem::new());
        work(&filesystem);
    });
}

fn in_fs<T>(filesystem: &Arc<Filesystem>, work: impl FnOnce() -> T) -> T {
    with_filesystem(filesystem, work)
}

#[test]
fn unsynchronized_data_is_lost() {
    scenario(1, |fs| {
        in_fs(fs, || {
            sync::create_dir("/d").unwrap();
            sync::sync_directory("/").unwrap();
            let mut file = sync::File::open_with("/d/f", &create_new()).unwrap();
            file.write_all(b"durable").unwrap();
            file.sync_all().unwrap();
            sync::sync_directory("/d").unwrap();
            file.write_all(b" pending").unwrap();
            assert_eq!(sync::read("/d/f").unwrap(), b"durable pending");
        });
        fs.crash(&CrashPolicy::drop_all_pending());
        in_fs(fs, || assert_eq!(sync::read("/d/f").unwrap(), b"durable"));
    });
}

#[test]
fn file_sync_without_directory_sync_loses_the_entry() {
    scenario(2, |fs| {
        in_fs(fs, || {
            let mut file = sync::File::open_with("/f", &create_new()).unwrap();
            file.write_all(b"data").unwrap();
            file.sync_all().unwrap();
        });
        fs.crash(&CrashPolicy::drop_all_pending());
        in_fs(fs, || {
            assert_eq!(
                sync::metadata("/f").unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            );
        });
    });
}

#[test]
fn directory_sync_without_file_sync_keeps_an_empty_file() {
    scenario(3, |fs| {
        in_fs(fs, || {
            let mut file = sync::File::open_with("/f", &create_new()).unwrap();
            file.write_all(b"data").unwrap();
            sync::sync_directory("/").unwrap();
        });
        fs.crash(&CrashPolicy::drop_all_pending());
        in_fs(fs, || assert_eq!(sync::read("/f").unwrap(), b""));
    });
}

#[test]
fn unlink_and_rename_need_directory_sync() {
    scenario(4, |fs| {
        in_fs(fs, || {
            sync::write("/a", b"a").unwrap();
            sync::File::open("/a").unwrap().sync_all().unwrap();
            sync::sync_directory("/").unwrap();
            sync::rename("/a", "/b").unwrap();
            sync::write("/c", b"c").unwrap();
            sync::remove_file("/c").unwrap();
        });
        fs.crash(&CrashPolicy::drop_all_pending());
        in_fs(fs, || {
            assert_eq!(sync::read("/a").unwrap(), b"a");
            assert!(!switchy_fs::simulator::exists("/b"));
            assert!(!switchy_fs::simulator::exists("/c"));
        });
    });
}

#[test]
fn unsynchronized_unlink_is_undone() {
    scenario(5, |fs| {
        in_fs(fs, || {
            sync::write("/keep", b"kept").unwrap();
            sync::File::open("/keep").unwrap().sync_all().unwrap();
            sync::sync_directory("/").unwrap();
            sync::remove_file("/keep").unwrap();
            assert!(!switchy_fs::simulator::exists("/keep"));
        });
        fs.crash(&CrashPolicy::drop_all_pending());
        in_fs(fs, || assert_eq!(sync::read("/keep").unwrap(), b"kept"));
    });
}

#[test]
fn publish_with_noclobber_is_atomic_and_durable() {
    scenario(6, |fs| {
        in_fs(fs, || {
            sync::create_dir("/state").unwrap();
            sync::sync_directory("/").unwrap();
            switchy_fs::sync::publish_file_noclobber("/state/.tmp", "/state/snapshot", b"v1")
                .unwrap();
            let error =
                switchy_fs::sync::publish_file_noclobber("/state/.tmp2", "/state/snapshot", b"v2")
                    .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        });
        fs.crash(&CrashPolicy::power_loss());
        in_fs(fs, || {
            assert_eq!(sync::read("/state/snapshot").unwrap(), b"v1");
            assert!(!switchy_fs::simulator::exists("/state/.tmp"));
        });
    });
}

#[test]
fn rename_is_atomic_across_crashes() {
    for seed in 0..64 {
        scenario(seed, |fs| {
            in_fs(fs, || {
                sync::create_dir("/a").unwrap();
                sync::create_dir("/b").unwrap();
                sync::write("/a/f", b"payload").unwrap();
                sync::File::open("/a/f").unwrap().sync_all().unwrap();
                sync::sync_directory("/a").unwrap();
                sync::sync_directory("/").unwrap();
                sync::rename("/a/f", "/b/f").unwrap();
            });
            fs.crash(&CrashPolicy::power_loss());
            in_fs(fs, || {
                let in_a = switchy_fs::simulator::exists("/a/f");
                let in_b = switchy_fs::simulator::exists("/b/f");
                assert!(in_a ^ in_b, "seed {seed}: exactly one name survives");
                let path = if in_a { "/a/f" } else { "/b/f" };
                assert_eq!(sync::read(path).unwrap(), b"payload");
            });
        });
    }
}

#[test]
fn process_crash_keeps_everything_but_handles_and_locks() {
    scenario(7, |fs| {
        let (handle, lock) = in_fs(fs, || {
            let mut handle = sync::File::open_with("/f", &create_new()).unwrap();
            handle.write_all(b"unsynced").unwrap();
            let lock = sync::File::open("/f").unwrap();
            lock.try_lock().unwrap();
            (handle, lock)
        });
        fs.crash(&CrashPolicy::process_crash());
        in_fs(fs, || {
            assert_eq!(sync::read("/f").unwrap(), b"unsynced");
            let contender = sync::File::open("/f").unwrap();
            contender.try_lock().unwrap();
        });
        let mut handle = handle;
        let error = handle.write_all(b"more").unwrap_err();
        #[cfg(unix)]
        assert_eq!(
            error.raw_os_error(),
            Some(rustix::io::Errno::STALE.raw_os_error())
        );
        let _ = error;
        assert!(lock.unlock().is_err());
        assert!(handle.metadata().is_err());
    });
}

#[test]
fn torn_writes_keep_whole_sectors() {
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..64 {
        scenario(seed, |fs| {
            in_fs(fs, || {
                let mut file = sync::File::open_with("/f", &create_new()).unwrap();
                file.write_all(&[0u8; 16]).unwrap();
                file.sync_all().unwrap();
                sync::sync_directory("/").unwrap();
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(&[1u8; 16]).unwrap();
                file.write_all(&[2u8; 4]).unwrap();
            });
            fs.crash(&CrashPolicy {
                file_data: FileDataLoss::TornPrefix { sector_size: 4 },
                directory_ops: DirectoryOpLoss::DropPending,
            });
            in_fs(fs, || {
                let data = sync::read("/f").unwrap();
                assert!(
                    data.len() == 16 || data.len() == 20,
                    "seed {seed}: {data:?}"
                );
                for sector in data[..16].chunks(4) {
                    assert!(
                        sector == [0; 4] || sector == [1; 4],
                        "seed {seed}: torn within a sector: {data:?}"
                    );
                }
                if data.len() == 20 {
                    assert_eq!(&data[..16], &[1u8; 16], "seed {seed}: later write kept");
                    assert_eq!(&data[16..], &[2u8; 4]);
                }
                outcomes.insert(data);
            });
        });
    }
    assert!(outcomes.len() > 3, "seeds explore several outcomes");
}

#[test]
fn crash_outcomes_are_deterministic_per_seed() {
    fn run(seed: u64) -> Vec<(String, Vec<u8>)> {
        let mut result = Vec::new();
        scenario(seed, |fs| {
            in_fs(fs, || {
                sync::create_dir("/d").unwrap();
                for index in 0..8 {
                    let path = format!("/d/{index}");
                    let mut file = sync::File::open_with(&path, &create_new()).unwrap();
                    file.write_all(&[index; 1024]).unwrap();
                    if index % 2 == 0 {
                        file.sync_data().unwrap();
                    }
                }
                sync::sync_directory("/").unwrap();
            });
            fs.crash(&CrashPolicy::power_loss());
            in_fs(fs, || {
                if let Ok(entries) = sync::read_dir_sorted("/d") {
                    for entry in entries {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        result.push((name, sync::read(entry.path()).unwrap()));
                    }
                }
            });
        });
        result
    }
    assert_eq!(run(42), run(42));
    let distinct: std::collections::BTreeSet<_> = (0..16).map(run).collect();
    assert!(
        distinct.len() > 1,
        "different seeds explore different outcomes"
    );
}

#[test]
fn unlinked_open_files_vanish_after_crash() {
    scenario(8, |fs| {
        let mut file = in_fs(fs, || {
            let mut file = sync::File::open_with(
                "/f",
                &OpenOptions::new().read(true).write(true).create_new(true),
            )
            .unwrap();
            file.write_all(b"data").unwrap();
            file.sync_all().unwrap();
            sync::sync_directory("/").unwrap();
            sync::remove_file("/f").unwrap();
            sync::sync_directory("/").unwrap();
            file
        });
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"data");
        fs.crash(&CrashPolicy::process_crash());
        in_fs(fs, || {
            assert!(sync::read_dir_sorted("/").unwrap().is_empty())
        });
        assert_eq!(fs.used_bytes(), 0);
    });
}

#[test]
fn injected_faults_fail_once() {
    scenario(9, |fs| {
        in_fs(fs, || {
            fs.inject_fault(OpClass::Open, Fault::Io);
            let error = sync::File::open_with("/f", &create_new()).unwrap_err();
            #[cfg(unix)]
            assert_eq!(
                error.raw_os_error(),
                Some(rustix::io::Errno::IO.raw_os_error())
            );
            let _ = error;
            assert!(!switchy_fs::simulator::exists("/f"));
            let mut file = sync::File::open_with("/f", &create_new()).unwrap();
            fs.inject_fault(OpClass::Write, Fault::NoSpace);
            assert_eq!(
                file.write(b"x").unwrap_err().kind(),
                std::io::ErrorKind::StorageFull
            );
            fs.inject_fault(OpClass::Write, Fault::ShortWrite);
            let written = file.write(b"0123456789").unwrap();
            assert!((1..10).contains(&written));
            fs.inject_fault(OpClass::Rename, Fault::Io);
            assert!(sync::rename("/f", "/g").is_err());
            sync::rename("/f", "/g").unwrap();
        });
    });
}

#[test]
fn failed_fsync_forgets_pending_data() {
    scenario(10, |fs| {
        in_fs(fs, || {
            let mut file = sync::File::open_with("/f", &create_new()).unwrap();
            file.write_all(b"first").unwrap();
            file.sync_all().unwrap();
            sync::sync_directory("/").unwrap();
            file.write_all(b" lost").unwrap();
            fs.inject_fault(OpClass::Sync, Fault::Io);
            assert!(file.sync_all().is_err());
            // The failed writeback stays visible until a crash ...
            assert_eq!(sync::read("/f").unwrap(), b"first lost");
            // ... and a later successful fsync does not make it durable.
            file.write_all(b" later").unwrap();
            file.sync_all().unwrap();
        });
        fs.crash(&CrashPolicy::drop_all_pending());
        in_fs(fs, || {
            assert_eq!(sync::read("/f").unwrap(), b"first\0\0\0\0\0 later");
        });
    });
}

#[test]
fn quota_shortens_then_rejects_writes() {
    scenario(11, |fs| {
        fs.set_quota(Some(10));
        in_fs(fs, || {
            let mut file = sync::File::open_with("/f", &create_new()).unwrap();
            assert_eq!(file.write(b"0123456789abc").unwrap(), 10);
            assert_eq!(
                file.write(b"x").unwrap_err().kind(),
                std::io::ErrorKind::StorageFull
            );
            assert_eq!(
                file.set_len(11).unwrap_err().kind(),
                std::io::ErrorKind::StorageFull
            );
            file.set_len(4).unwrap();
            assert_eq!(fs.used_bytes(), 4);
            drop(file);
            sync::remove_file("/f").unwrap();
            assert_eq!(fs.used_bytes(), 0);
        });
    });
}

#[test]
fn probabilistic_fault_plans_are_seeded() {
    fn failures(seed: u64) -> Vec<bool> {
        let mut result = Vec::new();
        scenario(seed, |fs| {
            fs.set_fault_plan(FaultPlan::new().with(
                OpClass::Metadata,
                Fault::Io,
                Probability::percent(50),
            ));
            in_fs(fs, || {
                for _ in 0..32 {
                    result.push(sync::metadata("/").is_err());
                }
            });
        });
        result
    }
    let first = failures(77);
    assert_eq!(first, failures(77));
    assert!(first.iter().any(|failed| *failed));
    assert!(first.iter().any(|failed| !*failed));
}

#[test]
fn locks_are_scoped_per_namespace() {
    let first = Arc::new(Filesystem::new());
    let second = Arc::new(Filesystem::new());
    let a = in_fs(&first, || {
        sync::write("/lock", b"").unwrap();
        let file = sync::File::open("/lock").unwrap();
        file.try_lock().unwrap();
        file
    });
    let b = in_fs(&second, || {
        sync::write("/lock", b"").unwrap();
        sync::File::open("/lock").unwrap()
    });
    b.try_lock().unwrap();
    drop(a);
    drop(b);
}

#[test]
fn simulated_credentials_drive_permission_checks() {
    let fs = Arc::new(Filesystem::with_credentials(1000, 1000));
    in_fs(&fs, || {
        assert_eq!(sync::current_uid().unwrap(), 1000);
        sync::write("/mine", b"").unwrap();
        sync::set_permissions("/mine", switchy_fs::Permissions::from_mode(0o600)).unwrap();
        fs.set_credentials(2000, 2000);
        assert_eq!(
            sync::File::open("/mine").unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            sync::set_permissions("/mine", switchy_fs::Permissions::from_mode(0o777))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        fs.set_credentials(0, 0);
        assert!(sync::File::open("/mine").is_ok());
        assert_eq!(sync::metadata("/mine").unwrap().uid(), Some(1000));
    });
}

#[test]
fn reset_invalidates_handles() {
    let fs = Arc::new(Filesystem::new());
    let mut file = in_fs(&fs, || sync::File::open_with("/f", &create_new()).unwrap());
    fs.reset();
    assert!(file.write_all(b"x").is_err());
    in_fs(&fs, || assert!(!switchy_fs::simulator::exists("/f")));
}
