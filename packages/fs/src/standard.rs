#[cfg(feature = "sync")]
pub mod sync {
    use std::path::Path;

    use crate::sync::OpenOptions;

    pub use std::fs::File;

    /// # Errors
    ///
    /// * If underlying `std::fs::read_to_string` fails
    pub fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        ::std::fs::read_to_string(path)
    }

    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir_all` fails
    pub fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        ::std::fs::create_dir_all(path)
    }

    /// # Errors
    ///
    /// * If underlying `std::fs::remove_dir_all` fails
    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        ::std::fs::remove_dir_all(path)
    }

    impl From<OpenOptions> for std::fs::OpenOptions {
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
        pub fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            let options: std::fs::OpenOptions = self.into();

            options.open(path)
        }
    }
}
