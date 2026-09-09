//! Participating owners contend on opened file identity.
#![cfg(feature = "sync")]

#[test]
fn lock_contends_across_rename_and_releases_on_drop() {
    use switchy_fs::sync::{File, rename_file, write};
    let root = switchy_fs::temp_dir::TempDir::new().unwrap();
    let path = root.path().join("owner");
    let moved = root.path().join("moved");
    write(&path, b"lock").unwrap();
    let guard = File::open(&path).unwrap().try_lock_exclusive().unwrap();
    let contender = File::open(&path).unwrap();
    std::thread::spawn(move || {
        let result = contender.try_lock_exclusive();
        assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
    })
    .join()
    .unwrap();
    let result = File::open(&path).unwrap().try_lock_exclusive();
    assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
    rename_file(&path, &moved).unwrap();
    let result = File::open(&moved).unwrap().try_lock_exclusive();
    assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
    std::thread::spawn(move || drop(guard)).join().unwrap();
    let _next = File::open(&moved).unwrap().try_lock_exclusive().unwrap();
}
