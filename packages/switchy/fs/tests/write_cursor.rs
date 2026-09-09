//! Cursor and append semantics agree across native and simulated filesystems.
#![cfg(feature = "sync")]

#[test]
fn overwrite_sparse_and_append_writes() {
    use std::io::{Seek as _, SeekFrom, Write as _};
    use switchy_fs::sync::{OpenOptions, read};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let path = root.path().join("data");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(b"abcdef").unwrap();
    assert_eq!(file.stream_position().unwrap(), 6);
    file.seek(SeekFrom::Start(2)).unwrap();
    file.write_all(b"XY").unwrap();
    assert_eq!(read(&path).unwrap(), b"abXYef");
    file.seek(SeekFrom::Start(8)).unwrap();
    file.write_all(b"z").unwrap();
    assert_eq!(read(&path).unwrap(), b"abXYef\0\0z");
    let mut first = OpenOptions::new().append(true).open(&path).unwrap();
    let mut second = OpenOptions::new().append(true).open(&path).unwrap();
    first.seek(SeekFrom::Start(0)).unwrap();
    first.write_all(b"1").unwrap();
    second.write_all(b"2").unwrap();
    first.write_all(b"3").unwrap();
    assert_eq!(first.stream_position().unwrap(), 12);
    assert_eq!(read(&path).unwrap(), b"abXYef\0\0z123");
}
