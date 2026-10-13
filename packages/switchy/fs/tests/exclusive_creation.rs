//! Exclusive creation must preserve the existing file across backends.
#![cfg(all(feature = "sync", any(feature = "std", feature = "simulator")))]

#[test]
fn exclusive_creation_preserves_existing_bytes() {
    use std::io::{ErrorKind, Write as _};
    use switchy_fs::sync::{OpenOptions, read};

    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let path = root.path().join("exclusive");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(b"retained").unwrap();
    drop(file);
    let result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .truncate(true)
        .open(&path);
    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::AlreadyExists));
    assert_eq!(read(&path).unwrap(), b"retained");
    let child = path.join("child");
    let error = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&child)
        .err()
        .expect("creating a child of a file must fail");
    // Native Windows reports ERROR_PATH_NOT_FOUND for a non-directory parent;
    // Unix and the simulator report NotADirectory. Both must reject creation.
    if cfg!(all(windows, not(feature = "simulator"))) {
        assert_eq!(error.kind(), ErrorKind::NotFound);
    } else {
        assert_eq!(error.kind(), ErrorKind::NotADirectory);
    }
    assert!(!switchy_fs::exists(&child));
}
