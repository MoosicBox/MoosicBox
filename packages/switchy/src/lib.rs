#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "async")]
pub use switchy_async as unsync;
#[cfg(feature = "async-macros")]
pub use switchy_async_macros as unsync_macros;
#[cfg(feature = "database")]
pub use switchy_database as database;
#[cfg(feature = "database-connection")]
pub use switchy_database_connection as database_connection;
#[cfg(feature = "fs")]
pub use switchy_fs as fs;
#[cfg(feature = "http")]
pub use switchy_http as http;
#[cfg(feature = "http-models")]
pub use switchy_http_models as http_models;
#[cfg(feature = "mdns")]
pub use switchy_mdns as mdns;
#[cfg(feature = "random")]
pub use switchy_random as random;
#[cfg(feature = "tcp")]
pub use switchy_tcp as tcp;
#[cfg(feature = "telemetry")]
pub use switchy_telemetry as telemetry;
#[cfg(feature = "time")]
pub use switchy_time as time;
#[cfg(feature = "upnp")]
pub use switchy_upnp as upnp;
