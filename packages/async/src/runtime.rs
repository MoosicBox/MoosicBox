use crate::Error;

pub trait GenericRuntime {
    fn block_on<F: Future + Send + 'static>(&self, f: F) -> F::Output
    where
        F::Output: Send;

    /// # Errors
    ///
    /// * If the `GenericRuntime` fails to join
    fn wait(self) -> Result<(), Error>;
}

pub struct Builder {
    #[cfg(feature = "rt-multi-thread")]
    pub max_blocking_threads: Option<u16>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            #[cfg(feature = "rt-multi-thread")]
            max_blocking_threads: None,
        }
    }

    #[cfg(feature = "rt-multi-thread")]
    pub fn max_blocking_threads<T: Into<Option<u16>>>(
        &mut self,
        max_blocking_threads: T,
    ) -> &mut Self {
        self.max_blocking_threads = max_blocking_threads.into();
        self
    }
}
