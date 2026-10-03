//! Compatibility compilation for legacy helpers; no caller SQL is executed.
#![cfg(feature = "raw-sql")]

#[test]
fn legacy_bulk_update_helpers_remain_available_with_raw() {
    #[cfg(feature = "sqlite-sqlx")]
    let _ = switchy_database::sqlx::sqlite::update_multi;
    #[cfg(feature = "mysql-sqlx")]
    let _ = switchy_database::sqlx::mysql::update_multi;
    #[cfg(feature = "postgres-sqlx")]
    let _ = switchy_database::sqlx::postgres::update_multi;
    #[cfg(feature = "postgres-raw")]
    let _ = switchy_database::postgres::postgres::update_multi;
}
