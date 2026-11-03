//! Multi-schema relation introspection, lossless decoding and raw result metadata
//! for the `SQLite`-family backends (rusqlite, sqlx-sqlite, simulator).
#![cfg(all(
    feature = "schema",
    feature = "raw-sql",
    any(feature = "sqlite-rusqlite", feature = "sqlite-sqlx")
))]

use switchy_database::{
    Database, DatabaseValue,
    schema::{DataType, RelationKind, RelationRef},
};

const SETUP: &str = "
    CREATE TABLE parents (id INTEGER PRIMARY KEY AUTOINCREMENT, name VARCHAR(40) NOT NULL);
    CREATE TABLE things (
        id INTEGER PRIMARY KEY,
        parent_id INTEGER REFERENCES parents(id) ON DELETE CASCADE,
        payload BLOB,
        price DECIMAL(10, 2),
        happened_on DATE,
        weird my_custom_type,
        untyped
    );
    CREATE INDEX things_parent ON things(parent_id);
    CREATE VIEW thing_names AS SELECT t.id, p.name FROM things t JOIN parents p ON p.id = t.parent_id;
";

async fn check_relations(db: &dyn Database) {
    db.exec_raw(SETUP).await.unwrap();

    let schemas = db.list_schemas().await.unwrap();
    let main = schemas
        .iter()
        .find(|s| s.name == "main")
        .expect("main schema");
    assert!(main.is_default);
    assert!(!main.is_system);

    let relations = db.list_relations(None).await.unwrap();
    let names: Vec<_> = relations
        .iter()
        .map(|r| (r.name.as_str(), r.kind.clone(), r.schema.as_deref()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("parents", RelationKind::Table, Some("main")),
            ("thing_names", RelationKind::View, Some("main")),
            ("things", RelationKind::Table, Some("main")),
        ]
    );
    assert_eq!(db.list_relations(Some("main")).await.unwrap(), relations);

    // Table info through the relation API
    let things = db
        .get_relation_info(&RelationRef::new("things"))
        .await
        .unwrap()
        .expect("things");
    assert_eq!(things.schema.as_deref(), Some("main"));
    assert_eq!(things.kind, RelationKind::Table);
    assert_eq!(things.columns["payload"].data_type, DataType::Blob);
    assert_eq!(things.columns["payload"].native_type, "BLOB");
    assert_eq!(things.columns["price"].data_type, DataType::Decimal(10, 2));
    assert_eq!(things.columns["price"].native_type, "DECIMAL(10, 2)");
    assert_eq!(things.columns["happened_on"].data_type, DataType::Date);
    assert_eq!(
        things.columns["weird"].data_type,
        DataType::Custom("my_custom_type".into())
    );
    assert_eq!(things.columns["weird"].native_type, "my_custom_type");
    assert_eq!(things.columns["untyped"].native_type, "");
    assert!(things.indexes.contains_key("things_parent"));
    assert_eq!(things.foreign_keys.len(), 1);

    // Existing table API stays table-only but reports schema/kind
    let parents = db.get_table_info("parents").await.unwrap().unwrap();
    assert_eq!(parents.schema.as_deref(), Some("main"));
    assert!(parents.columns["id"].auto_increment);
    assert_eq!(parents.columns["name"].data_type, DataType::VarChar(40));
    assert!(db.get_table_info("thing_names").await.unwrap().is_none());

    // Views
    let view = db
        .get_relation_info(&RelationRef::with_schema("main", "thing_names"))
        .await
        .unwrap()
        .expect("view");
    assert_eq!(view.kind, RelationKind::View);
    assert_eq!(view.columns.len(), 2);
    assert!(view.indexes.is_empty());
    let definition = db
        .get_view_definition(&RelationRef::new("thing_names"))
        .await
        .unwrap()
        .expect("view definition");
    assert!(definition.starts_with("CREATE VIEW thing_names"));
    assert!(
        db.get_view_definition(&RelationRef::new("things"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.get_relation_info(&RelationRef::new("missing"))
            .await
            .unwrap()
            .is_none()
    );
}

async fn check_values(db: &dyn Database) {
    db.exec_raw("CREATE TABLE vals (id INTEGER PRIMARY KEY, b BLOB, d TEXT, t TEXT, tz TEXT)")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
    let time = chrono::NaiveTime::from_hms_milli_opt(13, 14, 15, 250).unwrap();
    let tz = chrono::DateTime::parse_from_rfc3339("2024-02-29T13:14:15+02:00").unwrap();
    db.exec_raw_params(
        "INSERT INTO vals (id, b, d, t, tz) VALUES (?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(1),
            DatabaseValue::Bytes(vec![0, 1, 2, 255]),
            DatabaseValue::Date(date),
            DatabaseValue::Time(time),
            DatabaseValue::DateTimeTz(tz),
        ],
    )
    .await
    .unwrap();
    db.exec_raw_params(
        "INSERT INTO vals (id, b, d, t, tz) VALUES (?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(2),
            DatabaseValue::BytesOpt(None),
            DatabaseValue::DateOpt(None),
            DatabaseValue::TimeOpt(None),
            DatabaseValue::DateTimeTzOpt(None),
        ],
    )
    .await
    .unwrap();

    let rows = db
        .query_raw_params("SELECT b, d, t, tz FROM vals WHERE id = ?", &[1_i64.into()])
        .await
        .unwrap();
    assert_eq!(
        rows[0].get("b"),
        Some(DatabaseValue::Bytes(vec![0, 1, 2, 255]))
    );
    assert_eq!(
        rows[0].get("d"),
        Some(DatabaseValue::String("2024-02-29".into()))
    );
    assert_eq!(
        rows[0].get("t"),
        Some(DatabaseValue::String("13:14:15.250".into()))
    );
    assert_eq!(
        rows[0].get("tz"),
        Some(DatabaseValue::String("2024-02-29T13:14:15+02:00".into()))
    );

    // BLOB parameters compare as blobs
    let matched = db
        .query_raw_params(
            "SELECT id FROM vals WHERE b = ?",
            &[DatabaseValue::Bytes(vec![0, 1, 2, 255])],
        )
        .await
        .unwrap();
    assert_eq!(matched.len(), 1);

    let nulls = db
        .query_raw_params("SELECT b, d FROM vals WHERE id = ?", &[2_i64.into()])
        .await
        .unwrap();
    assert!(nulls[0].get("b").unwrap().is_null());
    assert!(nulls[0].get("d").unwrap().is_null());

    // Raw result metadata
    let result = db
        .query_raw_result(
            "SELECT id, b AS payload FROM vals WHERE id > ?",
            &[100_i64.into()],
        )
        .await
        .unwrap();
    assert!(result.rows.is_empty());
    let names: Vec<_> = result.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "payload"]);
    assert!(result.columns[0].type_name.is_some());

    let result = db
        .query_raw_result("SELECT 1 AS a, 'x' AS a, b FROM vals ORDER BY id", &[])
        .await
        .unwrap();
    let names: Vec<_> = result.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a", "a", "b"]);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(
        result.rows[0],
        vec![
            DatabaseValue::Int64(1),
            DatabaseValue::String("x".into()),
            DatabaseValue::Bytes(vec![0, 1, 2, 255]),
        ]
    );
    assert_eq!(result.rows[1][2], DatabaseValue::Null);
}

