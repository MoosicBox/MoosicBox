//! No-follow entry inspection preserves backend semantics.
#![cfg(feature = "sync")]

#[test]
fn inspect_existing_and_missing_entries() {
    use switchy_fs::sync::{symlink_metadata, write};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    assert!(symlink_metadata(root.path()).unwrap().is_dir());
    let path = root.path().join("file");
    write(&path, b"data").unwrap();
    let metadata = symlink_metadata(&path).unwrap();
    assert!(metadata.is_file());
    assert!(!metadata.is_symlink());
    assert_eq!(metadata.len(), 4);
    assert_eq!(
        symlink_metadata(root.path().join("missing"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
}

#[cfg(all(unix, not(feature = "simulator")))]
#[test]
fn native_inspection_does_not_follow_dangling_link() {
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(root.path().join("missing"), &link).unwrap();
    let metadata = switchy_fs::sync::symlink_metadata(&link).unwrap();
    assert!(metadata.is_symlink());
    assert!(!metadata.is_file());
    assert!(!metadata.is_dir());
}
