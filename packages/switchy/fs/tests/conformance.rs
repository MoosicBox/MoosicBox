//! Conformance suite: every case runs against the real `std` backend in a temporary
//! directory and against the simulator, asserting identical results and `errno` values.
#![cfg(all(unix, feature = "sync", feature = "std", feature = "simulator"))]

use std::{
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::Path,
    sync::Arc,
};

use rustix::io::Errno;
use switchy_fs::{FileKind, Permissions, sync::OpenOptions};

#[track_caller]
fn assert_errno<T: std::fmt::Debug>(result: std::io::Result<T>, expected: Errno) {
    let error = result.expect_err("operation must fail");
    assert_eq!(
        error.raw_os_error(),
        Some(expected.raw_os_error()),
        "expected {expected:?}, got {error:?}"
    );
}

#[track_caller]
fn assert_kind<T: std::fmt::Debug>(result: std::io::Result<T>, expected: std::io::ErrorKind) {
    let error = result.expect_err("operation must fail");
    assert_eq!(error.kind(), expected, "got {error:?}");
}

/// Errno of `unlink` on a directory.
fn unlink_dir_errno() -> Errno {
    if cfg!(any(target_os = "linux", target_os = "android")) {
        Errno::ISDIR
    } else {
        Errno::PERM
    }
}

fn is_root() -> bool {
    rustix::process::geteuid().is_root()
}

fn write_opts() -> OpenOptions {
    OpenOptions::new().write(true).create(true).truncate(true)
}

fn run_std(body: impl FnOnce(&Path)) {
    let temp = switchy_fs::standard::temp_dir::TempDir::new().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    switchy_fs::standard::sync::set_permissions(&root, Permissions::from_mode(0o700)).unwrap();
    body(&root);
    // Restore permissions so cleanup can descend everywhere.
    restore_permissions(&root);
}

fn restore_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                restore_permissions(&entry.path());
            }
        }
    }
}

fn run_sim(body: impl FnOnce(&Path)) {
    use switchy_fs::simulator::{Filesystem, with_filesystem};
    let uid = rustix::process::geteuid().as_raw();
    let gid = rustix::process::getegid().as_raw();
    let filesystem = Arc::new(Filesystem::with_credentials(uid, gid));
    with_filesystem(&filesystem, || {
        switchy_fs::simulator::sync::create_dir_all("/conformance/root").unwrap();
        switchy_fs::simulator::sync::set_permissions(
            "/conformance/root",
            Permissions::from_mode(0o700),
        )
        .unwrap();
        body(Path::new("/conformance/root"));
    });
}

macro_rules! conformance {
    ($(fn $name:ident($root:ident) $body:block)*) => {
        mod std_backend {
            #[allow(unused_imports)]
            use super::*;
            use switchy_fs::standard::sync as fs;

            $(
                #[test]
                fn $name() {
                    run_std(|$root: &Path| $body);
                }
            )*
        }

        mod sim_backend {
            #[allow(unused_imports)]
            use super::*;
            use switchy_fs::simulator::sync as fs;

            $(
                #[test]
                fn $name() {
                    run_sim(|$root: &Path| $body);
                }
            )*
        }
    };
}

