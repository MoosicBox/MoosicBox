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
