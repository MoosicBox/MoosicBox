#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::pin::Pin;

pub trait WebServer {
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>>;
    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>>;
}
