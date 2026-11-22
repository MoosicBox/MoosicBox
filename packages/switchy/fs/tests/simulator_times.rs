//! Simulated timestamps follow the simulated clock and POSIX update rules.
#![cfg(feature = "simulator")]

use std::{
    io::{Read as _, Write as _},
    sync::Arc,
    time::SystemTime,
};

use switchy_fs::{
    simulator::{Filesystem, with_filesystem},
    sync::{
        Metadata, OpenOptions, create_dir, metadata, read_dir_sorted, read_link, remove_file,
        rename, symlink, symlink_metadata, write,
    },
};
use switchy_simulation::{Context, with_context};
use switchy_time::simulator::set_step;

/// Run `work` in a fresh simulation run with its own filesystem.
fn simulate<T>(seed: u64, work: impl FnOnce() -> T) -> T {
    with_context(Context::new(seed), || {
        with_filesystem(&Arc::new(Filesystem::new()), work)
    })
}

/// The simulated time at `step`, leaving the clock there.
fn at(step: u64) -> SystemTime {
    let _ = set_step(step);
    switchy_time::now()
}

fn times(metadata: &Metadata) -> (SystemTime, SystemTime, SystemTime) {
    (
        metadata.created().unwrap(),
        metadata.modified().unwrap(),
        metadata.accessed().unwrap(),
    )
}

#[test]
fn file_times_follow_creation_writes_and_reads() {
    simulate(1, || {
        let created = at(1);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open("/file")
            .unwrap();
        assert_eq!(
            times(&file.metadata().unwrap()),
            (created, created, created)
        );

        let written = at(2);
        assert!(written > created);
        file.write_all(b"data").unwrap();
        assert_eq!(
            times(&metadata("/file").unwrap()),
            (created, written, created)
        );

        // Inspecting metadata never changes timestamps.
        let _ = at(3);
        assert_eq!(
            times(&metadata("/file").unwrap()),
            (created, written, created)
        );

        let read = at(4);
        let mut reader = OpenOptions::new().read(true).open("/file").unwrap();
        let mut contents = String::new();
        reader.read_to_string(&mut contents).unwrap();
        assert_eq!(times(&metadata("/file").unwrap()), (created, written, read));

        let truncated = at(5);
        file.set_len(1).unwrap();
        assert_eq!(
            times(&metadata("/file").unwrap()),
            (created, truncated, read)
        );
    });
}

#[test]
fn directory_times_follow_entry_changes_and_listing() {
    simulate(2, || {
        let created = at(1);
        create_dir("/dir").unwrap();
        create_dir("/other").unwrap();
        assert_eq!(times(&metadata("/dir").unwrap()), (created, created, created));

        let linked = at(2);
        write("/dir/file", b"x").unwrap();
        assert_eq!(times(&metadata("/dir").unwrap()), (created, linked, created));

        let listed = at(3);
        assert_eq!(read_dir_sorted("/dir").unwrap().len(), 1);
        assert_eq!(times(&metadata("/dir").unwrap()), (created, linked, listed));

        let renamed = at(4);
        rename("/dir/file", "/other/file").unwrap();
        assert_eq!(metadata("/dir").unwrap().modified().unwrap(), renamed);
        assert_eq!(metadata("/other").unwrap().modified().unwrap(), renamed);
        // Renaming does not change the file's own data timestamps.
        assert_eq!(
            metadata("/other/file").unwrap().modified().unwrap(),
            linked
        );

        let removed = at(5);
        remove_file("/other/file").unwrap();
        assert_eq!(metadata("/other").unwrap().modified().unwrap(), removed);
    });
}

#[test]
fn reading_a_symbolic_link_updates_its_access_time() {
    simulate(3, || {
        let created = at(1);
        symlink("target", "/link").unwrap();
        let read = at(2);
        read_link("/link").unwrap();
        assert_eq!(
            times(&symlink_metadata("/link").unwrap()),
            (created, created, read)
        );
    });
}

#[test]
fn timestamps_are_reproducible_per_seed() {
    let run = |seed| {
        simulate(seed, || {
            let _ = set_step(3);
            write("/file", b"data").unwrap();
            metadata("/file").unwrap().modified().unwrap()
        })
    };
    assert_eq!(run(9), run(9));
}
