pub use std::fs::File;

#[cfg(feature = "sync")]
pub mod sync {
    use std::path::Path;

    use crate::sync::OpenOptions;

    /// # Errors
    ///
    /// * If underlying `std::fs::read_to_string` fails
    pub fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        ::std::fs::read_to_string(path)
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
        pub fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<super::File> {
            let options: std::fs::OpenOptions = self.into();

            options.open(path)
        }
    }
}
