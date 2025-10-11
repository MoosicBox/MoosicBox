#[cfg(feature = "schema")]
pub mod introspection_tests;

#[cfg(feature = "schema")]
pub mod returning_tests;

pub mod savepoint_tests;

pub mod datetime_tests;

pub mod data_types_tests;

pub mod integration_tests;

#[cfg(feature = "cascade")]
pub mod cascade_tests;
