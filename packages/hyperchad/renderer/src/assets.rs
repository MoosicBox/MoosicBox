//! Static asset serving and routing configuration.
//!
//! This module provides types for configuring static asset routes in `HyperChad`
//! applications. Assets can be served from files, directories, or in-memory content.
//!
//! # Examples
//!
//! Creating a static asset route:
//!
//! ```rust
//! # #[cfg(feature = "assets")]
//! # {
//! use hyperchad_renderer::assets::{StaticAssetRoute, AssetPathTarget};
//! use std::path::PathBuf;
//!
//! # fn example() -> Result<(), std::io::Error> {
//! let route = StaticAssetRoute {
//!     route: "/static".to_string(),
//!     target: AssetPathTarget::Directory(PathBuf::from("./public")),
//! };
//! # Ok(())
//! # }
//! # }
//! ```

use std::{path::PathBuf, sync::LazyLock};

use bytes::Bytes;

/// Static asset route configuration
#[derive(Clone, Debug)]
pub struct StaticAssetRoute {
    /// HTTP route path
    pub route: String,
    /// Asset target (file, directory, or in-memory content)
    pub target: AssetPathTarget,
}

/// Target for static asset serving
#[derive(Clone, Debug)]
pub enum AssetPathTarget {
    /// Single file path
    File(PathBuf),
    /// In-memory file contents
    FileContents(Bytes),
    /// Directory path for serving multiple files
    Directory(PathBuf),
}

impl TryFrom<PathBuf> for AssetPathTarget {
    type Error = std::io::Error;

    /// # Errors
    ///
    /// * If the path exists but is neither a file nor a directory
    /// * If the path does not exist
    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Ok(if value.is_dir() {
            Self::Directory(value)
        } else if value.is_file() {
            Self::File(value)
        } else if value.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid file type for asset at {}", value.display()),
            ));
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Asset doesn't exist at {}", value.display()),
            ));
        })
    }
}

impl TryFrom<&str> for AssetPathTarget {
    type Error = std::io::Error;

    /// # Errors
    ///
    /// * If the path exists but is neither a file nor a directory
    /// * If the path does not exist
    ///
    /// # Panics
    ///
    /// * If the `ASSETS_DIR` environment variable is not set at compile time
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        static ASSETS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
            std::option_env!("ASSETS_DIR")
                .expect("Missing ASSETS_DIR")
                .into()
        });

        let mut path_buf = <&str as Into<PathBuf>>::into(value);

        if !path_buf.is_absolute() {
            path_buf = ASSETS_DIR.join(path_buf);
        }

        path_buf.try_into()
    }
}

impl TryFrom<String> for AssetPathTarget {
    type Error = std::io::Error;

    /// # Errors
    ///
    /// * If the path exists but is neither a file nor a directory
    /// * If the path does not exist
    ///
    /// # Panics
    ///
    /// * If the `ASSETS_DIR` environment variable is not set at compile time
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test_log::test]
    fn test_asset_path_target_try_from_directory() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let result = AssetPathTarget::try_from(temp_dir.path().to_path_buf());

        assert!(result.is_ok());
        match result.unwrap() {
            AssetPathTarget::Directory(path) => {
                assert_eq!(path, temp_dir.path());
            }
            _ => panic!("Expected Directory variant"),
        }
    }

    #[test_log::test]
    fn test_asset_path_target_try_from_file() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create file");
        file.write_all(b"test content")
            .expect("Failed to write to file");
        drop(file);

        let result = AssetPathTarget::try_from(file_path.clone());

        assert!(result.is_ok());
        match result.unwrap() {
            AssetPathTarget::File(path) => {
                assert_eq!(path, file_path);
            }
            _ => panic!("Expected File variant"),
        }
    }

    #[test_log::test]
    fn test_asset_path_target_try_from_nonexistent_path() {
        let nonexistent_path = PathBuf::from("/this/path/should/not/exist/ever/12345");

        let result = AssetPathTarget::try_from(nonexistent_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(err.to_string().contains("doesn't exist"));
    }

    #[test_log::test]
    fn test_static_asset_route_fields() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let target =
            AssetPathTarget::try_from(temp_dir.path().to_path_buf()).expect("Failed to create");

        let route = StaticAssetRoute {
            route: "/static".to_string(),
            target,
        };

        assert_eq!(route.route, "/static");
        match &route.target {
            AssetPathTarget::Directory(path) => {
                assert_eq!(path, temp_dir.path());
            }
            _ => panic!("Expected Directory variant"),
        }
    }

    #[test_log::test]
    fn test_asset_path_target_file_contents() {
        let content = bytes::Bytes::from("file content here");
        let target = AssetPathTarget::FileContents(content.clone());

        match target {
            AssetPathTarget::FileContents(data) => {
                assert_eq!(data, content);
            }
            _ => panic!("Expected FileContents variant"),
        }
    }

    #[test_log::test]
    fn test_asset_path_target_clone() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let original = AssetPathTarget::try_from(temp_dir.path().to_path_buf()).expect("Failed");

        #[allow(clippy::redundant_clone)]
        let cloned = original.clone();

        match (&original, &cloned) {
            (AssetPathTarget::Directory(orig_path), AssetPathTarget::Directory(cloned_path)) => {
                assert_eq!(orig_path, cloned_path);
            }
            _ => panic!("Expected both to be Directory variants"),
        }
    }

    #[test_log::test]
    fn test_static_asset_route_clone() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let target = AssetPathTarget::try_from(temp_dir.path().to_path_buf()).expect("Failed");

        let original = StaticAssetRoute {
            route: "/assets".to_string(),
            target,
        };

        #[allow(clippy::redundant_clone)]
        let cloned = original.clone();

        assert_eq!(original.route, cloned.route);
    }
}
