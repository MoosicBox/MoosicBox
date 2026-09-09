//! Sync preserves bytes under native and no-crash simulator semantics.
#![cfg(feature = "sync")]

#[test]
fn sync_reports_backend_support_without_changing_bytes() {
    use std::io::Write as _;
    use switchy_fs::sync::{File, read};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let path = root.path().join("data");
    let mut file = File::create(&path).unwrap();
    file.write_all(b"contents").unwrap();
    file.sync_all().unwrap();
    assert_eq!(read(&path).unwrap(), b"contents");
}