conformance! {
    fn open_flag_validation(root) {
        let path = root.join("file");
        assert_kind(
            fs::File::open_with(&path, &OpenOptions::new()),
            std::io::ErrorKind::InvalidInput,
        );
        for options in [
            OpenOptions::new().read(true).create(true),
            OpenOptions::new().read(true).truncate(true),
            OpenOptions::new().append(true).truncate(true),
            OpenOptions::new().create_new(true),
        ] {
            assert_kind(fs::File::open_with(&path, &options), std::io::ErrorKind::InvalidInput);
        }
        let dir = fs::Dir::open(root).unwrap();
        assert_kind(
            dir.open_file_at("file", &OpenOptions::new().read(true).create(true)),
            std::io::ErrorKind::InvalidInput,
        );
        assert_errno(fs::metadata(&path), Errno::NOENT);
    }

    fn open_errors(root) {
        let file = root.join("file");
        fs::write(&file, b"data").unwrap();
        assert_errno(fs::File::open(root.join("missing")), Errno::NOENT);
        assert_errno(fs::File::open(file.join("child")), Errno::NOTDIR);
        assert_errno(fs::File::open(root.join("file/")), Errno::NOTDIR);
        assert_errno(
            fs::File::open_with(&file, &OpenOptions::new().write(true).create_new(true)),
            Errno::EXIST,
        );
        assert_errno(
            fs::File::open_with(root, &OpenOptions::new().write(true)),
            Errno::ISDIR,
        );
        assert_errno(
            fs::File::open_with(&file, &OpenOptions::new().read(true).directory(true)),
            Errno::NOTDIR,
        );
        assert_errno(
            fs::File::open_with(
                root.join("missing/child"),
                &OpenOptions::new().write(true).create(true),
            ),
            Errno::NOENT,
        );
    }

    fn directory_handle_reads_fail(root) {
        let mut dir = fs::File::open(root).unwrap();
        let mut buf = [0u8; 4];
        assert_errno(dir.read(&mut buf), Errno::ISDIR);
        assert!(dir.metadata().unwrap().is_dir());
        dir.sync_all().unwrap();
    }

    fn read_write_seek_set_len(root) {
        let path = root.join("data");
        let mut file =
            fs::File::open_with(&path, &OpenOptions::new().read(true).write(true).create(true))
                .unwrap();
        file.write_all(b"hello world").unwrap();
        file.seek(SeekFrom::Start(6)).unwrap();
        file.write_all(b"there").unwrap();
        assert_eq!(file.seek(SeekFrom::End(-5)).unwrap(), 6);
        let mut buf = String::new();
        file.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "there");

        file.set_len(3).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hel");
        file.set_len(6).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hel\0\0\0");
        // Writing past the end leaves a zero-filled gap.
        file.seek(SeekFrom::Start(8)).unwrap();
        file.write_all(b"!").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hel\0\0\0\0\0!");
        file.sync_data().unwrap();
        file.sync_all().unwrap();
        assert_eq!(file.metadata().unwrap().len(), 9);
    }

    fn append_ignores_position(root) {
        let path = root.join("log");
        fs::write(&path, b"one").unwrap();
        let mut file = fs::File::open_with(&path, &OpenOptions::new().append(true)).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"onetwo");
    }

    fn access_mode_enforced_on_handles(root) {
        let path = root.join("file");
        fs::write(&path, b"data").unwrap();
        let mut reader = fs::File::open(&path).unwrap();
        assert_errno(reader.write(b"x"), Errno::BADF);
        let mut writer = fs::File::open_with(&path, &OpenOptions::new().write(true)).unwrap();
        let mut buf = [0u8; 4];
        assert_errno(writer.read(&mut buf), Errno::BADF);
        // `ftruncate` on a read-only descriptor fails with `EINVAL` (Linux and macOS).
        assert_errno(reader.set_len(0), Errno::INVAL);
        assert_eq!(fs::read(&path).unwrap(), b"data");
    }

    fn truncate_on_open(root) {
        let path = root.join("file");
        fs::write(&path, b"longer content").unwrap();
        let mut file = fs::File::open_with(&path, &write_opts()).unwrap();
        file.write_all(b"new").unwrap();
        drop(file);
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    fn metadata_and_identity(root) {
        let path = root.join("file");
        let mut file = fs::File::open_with(
            &path,
            &OpenOptions::new().write(true).create_new(true).mode(0o600),
        )
        .unwrap();
        file.write_all(b"12345").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.len(), 5);
        assert_eq!(metadata.kind(), FileKind::File);
        assert_eq!(metadata.mode(), 0o600);
        assert_eq!(metadata.uid(), Some(fs::current_uid().unwrap()));
        assert_eq!(metadata.nlink(), Some(1));
        let handle_metadata = file.metadata().unwrap();
        assert!(metadata.is_same_file(&handle_metadata));

        fs::hard_link(&path, root.join("link")).unwrap();
        let linked = fs::metadata(root.join("link")).unwrap();
        assert!(linked.is_same_file(&metadata));
        assert_eq!(linked.nlink(), Some(2));
        let other = fs::write(root.join("other"), b"").and_then(|()| fs::metadata(root.join("other")));
        assert!(!other.unwrap().is_same_file(&metadata));

        let dir = fs::metadata(root).unwrap();
        assert_eq!(dir.kind(), FileKind::Dir);
        assert_eq!(dir.mode(), 0o700);
        assert_errno(fs::metadata(root.join("missing")), Errno::NOENT);
        assert_errno(fs::metadata(path.join("x")), Errno::NOTDIR);
    }

    fn permissions_round_trip(root) {
        let path = root.join("file");
        fs::write(&path, b"").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode(), 0o640);
        let file = fs::File::open(&path).unwrap();
        file.set_permissions(Permissions::from_mode(0o604)).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode(), 0o604);
        assert!(!fs::metadata(&path).unwrap().permissions().readonly());
    }

    fn permission_checks(root) {
        if is_root() {
            return;
        }
        let path = root.join("readonly");
        fs::write(&path, b"data").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o400)).unwrap();
        assert_errno(
            fs::File::open_with(&path, &OpenOptions::new().write(true)),
            Errno::ACCESS,
        );
        assert!(fs::File::open(&path).is_ok());
        fs::set_permissions(&path, Permissions::from_mode(0o000)).unwrap();
        assert_errno(fs::File::open(&path), Errno::ACCESS);

        let locked = root.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("inside"), b"").unwrap();
        fs::set_permissions(&locked, Permissions::from_mode(0o500)).unwrap();
        assert_errno(fs::write(locked.join("new"), b""), Errno::ACCESS);
        assert_errno(fs::remove_file(locked.join("inside")), Errno::ACCESS);
        fs::set_permissions(&locked, Permissions::from_mode(0o000)).unwrap();
        assert_errno(fs::metadata(locked.join("inside")), Errno::ACCESS);
        fs::set_permissions(&locked, Permissions::from_mode(0o700)).unwrap();
    }

    fn symlinks(root) {
        let target = root.join("target");
        fs::write(&target, b"pointed").unwrap();
        let link = root.join("link");
        fs::symlink("target", &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), Path::new("target"));
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::symlink_metadata(&link).unwrap().len(), 6);
        assert!(fs::metadata(&link).unwrap().is_file());
        assert_eq!(fs::read(&link).unwrap(), b"pointed");
        assert_errno(fs::read_link(&target), Errno::INVAL);
        assert_errno(
            fs::File::open_with(&link, &OpenOptions::new().read(true).nofollow(true)),
            Errno::LOOP,
        );
        assert_errno(fs::symlink("elsewhere", &link), Errno::EXIST);

        // A symbolic link to a directory is followed by intermediate components.
        let dir = root.join("dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("inner"), b"inner").unwrap();
        fs::symlink(&dir, root.join("dirlink")).unwrap();
        assert_eq!(fs::read(root.join("dirlink/inner")).unwrap(), b"inner");
        assert_eq!(fs::canonicalize(root.join("dirlink/inner")).unwrap(), dir.join("inner"));
        assert_eq!(fs::canonicalize(root.join("dir/../link")).unwrap(), target);
    }

    fn dangling_and_looping_symlinks(root) {
        let dangling = root.join("dangling");
        fs::symlink("missing", &dangling).unwrap();
        assert!(fs::symlink_metadata(&dangling).unwrap().is_symlink());
        assert_errno(fs::metadata(&dangling), Errno::NOENT);
        assert_errno(
            fs::File::open_with(&dangling, &OpenOptions::new().write(true).create_new(true)),
            Errno::EXIST,
        );
        // Creating through a dangling link creates its target.
        fs::File::open_with(&dangling, &OpenOptions::new().write(true).create(true)).unwrap();
        assert!(fs::metadata(root.join("missing")).unwrap().is_file());

        fs::symlink("loop_b", root.join("loop_a")).unwrap();
        fs::symlink("loop_a", root.join("loop_b")).unwrap();
        assert_errno(fs::metadata(root.join("loop_a")), Errno::LOOP);
        assert_errno(fs::File::open(root.join("loop_a")), Errno::LOOP);
        assert!(fs::symlink_metadata(root.join("loop_a")).unwrap().is_symlink());
    }

    fn rename_semantics(root) {
        let a = root.join("a");
        let b = root.join("b");
        fs::write(&a, b"A").unwrap();
        fs::write(&b, b"B").unwrap();
        fs::rename(&a, &b).unwrap();
        assert_eq!(fs::read(&b).unwrap(), b"A");
        assert_errno(fs::metadata(&a), Errno::NOENT);
        assert_errno(fs::rename(&a, &b), Errno::NOENT);

        let dir = root.join("dir");
        let full = root.join("full");
        fs::create_dir(&dir).unwrap();
        fs::create_dir(&full).unwrap();
        fs::write(full.join("x"), b"").unwrap();
        assert_errno(fs::rename(&b, &dir), Errno::ISDIR);
        assert_errno(fs::rename(&dir, &b), Errno::NOTDIR);
        assert_errno(fs::rename(&dir, &full), Errno::NOTEMPTY);
        assert_errno(fs::rename(&full, full.join("sub")), Errno::INVAL);

        let empty = root.join("empty");
        fs::create_dir(&empty).unwrap();
        fs::rename(&full, &empty).unwrap();
        assert!(fs::metadata(empty.join("x")).unwrap().is_file());

        // Renaming onto another link to the same file does nothing.
        fs::hard_link(&b, root.join("b2")).unwrap();
        fs::rename(&b, root.join("b2")).unwrap();
        assert!(fs::metadata(&b).is_ok());
        assert!(fs::metadata(root.join("b2")).is_ok());
    }

    fn rename_noreplace(root) {
        let a = root.join("a");
        let b = root.join("b");
        fs::write(&a, b"A").unwrap();
        fs::write(&b, b"B").unwrap();
        assert_errno(fs::rename_noreplace(&a, &b), Errno::EXIST);
        assert_eq!(fs::read(&b).unwrap(), b"B");
        fs::symlink("nowhere", root.join("dangling")).unwrap();
        assert_errno(fs::rename_noreplace(&a, root.join("dangling")), Errno::EXIST);
        fs::rename_noreplace(&a, root.join("c")).unwrap();
        assert_eq!(fs::read(root.join("c")).unwrap(), b"A");
        assert_errno(fs::metadata(&a), Errno::NOENT);
    }

    fn open_handles_survive_rename_and_unlink(root) {
        let path = root.join("file");
        fs::write(&path, b"content").unwrap();
        let mut file = fs::File::open(&path).unwrap();
        fs::rename(&path, root.join("moved")).unwrap();
        fs::remove_file(root.join("moved")).unwrap();
        let mut buf = String::new();
        file.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "content");
        assert_eq!(file.metadata().unwrap().nlink(), Some(0));
    }

    fn remove_semantics(root) {
        let dir = root.join("dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("file"), b"").unwrap();
        assert_errno(fs::remove_file(&dir), unlink_dir_errno());
        assert_errno(fs::remove_dir(&dir), Errno::NOTEMPTY);
        assert_errno(fs::remove_dir(dir.join("file")), Errno::NOTDIR);
        assert_errno(fs::remove_file(root.join("missing")), Errno::NOENT);
        fs::remove_file(dir.join("file")).unwrap();
        fs::remove_dir(&dir).unwrap();
        assert_errno(fs::metadata(&dir), Errno::NOENT);
    }

    fn remove_dir_all_does_not_follow_links(root) {
        let dir = root.join("dir");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/file"), b"").unwrap();
        let keep = root.join("keep");
        fs::create_dir(&keep).unwrap();
        fs::write(keep.join("kept"), b"").unwrap();
        fs::symlink(&keep, dir.join("link")).unwrap();
        fs::symlink(&keep, root.join("toplink")).unwrap();
        fs::remove_dir_all(root.join("toplink")).unwrap();
        assert!(fs::metadata(keep.join("kept")).is_ok());
        fs::remove_dir_all(&dir).unwrap();
        assert_errno(fs::metadata(&dir), Errno::NOENT);
        assert!(fs::metadata(keep.join("kept")).is_ok());
        assert_errno(fs::remove_dir_all(keep.join("kept")), Errno::NOTDIR);
        assert_kind(fs::remove_dir_all(root.join("missing")), std::io::ErrorKind::NotFound);
    }

    fn create_dir_semantics(root) {
        let dir = root.join("dir");
        fs::create_dir(&dir).unwrap();
        assert_errno(fs::create_dir(&dir), Errno::EXIST);
        assert_errno(fs::create_dir(root.join("missing/child")), Errno::NOENT);
        fs::create_dir_all(dir.join("a/b/c")).unwrap();
        fs::create_dir_all(dir.join("a/b/c")).unwrap();
        let file = root.join("file");
        fs::write(&file, b"").unwrap();
        assert_errno(fs::create_dir_all(&file), Errno::EXIST);
        assert_errno(fs::create_dir_all(file.join("sub")), Errno::NOTDIR);
        assert_errno(fs::create_dir(file.join("sub")), Errno::NOTDIR);
    }

    fn read_dir_lists_sorted_entries(root) {
        fs::write(root.join("b"), b"").unwrap();
        fs::create_dir(root.join("a")).unwrap();
        fs::symlink("b", root.join("c")).unwrap();
        let names: Vec<_> = fs::read_dir_sorted(root)
            .unwrap()
            .iter()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    fn dir_handle_operations(root) {
        let dir = fs::Dir::open(root).unwrap();
        let options = OpenOptions::new().write(true).create_new(true).mode(0o600);
        let mut file = dir.open_file_at("file", &options).unwrap();
        file.write_all(b"data").unwrap();
        file.sync_all().unwrap();
        assert_errno(dir.open_file_at("file", &options), Errno::EXIST);
        assert_eq!(dir.metadata_at("file").unwrap().mode(), 0o600);
        assert_eq!(dir.metadata_at("file").unwrap().len(), 4);
        assert!(dir.metadata().unwrap().is_dir());

        dir.create_dir_at("sub", 0o700).unwrap();
        assert_errno(dir.create_dir_at("sub", 0o700), Errno::EXIST);
        assert_eq!(dir.metadata_at("sub").unwrap().mode(), 0o700);
        dir.symlink_at("file", "link").unwrap();
        assert_eq!(dir.read_link_at("link").unwrap(), Path::new("file"));
        assert!(dir.metadata_at("link").unwrap().is_symlink());
        assert_errno(
            dir.open_file_at("link", &OpenOptions::new().read(true).nofollow(true)),
            Errno::LOOP,
        );
        // `O_DIRECTORY | O_NOFOLLOW` on a symbolic link reports `ENOTDIR`.
        assert_errno(dir.open_dir_at("link"), Errno::NOTDIR);
        assert_errno(dir.open_dir_at("file"), Errno::NOTDIR);
        assert_errno(dir.metadata_at("missing"), Errno::NOENT);

        let entries: Vec<_> = dir
            .entries()
            .unwrap()
            .into_iter()
            .map(|entry| (entry.name().to_string_lossy().into_owned(), entry.kind()))
            .collect();
        assert_eq!(
            entries,
            [
                ("file".to_owned(), FileKind::File),
                ("link".to_owned(), FileKind::Symlink),
                ("sub".to_owned(), FileKind::Dir),
            ]
        );

        let sub = dir.open_dir_at("sub").unwrap();
        dir.rename_at("file", &sub, "moved").unwrap();
        assert_eq!(fs::read(root.join("sub/moved")).unwrap(), b"data");
        sub.hard_link_at("moved", &dir, "again").unwrap();
        assert_eq!(dir.metadata_at("again").unwrap().nlink(), Some(2));
        assert_errno(dir.rename_noreplace_at("again", &sub, "moved"), Errno::EXIST);
        dir.rename_noreplace_at("again", &sub, "fresh").unwrap();
        assert_errno(dir.remove_dir_at("sub"), Errno::NOTEMPTY);
        sub.remove_file_at("moved").unwrap();
        sub.remove_file_at("fresh").unwrap();
        assert_errno(sub.remove_file_at("fresh"), Errno::NOENT);
        dir.remove_dir_at("sub").unwrap();
        dir.remove_file_at("link").unwrap();
        sub.sync().unwrap();
        dir.sync().unwrap();
        assert!(dir.entries().unwrap().is_empty());
    }

    fn dir_handle_rejects_paths(root) {
        let dir = fs::Dir::open(root).unwrap();
        let options = OpenOptions::new().write(true).create(true);
        for name in ["a/b", "..", ".", "", "/abs", "a/"] {
            assert_kind(dir.open_file_at(name, &options), std::io::ErrorKind::InvalidInput);
            assert_kind(dir.metadata_at(name), std::io::ErrorKind::InvalidInput);
        }
    }

    fn dir_open_nofollow(root) {
        fs::create_dir(root.join("real")).unwrap();
        fs::symlink("real", root.join("link")).unwrap();
        fs::write(root.join("file"), b"").unwrap();
        assert_errno(fs::Dir::open(root.join("link")), Errno::NOTDIR);
        assert_errno(fs::Dir::open(root.join("file")), Errno::NOTDIR);
        assert_errno(fs::Dir::open(root.join("missing")), Errno::NOENT);
        // Intermediate components still follow links.
        fs::create_dir(root.join("real/inner")).unwrap();
        assert!(fs::Dir::open(root.join("link/inner")).is_ok());
    }

    fn dir_handle_follows_renamed_directory(root) {
        fs::create_dir(root.join("before")).unwrap();
        let dir = fs::Dir::open(root.join("before")).unwrap();
        fs::rename(root.join("before"), root.join("after")).unwrap();
        dir.open_file_at("child", &OpenOptions::new().write(true).create(true))
            .unwrap();
        assert!(fs::metadata(root.join("after/child")).is_ok());
        dir.remove_file_at("child").unwrap();
        fs::remove_dir(root.join("after")).unwrap();
        assert_errno(
            dir.open_file_at("child", &OpenOptions::new().write(true).create(true)),
            Errno::NOENT,
        );
    }

    fn file_locks(root) {
        let path = root.join("lock");
        fs::write(&path, b"").unwrap();
        let first = fs::File::open(&path).unwrap();
        let second = fs::File::open(&path).unwrap();
        first.try_lock().unwrap();
        assert_kind(second.try_lock(), std::io::ErrorKind::WouldBlock);
        assert_kind(second.try_lock_shared(), std::io::ErrorKind::WouldBlock);
        first.unlock().unwrap();
        first.try_lock_shared().unwrap();
        second.try_lock_shared().unwrap();
        assert_kind(second.try_lock(), std::io::ErrorKind::WouldBlock);
        drop(first);
        second.try_lock().unwrap();
        let third = fs::File::open(&path).unwrap();
        assert_kind(third.try_lock_shared(), std::io::ErrorKind::WouldBlock);
        drop(second);
        third.try_lock().unwrap();
    }

    fn anonymous_temp_files(root) {
        let mut file = fs::tempfile_in(root).unwrap();
        file.write_all(b"scratch").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut buf = String::new();
        file.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "scratch");
        assert_eq!(file.metadata().unwrap().nlink(), Some(0));
        assert!(fs::read_dir_sorted(root).unwrap().is_empty());
        let mut global = fs::tempfile().unwrap();
        global.write_all(b"x").unwrap();
        assert_errno(fs::tempfile_in(root.join("missing")), Errno::NOENT);
    }

    fn sync_directory_checks_kind(root) {
        fs::write(root.join("file"), b"").unwrap();
        fs::sync_directory(root).unwrap();
        assert_kind(fs::sync_directory(root.join("file")), std::io::ErrorKind::NotADirectory);
        assert_kind(fs::sync_directory(root.join("missing")), std::io::ErrorKind::NotFound);
    }
}
