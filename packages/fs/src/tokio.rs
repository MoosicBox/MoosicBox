#[cfg(feature = "async")]
pub mod unsync {
    use std::path::Path;

    use crate::unsync::OpenOptions;

    pub use tokio::fs::File;

    /// # Errors
    ///
    /// * If underlying `tokio::fs::read_to_string` fails
    pub async fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        ::tokio::fs::read_to_string(path).await
    }

    /// # Errors
    ///
    /// * If underlying `tokio::fs::create_dir_all` fails
    pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        tokio::fs::create_dir_all(path).await
    }

    /// # Errors
    ///
    /// * If underlying `tokio::fs::remove_dir_all` fails
    pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        tokio::fs::remove_dir_all(path).await
    }

    impl From<OpenOptions> for tokio::fs::OpenOptions {
        fn from(value: OpenOptions) -> Self {
            let mut options = Self::new();

            options
                .create(value.create)
                .append(value.append)
                .read(value.read)
                .write(value.write)
                .truncate(value.truncate);

            options
        }
    }

    #[cfg(not(feature = "simulator"))]
    impl OpenOptions {
        /// # Errors
        ///
        /// * If and IO error occurs
        pub async fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            let options: tokio::fs::OpenOptions = self.into();

            options.open(path).await
        }
    }
}
