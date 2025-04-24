#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "simulator")]
pub mod simulator;
#[cfg(feature = "std")]
pub mod std;
#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(all(feature = "sync", feature = "std"))]
pub trait GenericSyncFile: Send + Sync + ::std::io::Read + ::std::io::Write {}

#[cfg(all(feature = "async", feature = "tokio"))]
pub trait GenericAsyncFile: Send + Sync + ::tokio::io::AsyncRead + ::tokio::io::AsyncWrite {}

#[allow(unused)]
macro_rules! impl_open_options {
    ($(,)?) => {
        pub struct OpenOptions {
            pub(crate) create: bool,
            pub(crate) append: bool,
            pub(crate) read: bool,
            pub(crate) write: bool,
            pub(crate) truncate: bool,
        }

        impl Default for OpenOptions {
            fn default() -> Self {
                Self::new()
            }
        }

        impl OpenOptions {
            #[must_use]
            pub const fn new() -> Self {
                Self {
                    create: false,
                    append: false,
                    read: false,
                    write: false,
                    truncate: false,
                }
            }

            #[must_use]
            pub const fn create(mut self, create: bool) -> Self {
                self.create = create;
                self
            }

            #[must_use]
            pub const fn append(mut self, append: bool) -> Self {
                self.append = append;
                self
            }

            #[must_use]
            pub const fn read(mut self, read: bool) -> Self {
                self.read = read;
                self
            }

            #[must_use]
            pub const fn write(mut self, write: bool) -> Self {
                self.write = write;
                self
            }

            #[must_use]
            pub const fn truncate(mut self, truncate: bool) -> Self {
                self.truncate = truncate;
                self
            }
        }
    };
}

#[allow(unused)]
macro_rules! impl_sync_fs {
    ($module:ident $(,)?) => {
        #[cfg(feature = "sync")]
        pub mod sync {
            pub use $crate::$module::File;
            pub use $crate::$module::sync::read_to_string;

            impl_open_options!();
        }
    };
}

#[allow(unused)]
macro_rules! impl_async_fs {
    ($module:ident $(,)?) => {
        #[cfg(feature = "async")]
        pub mod unsync {
            pub use $crate::$module::File;
            pub use $crate::$module::unsync::read_to_string;

            impl_open_options!();

            #[cfg(feature = "sync")]
            impl OpenOptions {
                #[must_use]
                pub const fn into_sync(self) -> crate::sync::OpenOptions {
                    crate::sync::OpenOptions {
                        create: self.create,
                        append: self.append,
                        read: self.read,
                        write: self.write,
                        truncate: self.truncate,
                    }
                }
            }

            #[cfg(feature = "sync")]
            impl From<OpenOptions> for crate::sync::OpenOptions {
                fn from(value: OpenOptions) -> Self {
                    value.into_sync()
                }
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_sync_fs!(simulator);
#[cfg(feature = "simulator")]
impl_async_fs!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "std"))]
impl_sync_fs!(std);

#[cfg(all(not(feature = "simulator"), feature = "tokio"))]
impl_async_fs!(tokio);
