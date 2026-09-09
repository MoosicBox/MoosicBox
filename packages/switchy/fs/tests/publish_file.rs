//! Publication behavior is shared by native and no-crash simulated backends.
#![cfg(all(feature = "sync", unix))]

#[test]
fn publication_replaces_only_after_exclusive_staging() {
    use switchy_fs::sync::{publish_file, read, write};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let staging = root.path().join("pending");
    let state = root.path().join("state");
    write(&state, b"old").unwrap();
    write(&staging, b"unowned").unwrap();
    assert!(publish_file(&staging, &state, b"new").is_err());
    assert_eq!(read(&staging).unwrap(), b"unowned");
    assert_eq!(read(&state).unwrap(), b"old");
    let available = root.path().join("available");
    publish_file(&available, &state, b"new").unwrap();
    assert_eq!(read(&state).unwrap(), b"new");
    assert!(!switchy_fs::exists(&available));
    assert!(publish_file(&state, &state, b"invalid").is_err());
    assert_eq!(read(&state).unwrap(), b"new");
}

#[test]
fn rename_failure_preserves_staging_and_destination() {
    use switchy_fs::sync::{create_dir, publish_file, read, symlink_metadata};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let staging = root.path().join("pending");
    let destination = root.path().join("directory");
    create_dir(&destination).unwrap();
    assert!(publish_file(&staging, &destination, b"snapshot").is_err());
    assert_eq!(read(&staging).unwrap(), b"snapshot");
    assert!(symlink_metadata(&destination).unwrap().is_dir());
}

#[cfg(not(feature = "simulator"))]
#[test]
fn native_staging_and_published_files_are_private() {
    use std::os::unix::fs::PermissionsExt as _;
    use switchy_fs::sync::{create_private_file, publish_file};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let path = root.path().join("private");
    let file = create_private_file(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );
    drop(file);
    let destination = root.path().join("state");
    publish_file(root.path().join("pending"), &destination, b"snapshot").unwrap();
    assert_eq!(
        std::fs::metadata(destination).unwrap().permissions().mode() & 0o077,
        0
    );
}
