//! The Windows personality: open files cannot be removed or replaced, names are
//! case-insensitive but case-preserving, and symbolic links need a missing privilege.
#![cfg(feature = "simulator")]

use std::{io::Write as _, sync::Arc};

use switchy_fs::{
    simulator::{Filesystem, Personality, set_run_personality, with_filesystem},
    sync::{
        Dir, File, OpenOptions, create_dir, create_dir_all, hard_link, metadata,
        read_dir_sorted, read_to_string, remove_dir_all, remove_file, rename, symlink, write,
    },
};
use switchy_simulation::{Context, with_context};

fn in_namespace<T>(personality: Personality, work: impl FnOnce() -> T) -> T {
    let filesystem = Arc::new(Filesystem::new());
    filesystem.set_personality(personality);
    assert_eq!(filesystem.personality(), personality);
    with_filesystem(&filesystem, work)
}

/// The error of a Windows-only failure: the native code on Windows hosts, else
/// `PermissionDenied`.
#[track_caller]
fn assert_windows_error<T: std::fmt::Debug>(result: std::io::Result<T>, code: i32) {
    let error = result.expect_err("operation must fail");
    if cfg!(windows) {
        assert_eq!(error.raw_os_error(), Some(code), "got {error:?}");
    } else {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::PermissionDenied,
            "got {error:?}"
        );
    }
}

const SHARING_VIOLATION: i32 = 32;
const PRIVILEGE_NOT_HELD: i32 = 1314;

fn names(path: &str) -> Vec<String> {
    read_dir_sorted(path)
        .unwrap()
        .iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn posix_is_the_default() {
    assert_eq!(Filesystem::new().personality(), Personality::Posix);
    in_namespace(Personality::Posix, || {
        let open = File::create("/file").unwrap();
        remove_file("/file").unwrap();
        drop(open);

        write("/a", b"a").unwrap();
        write("/A", b"A").unwrap();
        assert_eq!(names("/"), ["A", "a"]);
        assert!(metadata("/missing").is_err());
        symlink("a", "/link").unwrap();
    });
}

#[test]
fn open_files_cannot_be_removed() {
    in_namespace(Personality::Windows, || {
        let mut open = File::create("/file").unwrap();
        assert_windows_error(remove_file("/file"), SHARING_VIOLATION);
        let dir = Dir::open("/").unwrap();
        assert_windows_error(dir.remove_file_at("file"), SHARING_VIOLATION);
        open.write_all(b"still here").unwrap();
        drop(open);
        remove_file("/file").unwrap();

        create_dir_all("/tree/sub").unwrap();
        let open = File::create("/tree/sub/file").unwrap();
        assert_windows_error(remove_dir_all("/tree"), SHARING_VIOLATION);
        drop(open);
        remove_dir_all("/tree").unwrap();
    });
}

#[test]
fn open_files_cannot_be_replaced_by_rename() {
    in_namespace(Personality::Windows, || {
        write("/source", b"new").unwrap();
        let target = File::open({
            write("/target", b"old").unwrap();
            "/target"
        })
        .unwrap();
        assert_windows_error(rename("/source", "/target"), SHARING_VIOLATION);
        assert_eq!(read_to_string("/target").unwrap(), "old");
        drop(target);

        // Renaming an open file itself is allowed.
        let source = File::open("/source").unwrap();
        rename("/source", "/target").unwrap();
        drop(source);
        assert_eq!(read_to_string("/target").unwrap(), "new");
    });
}

#[test]
fn names_are_case_insensitive_and_case_preserving() {
    in_namespace(Personality::Windows, || {
        create_dir("/Dir").unwrap();
        write("/Dir/Name.TXT", b"data").unwrap();
        assert_eq!(read_to_string("/dir/name.txt").unwrap(), "data");
        assert_eq!(read_to_string("/DIR/NAME.txt").unwrap(), "data");
        assert_eq!(names("/dIR"), ["Name.TXT"]);

        // Another spelling of an existing name is the existing entry.
        write("/DIR/name.txt", b"replaced").unwrap();
        assert_eq!(names("/Dir"), ["Name.TXT"]);
        assert_eq!(read_to_string("/Dir/Name.TXT").unwrap(), "replaced");
        let error = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open("/dir/NAME.TXT")
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            create_dir("/DIR").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );

        // Renaming to another spelling changes the case.
        rename("/dir/name.txt", "/dir/NAME.txt").unwrap();
        assert_eq!(names("/Dir"), ["NAME.txt"]);
        let dir = Dir::open("/DIR").unwrap();
        dir.rename_at("name.TXT", &dir, "name.txt").unwrap();
        assert_eq!(names("/Dir"), ["name.txt"]);

        // Renaming over another spelling of an existing entry replaces it, and the
        // replacing entry takes the requested spelling.
        write("/Dir/other", b"other").unwrap();
        rename("/dir/OTHER", "/dir/NAME.TXT").unwrap();
        assert_eq!(names("/Dir"), ["NAME.TXT"]);
        assert_eq!(read_to_string("/dir/name.txt").unwrap(), "other");

        remove_file("/DIR/NAME.TXT").unwrap();
        assert!(names("/dir").is_empty());
    });
}

