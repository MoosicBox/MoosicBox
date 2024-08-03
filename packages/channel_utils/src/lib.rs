#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "futures-channel")]
pub mod futures_channel;

pub trait MoosicBoxSender<T, E> {
    fn send(&self, msg: T) -> Result<(), E>;
}
