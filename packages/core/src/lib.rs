#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

pub mod app;
mod cache;
pub mod slim;
pub mod sqlite;

pub trait ToApi<T> {
    fn to_api(&self) -> T;
}
