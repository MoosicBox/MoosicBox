//! Native and simulator seek/read edge semantics.
#![cfg(feature = "sync")]
#[test]
fn eof_and_negative_seek() {
    use std::io::{Read as _, Seek as _, SeekFrom};
    use switchy_fs::sync::{File, write};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let path = root.path().join("data");
    write(&path, b"abc").unwrap();
    let mut file = File::open(&path).unwrap();
    assert_eq!(file.seek(SeekFrom::End(-1)).unwrap(), 2);
    let mut byte = [0];
    assert_eq!(file.read(&mut byte).unwrap(), 1);
    assert_eq!(byte, [b'c']);
    assert!(file.seek(SeekFrom::Current(-4)).is_err());
    assert_eq!(file.stream_position().unwrap(), 3);
    file.seek(SeekFrom::Start(100)).unwrap();
    assert_eq!(file.read(&mut byte).unwrap(), 0);
}
