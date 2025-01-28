pub mod handler;
pub mod server;

/// Connection ID.
pub type ConnId = u64;

/// Room ID.
pub type RoomId = String;

/// Message sent to a room/client.
pub type Msg = String;
