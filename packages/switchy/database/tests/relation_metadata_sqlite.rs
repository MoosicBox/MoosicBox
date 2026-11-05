//! Multi-schema relation introspection, lossless decoding and raw result metadata
//! for the `SQLite`-family backends (rusqlite, sqlx-sqlite, simulator).
#![cfg(all(
    feature = "schema",
    feature = "raw-sql",
    any(feature = "sqlite-rusqlite", feature = "sqlite-sqlx")
))]

use switchy_database::{
    Database, DatabaseValue,
    schema::{DataType, GeneratedKind, RelationKind, RelationRef},
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

async fn check_routines(db: &dyn Database) {
    use switchy_database::schema::RoutineKind;
    let routines = db.list_routines(None).await.unwrap();
    let lower = routines.iter().find(|r| r.name == "lower").unwrap();
    assert!(lower.builtin);
    assert_eq!(lower.kind, RoutineKind::Function);
    // SQLite reports window-capable aggregates (like count) as window functions.
    assert!(routines.iter().any(|r| {
        r.name == "count" && matches!(r.kind, RoutineKind::Aggregate | RoutineKind::Window)
    }));
}

/// Triggers are listed with their table, timing and events (read from the
/// stored statement); `SQLite` has no sequences, types or roles.
async fn check_triggers(db: &dyn Database) {
    db.exec_raw("CREATE TABLE trig_items (id INTEGER PRIMARY KEY, n INTEGER)")
        .await
        .unwrap();
    db.exec_raw(
        "CREATE TRIGGER trig_items_stamp AFTER UPDATE OF n ON trig_items \
         BEGIN SELECT 1; END",
    )
    .await
    .unwrap();
    let triggers = db.list_triggers(None).await.unwrap();
    let stamp = triggers
        .iter()
        .find(|t| t.name == "trig_items_stamp")
        .unwrap();
    assert_eq!(stamp.schema.as_deref(), Some("main"));
    assert_eq!(stamp.table.as_deref(), Some("trig_items"));
    assert_eq!(stamp.timing.as_deref(), Some("AFTER"));
    assert_eq!(stamp.events, vec!["UPDATE"]);
    assert!(stamp.enabled);
    assert!(
        stamp
            .definition
            .as_deref()
            .is_some_and(|sql| sql.contains("BEGIN"))
    );
    assert_eq!(db.list_triggers(Some("main")).await.unwrap(), triggers);
    assert!(db.list_sequences(None).await.is_err());
    assert!(db.list_roles().await.is_err());
}

/// Generated columns (listed by `PRAGMA table_xinfo`, not `table_info`) are
/// reported with their kind, keep their ordinal and have no default; `SQLite`
/// has no enum types.
async fn check_generated_columns(db: &dyn Database) {
    db.exec_raw(
        "CREATE TABLE gen_cols (
            a INTEGER NOT NULL DEFAULT 1,
            v INTEGER GENERATED ALWAYS AS (a * 2) VIRTUAL,
            s TEXT GENERATED ALWAYS AS ('x' || a) STORED,
            b TEXT
        )",
    )
    .await
    .unwrap();

    let info = db
        .get_relation_info(&RelationRef::new("gen_cols"))
        .await
        .unwrap()
        .expect("gen_cols");
    assert_eq!(info.columns.len(), 4);
    let column = |name: &str| &info.columns[name];
    assert_eq!(column("a").generated, None);
    assert_eq!(column("a").default_value, Some(DatabaseValue::Int64(1)));
    assert_eq!(column("v").generated, Some(GeneratedKind::Virtual));
    assert_eq!(column("v").native_type, "INTEGER");
    assert_eq!(column("v").default_value, None);
    assert_eq!(column("v").ordinal_position, 2);
    assert_eq!(column("s").generated, Some(GeneratedKind::Stored));
    assert_eq!(column("s").ordinal_position, 3);
    assert_eq!(column("b").generated, None);
    assert_eq!(column("b").ordinal_position, 4);
    assert!(info.columns.values().all(|c| c.enum_values.is_none()));

    let columns = db.get_table_columns("gen_cols").await.unwrap();
    let names: Vec<_> = columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["a", "v", "s", "b"]);
    assert!(db.column_exists("gen_cols", "s").await.unwrap());
}

