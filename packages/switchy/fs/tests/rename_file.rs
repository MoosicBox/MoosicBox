//! File publication behavior shared by native and simulated backends.
#![cfg(feature = "sync")]

#[test]
fn replacement_preserves_open_handles_and_failed_publication() {
    use std::io::Read as _;
    use switchy_fs::sync::{File, read, rename_file, write};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    write(&source, b"new").unwrap();
    write(&target, b"old").unwrap();
    let mut old = File::open(&target).unwrap();
    assert!(rename_file(&source, root.path().join("missing/target")).is_err());
    assert_eq!(read(&source).unwrap(), b"new");
    assert_eq!(read(&target).unwrap(), b"old");
    rename_file(&source, &target).unwrap();
    assert!(!switchy_fs::exists(&source));
    assert_eq!(read(&target).unwrap(), b"new");
    let mut bytes = Vec::new();
    old.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"old");
    rename_file(&target, &target).unwrap();
    assert_eq!(read(&target).unwrap(), b"new");
}
