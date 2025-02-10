use std::{path::PathBuf, sync::LazyLock};

use bytes::Bytes;

#[derive(Clone, Debug)]
pub struct StaticAssetRoute {
    pub route: String,
    pub target: AssetPathTarget,
}

#[derive(Clone, Debug)]
pub enum AssetPathTarget {
    File(PathBuf),
    FileContents(Bytes),
    Directory(PathBuf),
}

impl TryFrom<PathBuf> for AssetPathTarget {
    type Error = std::io::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Ok(if value.is_dir() {
            Self::Directory(value)
        } else if value.is_file() {
            Self::File(value)
        } else if value.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid file type for asset at {value:?}"),
            ));
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Asset doesn't exist at {value:?}"),
            ));
        })
    }
}

impl TryFrom<&str> for AssetPathTarget {
    type Error = std::io::Error;

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

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}
