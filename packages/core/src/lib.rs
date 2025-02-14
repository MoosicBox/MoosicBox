#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use async_trait::async_trait;

pub mod app;
pub mod sqlite;

#[async_trait]
pub trait TryFromAsync<T>
where
    Self: Sized,
{
    type Error;

    async fn try_from_async(value: T) -> Result<Self, Self::Error>;
}