async fn check_relations(db: &dyn Database) {
    db.exec_raw(SETUP).await.unwrap();

    let schemas = db.list_schemas().await.unwrap();
    let main = schemas
        .iter()
        .find(|s| s.name == "main")
        .expect("main schema");
    assert!(main.is_default);
    assert!(!main.is_system);

    let databases = db.list_databases().await.unwrap();
    let main = databases
        .iter()
        .find(|d| d.name == "main")
        .expect("main database");
    assert!(main.is_current);
    assert!(!main.is_system);
    assert_eq!(databases.iter().filter(|d| d.is_current).count(), 1);

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

/// Composite keys keep every column pair (keys to a primary key implicitly
/// report its columns), defaults keep their SQL text, and keys are listed
/// in both directions.
async fn check_foreign_keys(db: &dyn Database) {
    use switchy_database::schema::ForeignKeyScope;
    db.exec_raw("CREATE TABLE albums (id INTEGER PRIMARY KEY, title TEXT DEFAULT 'untitled')")
        .await
        .unwrap();
    db.exec_raw(
        "CREATE TABLE tracks (album_id INTEGER NOT NULL REFERENCES albums, \
         track_no INTEGER NOT NULL, PRIMARY KEY (album_id, track_no))",
    )
    .await
    .unwrap();
    db.exec_raw(
        "CREATE TABLE plays (id INTEGER PRIMARY KEY, album_id INTEGER, track_no INTEGER, \
         FOREIGN KEY (album_id, track_no) REFERENCES tracks (album_id, track_no) \
         ON DELETE CASCADE)",
    )
    .await
    .unwrap();

    let albums = db
        .get_relation_info(&RelationRef::new("albums"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        albums.columns["title"].default_text.as_deref(),
        Some("'untitled'")
    );
    assert_eq!(albums.columns["id"].default_text, None);

    let plays = db
        .get_relation_info(&RelationRef::new("plays"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(plays.foreign_keys.len(), 1);
    let key = plays.foreign_keys.values().next().unwrap();
    assert_eq!(key.columns, vec!["album_id", "track_no"]);
    assert_eq!(key.referenced_columns, vec!["album_id", "track_no"]);
    assert_eq!(key.column, "album_id");
    assert_eq!(key.referenced_schema.as_deref(), Some("main"));

    // An implicit reference to the primary key reports its column.
    let tracks = db
        .get_relation_info(&RelationRef::new("tracks"))
        .await
        .unwrap()
        .unwrap();
    let (name, key) = tracks.foreign_keys.iter().next().unwrap();
    assert_eq!(name, "tracks_album_id_albums_id");
    assert_eq!(key.referenced_columns, vec!["id"]);

    let around = db
        .list_foreign_keys(&ForeignKeyScope::Relation(RelationRef::new("tracks")))
        .await
        .unwrap();
    let described: Vec<String> = around
        .iter()
        .map(|key| {
            format!(
                "{} ({}) -> {} ({}) {:?}",
                key.table,
                key.foreign_key.columns.join(", "),
                key.referenced(),
                key.foreign_key.referenced_columns.join(", "),
                key.foreign_key.on_delete
            )
        })
        .collect();
    assert_eq!(
        described,
        vec![
            "main.plays (album_id, track_no) -> main.tracks (album_id, track_no) Some(\"CASCADE\")",
            "main.tracks (album_id) -> main.albums (id) Some(\"NO ACTION\")",
        ]
    );
    // Listed keys are named as table introspection names them.
    assert_eq!(
        &around[0].foreign_key.name,
        plays.foreign_keys.keys().next().unwrap()
    );
    let referencing = db
        .get_referencing_foreign_keys(&RelationRef::new("albums"))
        .await
        .unwrap();
    assert_eq!(referencing.len(), 1);
    assert_eq!(referencing[0].table.name, "tracks");
    assert_eq!(
        db.list_foreign_keys(&ForeignKeyScope::Schema(None))
            .await
            .unwrap()
            .len(),
        2
    );
}

/// Row counts come from `sqlite_stat1` once `ANALYZE` has run; tables that
/// were empty then have none.
async fn check_statistics(db: &dyn Database) {
    db.exec_raw(
        "CREATE TABLE stat_full (x INTEGER);
         CREATE TABLE stat_indexed (x INTEGER PRIMARY KEY, y INTEGER);
         CREATE INDEX stat_indexed_y ON stat_indexed(y);
         CREATE TABLE stat_empty (x INTEGER);
         CREATE VIEW stat_view AS SELECT x FROM stat_full;
         INSERT INTO stat_full VALUES (1), (2), (3);
         INSERT INTO stat_indexed (y) VALUES (1), (2);",
    )
    .await
    .unwrap();
    let find = |statistics: &[switchy_database::schema::RelationStatistics], name: &str| {
        statistics.iter().find(|t| t.name == name).cloned().unwrap()
    };
    let before = db.list_relation_statistics(None).await.unwrap();
    assert!(
        before.iter().all(|t| t.name != "stat_view"),
        "views have none"
    );
    let full = find(&before, "stat_full");
    assert_eq!(full.schema.as_deref(), Some("main"));
    assert_eq!(full.planner_rows, None);
    assert_eq!(full.ever_analyzed, Some(false));
    assert!(full.analyzable);

    db.exec_raw("ANALYZE").await.unwrap();
    let after = db.list_relation_statistics(Some("main")).await.unwrap();
    assert_eq!(find(&after, "stat_full").planner_rows, Some(3));
    assert_eq!(find(&after, "stat_indexed").planner_rows, Some(2));
    let empty = find(&after, "stat_empty");
    assert_eq!(empty.planner_rows, None);
    assert_eq!(empty.ever_analyzed, Some(true));
    assert!(after.iter().all(|t| !t.name.starts_with("sqlite_")));
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
    async fn statistics() {
        check_statistics(&db()).await;
    }

    #[tokio::test]
    async fn routines() {
        check_routines(&db()).await;
    }

    #[tokio::test]
    async fn triggers() {
        check_triggers(&db()).await;
    }

    #[tokio::test]
    async fn generated_columns() {
        check_generated_columns(&db()).await;
    }

    #[tokio::test]
    async fn foreign_keys() {
        check_foreign_keys(&db()).await;
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

        let databases = db.list_databases().await.unwrap();
        let aux = databases.iter().find(|d| d.name == "aux").expect("aux");
        assert!(!aux.is_current && !aux.is_system);
        let temp = databases.iter().find(|d| d.name == "temp").expect("temp");
        assert!(temp.is_system && !temp.is_current);

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
        assert!(tx.list_databases().await.unwrap().len() >= 3);
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
    async fn statistics() {
        check_statistics(&db().await).await;
    }

    #[tokio::test]
    async fn routines() {
        check_routines(&db().await).await;
    }

    #[tokio::test]
    async fn triggers() {
        check_triggers(&db().await).await;
    }

    #[tokio::test]
    async fn generated_columns() {
        check_generated_columns(&db().await).await;
    }

    #[tokio::test]
    async fn values_and_raw_results() {
        check_values(&db().await).await;
    }

    #[tokio::test]
    async fn foreign_keys() {
        check_foreign_keys(&db().await).await;
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

    #[tokio::test]
    async fn generated_columns() {
        check_generated_columns(&SimulationDatabase::new().unwrap()).await;
    }
}
