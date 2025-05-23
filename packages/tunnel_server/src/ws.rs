pub mod api;
pub mod handler;
pub mod server;

/// Connection ID.
pub type ConnId = u64;

/// Message sent to a room/client.
pub type Msg = String;
