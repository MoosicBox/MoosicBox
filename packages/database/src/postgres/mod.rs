#[allow(clippy::module_inception)]
#[cfg(feature = "postgres-raw")]
pub mod postgres;

#[cfg(all(feature = "postgres-raw", feature = "schema"))]
pub(crate) mod introspection;