#[cfg(feature = "sqlite-rusqlite")]
mod rusqlite_backend {
    use super::*;
    use switchy_database::rusqlite::RusqliteDatabase;

    fn db() -> RusqliteDatabase {
        RusqliteDatabase::new(vec![std::sync::Arc::new(switchy_async::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ))])
    }

    #[tokio::test]
    async fn relations() {
        check_relations(&db()).await;
    }

    #[tokio::test]
    async fn values_and_raw_results() {
        check_values(&db()).await;
    }

    #[tokio::test]
    async fn attached_schema_and_transactions() {
        let db = db();
        db.exec_raw(
            "ATTACH DATABASE ':memory:' AS aux;
             CREATE TABLE aux.items (id INTEGER PRIMARY KEY, label TEXT NOT NULL);
             CREATE INDEX aux.items_label ON items(label);
             CREATE VIEW aux.labels AS SELECT label FROM items;
             CREATE TEMP TABLE scratch (x INTEGER);",
        )
        .await
        .unwrap();

        let schemas = db.list_schemas().await.unwrap();
        let aux = schemas.iter().find(|s| s.name == "aux").expect("aux");
        assert!(!aux.is_default && !aux.is_system);
        let temp = schemas.iter().find(|s| s.name == "temp").expect("temp");
        assert!(temp.is_system);

        let relations = db.list_relations(Some("aux")).await.unwrap();
        assert_eq!(relations.len(), 2);
        assert!(relations.iter().all(|r| r.schema.as_deref() == Some("aux")));
        let temp_relations = db.list_relations(Some("temp")).await.unwrap();
        assert_eq!(temp_relations[0].name, "scratch");

        // Not visible in the default schema
        assert!(db.list_relations(None).await.unwrap().is_empty());
        assert!(
            db.get_relation_info(&RelationRef::new("items"))
                .await
                .unwrap()
                .is_none()
        );

        let items = db
            .get_relation_info(&RelationRef::with_schema("aux", "items"))
            .await
            .unwrap()
            .expect("aux.items");
        assert_eq!(items.schema.as_deref(), Some("aux"));
        assert!(items.indexes.contains_key("items_label"));
        assert!(!items.columns["label"].nullable);

        let tx = db.begin_transaction().await.unwrap();
        let view = tx
            .get_relation_info(&RelationRef::with_schema("aux", "labels"))
            .await
            .unwrap()
            .expect("aux.labels");
        assert_eq!(view.kind, RelationKind::View);
        assert!(
            tx.get_view_definition(&RelationRef::with_schema("aux", "labels"))
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(tx.list_relations(Some("aux")).await.unwrap().len(), 2);
        assert!(tx.list_schemas().await.unwrap().len() >= 3);
        let result = tx
            .query_raw_result("SELECT id, label FROM aux.items", &[])
            .await
            .unwrap();
        assert_eq!(result.columns[1].type_name.as_deref(), Some("TEXT"));
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn query_builder_still_requires_bounded_blob_projection() {
        let db = db();
        db.exec_raw("CREATE TABLE blobs (b BLOB); INSERT INTO blobs VALUES (X'0102');")
            .await
            .unwrap();
        assert!(db.select("blobs").execute(&db).await.is_err());
        let rows = db.query_raw("SELECT b FROM blobs").await.unwrap();
        assert_eq!(rows[0].get("b"), Some(DatabaseValue::Bytes(vec![1, 2])));
    }
}

#[cfg(feature = "sqlite-sqlx")]
mod sqlx_backend {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    async fn db() -> SqliteSqlxDatabase {
        use std::sync::atomic::{AtomicU32, Ordering};

        // The backend caches one pooled connection, so use a file shared by several.
        static ID: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "switchy_relation_metadata_{}_{}.db",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(&format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        SqliteSqlxDatabase::new(Arc::new(switchy_async::sync::Mutex::new(pool)))
    }

    #[tokio::test]
    async fn relations() {
        check_relations(&db().await).await;
    }

    #[tokio::test]
    async fn values_and_raw_results() {
        check_values(&db().await).await;
    }

    #[tokio::test]
    async fn transaction_relations() {
        let db = db().await;
        db.exec_raw("CREATE TABLE t (id INTEGER PRIMARY KEY); CREATE VIEW v AS SELECT id FROM t;")
            .await
            .unwrap();
        let tx = db.begin_transaction().await.unwrap();
        assert_eq!(tx.list_relations(None).await.unwrap().len(), 2);
        assert!(
            tx.list_schemas()
                .await
                .unwrap()
                .iter()
                .any(|s| s.is_default)
        );
        assert_eq!(
            tx.get_relation_info(&RelationRef::new("v"))
                .await
                .unwrap()
                .unwrap()
                .kind,
            RelationKind::View
        );
        assert!(
            tx.get_view_definition(&RelationRef::new("v"))
                .await
                .unwrap()
                .is_some()
        );
        let result = tx.query_raw_result("SELECT id FROM t", &[]).await.unwrap();
        assert_eq!(result.columns[0].name, "id");
        assert!(result.rows.is_empty());
        tx.rollback().await.unwrap();
    }
}

#[cfg(feature = "simulator")]
mod simulator_backend {
    use super::*;
    use switchy_database::simulator::SimulationDatabase;

    #[tokio::test]
    async fn relations() {
        check_relations(&SimulationDatabase::new().unwrap()).await;
    }

    #[tokio::test]
    async fn values_and_raw_results() {
        check_values(&SimulationDatabase::new().unwrap()).await;
    }
}
