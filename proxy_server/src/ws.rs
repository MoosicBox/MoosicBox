#[cfg(feature = "server")]
pub mod api;
#[cfg(feature = "server")]
pub mod handler;
#[cfg(feature = "server")]
pub mod server;

/// Connection ID.
#[cfg(feature = "server")]
pub type ConnId = usize;

/// Room ID.
#[cfg(feature = "server")]
pub type RoomId = String;

/// Message sent to a room/client.
#[cfg(feature = "server")]
pub type Msg = String;
