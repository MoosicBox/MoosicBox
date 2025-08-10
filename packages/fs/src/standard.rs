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

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::read_dir` fails
    /// * If any directory entry cannot be read
    pub fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<std::fs::DirEntry>> {
        let mut entries: Vec<_> = ::std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        Ok(entries)
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read
    /// * If any directory entry cannot be accessed
    pub fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<std::fs::DirEntry>> {
        fn walk_recursive(
            path: &Path,
            entries: &mut Vec<std::fs::DirEntry>,
        ) -> std::io::Result<()> {
            let mut dir_entries: Vec<_> =
                ::std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            dir_entries.sort_by_key(std::fs::DirEntry::file_name);

            for entry in dir_entries {
                let path = entry.path();
                entries.push(entry);

                if path.is_dir() {
                    walk_recursive(&path, entries)?;
                }
            }
            Ok(())
        }

        let mut all_entries = Vec::new();
        walk_recursive(path.as_ref(), &mut all_entries)?;

        // Sort all entries by full path for deterministic order
        all_entries.sort_by_key(std::fs::DirEntry::path);
        Ok(all_entries)
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
