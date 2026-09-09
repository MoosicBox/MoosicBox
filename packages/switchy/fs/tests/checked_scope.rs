//! Checked namespace selection must reject native mode before callback effects.
#![cfg(all(feature = "simulator", feature = "simulator-real-fs"))]
#[test]
fn native_mode_does_not_invoke_callback() {
    use switchy_fs::simulator::{Filesystem, try_with_filesystem, with_real_fs};
    let owner = std::sync::Arc::new(Filesystem::new());
    let mut called = false;
    let result = with_real_fs(|| {
        try_with_filesystem(&owner, || {
            called = true;
            Ok(())
        })
    });
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Unsupported);
    assert!(!called);
    try_with_filesystem(&owner, || {
        called = true;
        Ok(())
    })
    .unwrap();
    assert!(called);
}
