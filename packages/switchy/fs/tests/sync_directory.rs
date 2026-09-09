//! Directory publication acknowledgement across native and no-crash backends.
#![cfg(all(feature = "sync", unix))]

#[test]
fn sync_validates_directory_and_preserves_published_bytes() {
    use std::io::{ErrorKind, Write as _};
    use switchy_fs::sync::{File, read, rename_file, sync_directory};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let pending = root.path().join("pending");
    let state = root.path().join("state");
    let mut file = File::create(&pending).unwrap();
    file.write_all(b"snapshot").unwrap();
    file.sync_all().unwrap();
    drop(file);
    rename_file(&pending, &state).unwrap();
    sync_directory(root.path()).unwrap();
    assert_eq!(read(&state).unwrap(), b"snapshot");
    assert_eq!(
        sync_directory(&state).unwrap_err().kind(),
        ErrorKind::NotADirectory
    );
    assert_eq!(
        sync_directory(&pending).unwrap_err().kind(),
        ErrorKind::NotFound
    );
}