#[test]
fn symbolic_links_need_a_privilege() {
    in_namespace(Personality::Windows, || {
        write("/target", b"data").unwrap();
        assert_windows_error(symlink("/target", "/link"), PRIVILEGE_NOT_HELD);
        let dir = Dir::open("/").unwrap();
        assert_windows_error(dir.symlink_at("target", "link"), PRIVILEGE_NOT_HELD);
        assert!(metadata("/link").is_err());
        // Hard links need no privilege.
        hard_link("/target", "/hard").unwrap();
        assert_eq!(read_to_string("/hard").unwrap(), "data");
    });
}

#[test]
fn run_personality_applies_to_the_runs_default_namespace() {
    with_context(Context::new(1), || {
        set_run_personality(Personality::Windows);
        write("/Name", b"data").unwrap();
        assert_eq!(read_to_string("/name").unwrap(), "data");
        // An explicit namespace keeps its own personality.
        in_namespace(Personality::Posix, || {
            write("/Name", b"data").unwrap();
            assert!(read_to_string("/name").is_err());
        });
    });
    // Another run's default namespace is POSIX.
    with_context(Context::new(1), || {
        write("/Name", b"data").unwrap();
        assert!(read_to_string("/name").is_err());
    });
}

#[test]
fn personality_survives_reset_and_crash() {
    let filesystem = Arc::new(Filesystem::new());
    filesystem.set_personality(Personality::Windows);
    filesystem.reset();
    filesystem.crash(&switchy_fs::simulator::CrashPolicy::process_crash());
    assert_eq!(filesystem.personality(), Personality::Windows);
}

#[test]
fn case_changing_renames_are_durable_and_crash_atomic() {
    use switchy_fs::simulator::CrashPolicy;
    let filesystem = Arc::new(Filesystem::new());
    filesystem.set_personality(Personality::Windows);
    let synced = |work: &dyn Fn()| {
        with_filesystem(&filesystem, || {
            work();
            switchy_fs::sync::sync_directory("/").unwrap();
        });
    };
    synced(&|| {
        for (path, data) in [("/target", b"old"), ("/source", b"new")] {
            let mut file = File::create(path).unwrap();
            file.write_all(data).unwrap();
            file.sync_all().unwrap();
        }
    });
    synced(&|| rename("/source", "/TARGET").unwrap());
    synced(&|| rename("/target", "/Target").unwrap());
    filesystem.crash(&CrashPolicy::drop_all_pending());
    with_filesystem(&filesystem, || {
        assert_eq!(names("/"), ["Target"]);
        assert_eq!(read_to_string("/target").unwrap(), "new");
    });

    // An unsynchronized case change is lost or kept as a whole.
    with_filesystem(&filesystem, || rename("/target", "/tARGET").unwrap());
    filesystem.crash(&CrashPolicy::drop_all_pending());
    with_filesystem(&filesystem, || assert_eq!(names("/"), ["Target"]));
}
