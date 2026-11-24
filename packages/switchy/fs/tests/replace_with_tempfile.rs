//! Native same-directory staging never writes through the old destination.
#![cfg(feature = "std")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_fs::standard::{sync::replace_with_tempfile, temp_dir::TempDir};

#[test]
fn replaces_without_truncating_old_file_and_cleans_failed_stage() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("state");
    let alias = root.path().join("old");
    std::fs::write(&path, b"old").unwrap();
    std::fs::hard_link(&path, &alias).unwrap();
    replace_with_tempfile(&path, b"new").unwrap();
    assert_eq!(std::fs::read(alias).unwrap(), b"old");
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o077, 0);
    }
    let directory = root.path().join("directory");
    std::fs::create_dir(&directory).unwrap();
    assert!(replace_with_tempfile(&directory, b"bad").is_err());
    assert!(directory.is_dir());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
}
