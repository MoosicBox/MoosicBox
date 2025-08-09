use std::sync::Arc;

use switchy_database::{Database, Row, query::FilterableQuery as _};

#[cfg(any(feature = "sqlite-rusqlite", feature = "sqlite-sqlx"))]
macro_rules! generate_tests {
    () => {
        #[switchy_async::test]
        #[test_log::test]
        async fn test_insert() {
            let db = setup_db().await;
            let db = &**db;

            // Insert a record
            db.insert("users")
                .value("name", "Alice")
                .execute(db)
                .await
                .unwrap();

            // Verify the record was inserted
            let rows = db
                .select("users")
                .where_eq("name", "Alice")
                .execute(db)
                .await
                .unwrap();

            assert_eq!(
                rows,
                vec![Row {
                    columns: vec![("id".into(), 1.into()), ("name".into(), "Alice".into())]
                }]
            );
        }

        #[switchy_async::test]
        #[test_log::test]
        async fn test_update() {
            let db = setup_db().await;
            let db = &**db;

            // Insert a record
            db.insert("users")
                .value("name", "Bob")
                .execute(db)
                .await
                .unwrap();

            // Update the record
            db.update("users")
                .value("name", "Charlie")
                .where_eq("name", "Bob")
                .execute(db)
                .await
                .unwrap();

            // Verify the record was updated
            let rows = db
                .select("users")
                .where_eq("name", "Charlie")
                .execute(db)
                .await
                .unwrap();

            assert_eq!(
                rows,
                vec![Row {
                    columns: vec![("id".into(), 1.into()), ("name".into(), "Charlie".into())]
                }]
            );
        }

        #[switchy_async::test]
        #[test_log::test]
        async fn test_delete() {
            let db = setup_db().await;
            let db = &**db;

            // Insert a record
            db.insert("users")
                .value("name", "Dave")
                .execute(db)
                .await
                .unwrap();

            // Delete the record
            let deleted = db
                .delete("users")
                .where_eq("name", "Dave")
                .execute(db)
                .await
                .unwrap();

            assert_eq!(
                deleted,
                vec![Row {
                    columns: vec![("id".into(), 1.into()), ("name".into(), "Dave".into())]
                }]
            );

            // Verify the record was deleted
            let rows = db
                .select("users")
                .where_eq("name", "Dave")
                .execute(db)
                .await
                .unwrap();

            assert_eq!(rows.len(), 0);
        }

        #[switchy_async::test]
        #[test_log::test]
        async fn test_delete_with_limit() {
            let db = setup_db().await;
            let db = &**db;

            // Insert a record
            db.insert("users")
                .value("name", "Dave")
                .execute(db)
                .await
                .unwrap();

            // Delete the record
            let deleted = db
                .delete("users")
                .where_not_eq("name", "Bob")
                .where_eq("name", "Dave")
                .execute_first(db)
                .await
                .unwrap();

            assert_eq!(
                deleted,
                Some(Row {
                    columns: vec![("id".into(), 1.into()), ("name".into(), "Dave".into())]
                })
            );

            // Verify the record was deleted
            let rows = db
                .select("users")
                .where_eq("name", "Dave")
                .execute(db)
                .await
                .unwrap();

            assert_eq!(rows.len(), 0);
        }
    };
}

#[cfg(feature = "sqlite-sqlx")]
mod sqlx_sqlite {
    use pretty_assertions::assert_eq;

    use super::*;

    async fn setup_db() -> Arc<Box<dyn Database>> {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();
        let db = Arc::new(db);

        // Create a sample table
        db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();
        db
    }

    generate_tests!();
}

#[cfg(feature = "sqlite-rusqlite")]
mod rusqlite {
    use pretty_assertions::assert_eq;

    use super::*;

    async fn setup_db() -> Arc<Box<dyn Database>> {
        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();
        let db = Arc::new(db);

        // Create a sample table
        db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();
        db
    }

    generate_tests!();
}
