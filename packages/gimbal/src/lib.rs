#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "async")]
pub use gimbal_async as unsync;
#[cfg(feature = "async-macros")]
pub use gimbal_async_macros as unsync_macros;
#[cfg(feature = "database")]
pub use gimbal_database as database;
#[cfg(feature = "database-connection")]
pub use gimbal_database_connection as database_connection;
#[cfg(feature = "fs")]
pub use gimbal_fs as fs;
#[cfg(feature = "http")]
pub use gimbal_http as http;
#[cfg(feature = "http-models")]
pub use gimbal_http_models as http_models;
#[cfg(feature = "mdns")]
pub use gimbal_mdns as mdns;
#[cfg(feature = "random")]
pub use gimbal_random as random;
#[cfg(feature = "tcp")]
pub use gimbal_tcp as tcp;
#[cfg(feature = "telemetry")]
pub use gimbal_telemetry as telemetry;
#[cfg(feature = "time")]
pub use gimbal_time as time;
#[cfg(feature = "upnp")]
pub use gimbal_upnp as upnp;
