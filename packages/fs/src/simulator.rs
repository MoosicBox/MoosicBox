use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use bytes::BytesMut;

#[allow(clippy::type_complexity)]
static FILES: LazyLock<RwLock<BTreeMap<String, Arc<Mutex<BytesMut>>>>> =
    LazyLock::new(|| RwLock::new(BTreeMap::new()));

/// # Panics
///
/// * If the `FILES` `RwLock` fails to write to
pub fn reset_fs() {
    FILES.write().unwrap().clear();
}

macro_rules! path_to_str {
    ($path:expr) => {{
        $path.as_ref().to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "path is invalid str")
        })
    }};
}

pub struct File {
    pub(crate) data: Arc<Mutex<BytesMut>>,
    pub(crate) position: u64,
    pub(crate) write: bool,
}

#[cfg(feature = "sync")]
pub mod sync {
    use std::{
        path::Path,
        sync::{Arc, Mutex},
    };

    use bytes::{BufMut as _, BytesMut};

    use crate::sync::{File, OpenOptions};

    use super::FILES;

    impl std::io::Read for File {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }

            let binding = self.data.lock().unwrap();

            let len = binding.len();
            let pos = usize::try_from(self.position).unwrap();

            let remaining = len - pos;
            let read_count = std::cmp::min(remaining, buf.len());

            if read_count == 0 {
                return Ok(0);
            }

            let data = &binding[pos..(pos + read_count)];
            buf[..read_count].copy_from_slice(data);

            self.position += read_count as u64;

            drop(binding);

            Ok(read_count)
        }
    }

    impl std::io::Write for File {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.write {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "File not opened in write mode",
                ));
            }
            let mut binding = self.data.lock().unwrap();

            binding.put(buf);

            drop(binding);

            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for File {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.position = match pos {
                std::io::SeekFrom::Start(x) => x,
                std::io::SeekFrom::End(x) => {
                    u64::try_from(i64::try_from(self.data.lock().unwrap().len()).unwrap() - x)
                        .unwrap()
                }
                std::io::SeekFrom::Current(x) => {
                    u64::try_from(i64::try_from(self.position).unwrap() + x).unwrap()
                }
            };

            Ok(self.position)
        }
    }

    impl OpenOptions {
        /// # Errors
        ///
        /// * If and IO error occurs
        ///
        /// # Panics
        ///
        /// * If the `FILES` `RwLock` fails to read.
        pub fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            let location = path_to_str!(path)?;
            let data = if let Some(data) = { FILES.read().unwrap().get(location).cloned() } {
                data
            } else if self.create {
                let data = Arc::new(Mutex::new(BytesMut::new()));
                FILES
                    .write()
                    .unwrap()
                    .insert(location.to_string(), data.clone());
                data
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("File not found at path={location}"),
                ));
            };

            if self.truncate {
                data.lock().unwrap().clear();
            }

            Ok(File {
                data,
                position: 0,
                write: self.write,
            })
        }
    }

    /// # Errors
    ///
    /// * If the file doesn't exist
    /// * If the file contents cannot be converted to a UTF-8 encoded `String`
    /// * If the file `Path` cannot be converted to a `str`
    ///
    /// # Panics
    ///
    /// * If the `FILES` `RwLock` fails to read.
    pub fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        let location = path_to_str!(path)?;
        let Some(existing) = FILES.read().unwrap().get(location).cloned() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found at path={location}"),
            ));
        };

        String::from_utf8(existing.lock().unwrap().to_vec())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    #[cfg(test)]
    mod test {
        use std::{
            io::Read as _,
            sync::{Arc, Mutex},
        };

        use bytes::BytesMut;
        use pretty_assertions::assert_eq;

        use crate::simulator::FILES;

        use super::OpenOptions;

        #[tokio::test]
        async fn can_read_empty_file() {
            const FILENAME: &str = "sync::test1";

            let mut binding = FILES.write().unwrap();
            binding.insert(FILENAME.to_string(), Arc::new(Mutex::new(BytesMut::new())));
            drop(binding);

            let mut file = OpenOptions::new().create(true).open(FILENAME).unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).unwrap();

            assert_eq!(read_count, 0);
        }

        #[tokio::test]
        async fn can_read_small_bytes_file() {
            const FILENAME: &str = "sync::test2";

            let mut binding = FILES.write().unwrap();
            binding.insert(
                FILENAME.to_string(),
                Arc::new(Mutex::new(BytesMut::from(b"hey" as &[u8]))),
            );
            drop(binding);

            let mut file = OpenOptions::new().create(true).open(FILENAME).unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).unwrap();

            assert_eq!(read_count, 3);
        }
    }
}

#[cfg(feature = "async")]
pub mod unsync {
    use std::{path::Path, task::Poll};

    use crate::unsync::OpenOptions;

    use super::File;

    impl tokio::io::AsyncRead for File {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            use std::io::Read as _;

            let dst = buf.initialize_unfilled();

            match self.get_mut().read(dst) {
                Ok(count) => {
                    buf.advance(count);
                }
                Err(e) => return Poll::Ready(Err(e)),
            }

            Poll::Ready(Ok(()))
        }
    }

    impl tokio::io::AsyncSeek for File {
        fn start_seek(
            self: std::pin::Pin<&mut Self>,
            position: std::io::SeekFrom,
        ) -> std::io::Result<()> {
            use std::io::Seek as _;

            self.get_mut().seek(position)?;
            Ok(())
        }

        fn poll_complete(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<std::io::Result<u64>> {
            use std::io::Seek as _;

            Poll::Ready(self.get_mut().stream_position())
        }
    }

    impl tokio::io::AsyncWrite for File {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            use std::io::Write as _;

            Poll::Ready(self.get_mut().write(buf))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            use std::io::Write as _;

            Poll::Ready(self.get_mut().flush())
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl OpenOptions {
        /// # Errors
        ///
        /// * If and IO error occurs
        ///
        /// # Panics
        ///
        /// * If the `FILES` `RwLock` fails to read.
        #[allow(clippy::unused_async)]
        pub async fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            self.into_sync().open(path)
        }
    }

    /// # Errors
    ///
    /// * If the file doesn't exist
    /// * If the file contents cannot be converted to a UTF-8 encoded `String`
    /// * If the file `Path` cannot be converted to a `str`
    pub async fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        super::sync::read_to_string(path)
    }

    #[cfg(test)]
    #[allow(clippy::await_holding_lock)]
    mod test {
        use std::sync::{Arc, Mutex};

        use bytes::BytesMut;
        use pretty_assertions::assert_eq;
        use tokio::io::AsyncReadExt as _;

        use crate::simulator::FILES;

        use super::OpenOptions;

        #[tokio::test]
        async fn can_read_empty_file() {
            const FILENAME: &str = "unsync::test1";

            let mut binding = FILES.write().unwrap();
            binding.insert(FILENAME.to_string(), Arc::new(Mutex::new(BytesMut::new())));
            drop(binding);

            let mut file = OpenOptions::new()
                .create(true)
                .open(FILENAME)
                .await
                .unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).await.unwrap();

            assert_eq!(read_count, 0);
        }

        #[tokio::test]
        async fn can_read_small_bytes_file() {
            const FILENAME: &str = "unsync::test2";

            let mut binding = FILES.write().unwrap();
            binding.insert(
                FILENAME.to_string(),
                Arc::new(Mutex::new(BytesMut::from(b"hey" as &[u8]))),
            );
            drop(binding);

            let mut file = OpenOptions::new()
                .create(true)
                .open(FILENAME)
                .await
                .unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).await.unwrap();

            assert_eq!(read_count, 3);
        }
    }
}
