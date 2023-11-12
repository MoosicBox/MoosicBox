use std::sync::Mutex;

use futures_channel::mpsc::UnboundedSender;
use once_cell::sync::Lazy;
use tokio_tungstenite::tungstenite::Message;

#[cfg(feature = "sender")]
pub mod sender;

pub static SENDER: Lazy<Mutex<Option<UnboundedSender<Message>>>> = Lazy::new(|| Mutex::new(None));
static WS_HOST: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

pub fn init_host(
    host: String,
) -> std::result::Result<
    (),
    std::sync::PoisonError<
        std::sync::MutexGuard<'static, std::option::Option<std::string::String>>,
    >,
> {
    WS_HOST.lock()?.replace(host);
    Ok(())
}
