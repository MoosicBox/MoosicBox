#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "futures-channel")]
pub mod futures_channel;

pub trait MoosicBoxSender<T, E> {
    /// # Errors
    ///
    /// * If the send failed
    fn send(&self, msg: T) -> Result<(), E>;
}
