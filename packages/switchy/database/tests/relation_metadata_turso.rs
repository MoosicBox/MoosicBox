#![cfg(all(feature = "turso", feature = "schema", feature = "raw-sql"))]

use switchy_database::{
    Database, DatabaseValue,
    schema::{DataType, RelationKind, RelationRef},
    turso::TursoDatabase,
};

async fn setup() -> TursoDatabase {
    let db = TursoDatabase::new(":memory:").await.unwrap();
    db.exec_raw(
        "CREATE TABLE parents (id INTEGER PRIMARY KEY AUTOINCREMENT, name VARCHAR(50) NOT NULL)",
    )
    .await
    .unwrap();
    db.exec_raw(
        "CREATE TABLE children (\
            id INTEGER PRIMARY KEY, \
            parent_id INTEGER, \
            payload BLOB, \
            price DECIMAL(10, 2), \
            weird my_custom_type, \
            FOREIGN KEY (parent_id) REFERENCES parents(id) ON DELETE CASCADE)",
    )
    .await
    .unwrap();
    db.exec_raw("CREATE INDEX idx_children_parent ON children (parent_id)")
        .await
        .unwrap();
    db.exec_raw("CREATE VIEW parent_names AS SELECT id, name FROM parents")
        .await
        .unwrap();
    db
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn lists_schemas_with_main_default() {
    let db = setup().await;
    let schemas = db.list_schemas().await.unwrap();
    let main = schemas.iter().find(|s| s.name == "main").unwrap();
    assert!(main.is_default);
    assert!(!main.is_system);
    assert_eq!(schemas.iter().filter(|s| s.is_default).count(), 1);
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn lists_databases_with_main_current() {
    let db = setup().await;
    let databases = db.list_databases().await.unwrap();
    let main = databases.iter().find(|d| d.name == "main").unwrap();
    assert!(main.is_current);
    assert!(!main.is_system);
    assert_eq!(databases.iter().filter(|d| d.is_current).count(), 1);
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn lists_tables_and_views() {
    let db = setup().await;
    let relations = db.list_relations(None).await.unwrap();
    let names: Vec<_> = relations
        .iter()
        .map(|r| (r.name.as_str(), r.kind.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("children", RelationKind::Table),
            ("parent_names", RelationKind::View),
            ("parents", RelationKind::Table),
        ]
    );
    assert!(
        relations
            .iter()
            .all(|r| r.schema.as_deref() == Some("main") && r.estimated_rows.is_none())
    );
    assert_eq!(db.list_relations(Some("main")).await.unwrap(), relations);
    assert!(
        db.list_relations(Some("does_not_exist"))
            .await
            .unwrap()
            .is_empty()
    );
    // Table-oriented API still lists only tables
    let mut tables = db.list_tables().await.unwrap();
    tables.sort();
    assert_eq!(tables, vec!["children", "parents"]);
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn relation_info_for_tables_and_views() {
    let db = setup().await;

    let children = db
        .get_relation_info(&RelationRef::with_schema("main", "children"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(children.schema.as_deref(), Some("main"));
    assert_eq!(children.kind, RelationKind::Table);
    assert_eq!(children.columns["payload"].data_type, DataType::Blob);
    assert_eq!(children.columns["payload"].native_type, "BLOB");
    // Turso reports declared types without arguments (`DECIMAL(10, 2)` -> `DECIMAL`)
    assert_eq!(children.columns["price"].native_type, "DECIMAL");
    assert_eq!(
        children.columns["weird"].data_type,
        DataType::Custom("my_custom_type".to_string())
    );
    assert_eq!(children.columns["weird"].native_type, "my_custom_type");
    assert!(children.indexes.contains_key("idx_children_parent"));
    let fk = children.foreign_keys.values().next().unwrap();
    assert_eq!(fk.referenced_table, "parents");
    assert_eq!(fk.on_delete.as_deref(), Some("CASCADE"));

    let parents = db.get_table_info("parents").await.unwrap().unwrap();
    assert_eq!(parents.columns["name"].native_type, "VARCHAR");
    assert!(!parents.columns["name"].nullable);
    assert!(parents.columns["id"].auto_increment);

    let view = db
        .get_relation_info(&RelationRef::new("parent_names"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.kind, RelationKind::View);
    assert!(view.columns.contains_key("id"));
    assert!(view.columns.contains_key("name"));
    assert!(view.indexes.is_empty());

    // Table-oriented API does not report views
    assert!(db.get_table_info("parent_names").await.unwrap().is_none());
    assert!(
        db.get_relation_info(&RelationRef::new("missing"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.get_relation_info(&RelationRef::with_schema("nope", "parents"))
            .await
            .unwrap()
            .is_none()
    );
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn view_definitions() {
    let db = setup().await;
    let definition = db
        .get_view_definition(&RelationRef::new("parent_names"))
        .await
        .unwrap()
        .unwrap();
    assert!(definition.contains("SELECT id, name FROM parents"));
    assert!(
        db.get_view_definition(&RelationRef::new("parents"))
            .await
            .unwrap()
            .is_none()
    );
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn blobs_and_new_values_round_trip() {
    let db = setup().await;
    db.exec_raw_params(
        "INSERT INTO children (id, payload, weird) VALUES (?, ?, ?)",
        &[
            DatabaseValue::Int64(1),
            DatabaseValue::Bytes(vec![0, 1, 2, 255]),
            DatabaseValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()),
        ],
    )
    .await
    .unwrap();
    db.exec_raw_params(
        "INSERT INTO children (id, payload, weird) VALUES (?, ?, ?)",
        &[
            DatabaseValue::Int64(2),
            DatabaseValue::BytesOpt(None),
            DatabaseValue::Other {
                type_name: "x".to_string(),
                text: "other text".to_string(),
            },
        ],
    )
    .await
    .unwrap();

    let rows = db
        .query_raw("SELECT payload, weird FROM children ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        rows[0].get("payload").unwrap().as_bytes(),
        Some(&[0u8, 1, 2, 255][..])
    );
    assert_eq!(rows[0].get("weird").unwrap().as_str(), Some("2024-02-29"));
    assert!(rows[1].get("payload").unwrap().is_null());
    assert_eq!(rows[1].get("weird").unwrap().as_str(), Some("other text"));
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn query_raw_result_preserves_columns() {
    let db = setup().await;

    let empty = db
        .query_raw_result("SELECT id, name FROM parents", &[])
        .await
        .unwrap();
    assert!(empty.rows.is_empty());
    let columns: Vec<_> = empty
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.type_name.as_deref()))
        .collect();
    assert_eq!(
        columns,
        vec![("id", Some("INTEGER")), ("name", Some("VARCHAR"))]
    );

    db.exec_raw("INSERT INTO parents (name) VALUES ('a')")
        .await
        .unwrap();
    let dup = db
        .query_raw_result(
            "SELECT name, name, 1 + ? FROM parents",
            &[DatabaseValue::Int64(1)],
        )
        .await
        .unwrap();
    assert_eq!(dup.columns.len(), 3);
    assert_eq!(dup.columns[0].name, "name");
    assert_eq!(dup.columns[1].name, "name");
    assert_eq!(dup.columns[2].type_name, None);
    assert_eq!(dup.rows.len(), 1);
    assert_eq!(
        dup.rows[0],
        vec![
            DatabaseValue::String("a".to_string()),
            DatabaseValue::String("a".to_string()),
            DatabaseValue::Int64(2),
        ]
    );
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn transaction_introspection_sees_uncommitted_objects() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    tx.exec_raw("CREATE TABLE tx_only (x TEXT)").await.unwrap();
    let relations = tx.list_relations(None).await.unwrap();
    assert!(relations.iter().any(|r| r.name == "tx_only"));
    let info = tx
        .get_relation_info(&RelationRef::new("tx_only"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.columns["x"].native_type, "TEXT");
    assert!(
        tx.get_view_definition(&RelationRef::new("parent_names"))
            .await
            .unwrap()
            .is_some()
    );
    let result = tx
        .query_raw_result("SELECT x FROM tx_only", &[])
        .await
        .unwrap();
    assert_eq!(result.columns[0].name, "x");
    assert!(!tx.list_schemas().await.unwrap().is_empty());
    assert!(!tx.list_databases().await.unwrap().is_empty());
    tx.rollback().await.unwrap();
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn query_builder_binds_new_variants() {
    let db = setup().await;
    db.insert("children")
        .value("id", 10_i64)
        .value("payload", DatabaseValue::BytesOpt(None))
        .value("weird", DatabaseValue::DateOpt(None))
        .value("price", 1.5_f64)
        .execute(&db)
        .await
        .unwrap();
    db.insert("children")
        .value("id", 11_i64)
        .value("payload", vec![9_u8, 8, 7])
        .value(
            "weird",
            chrono::NaiveTime::from_hms_opt(12, 34, 56).unwrap(),
        )
        .execute(&db)
        .await
        .unwrap();

    let rows = db
        .query_raw("SELECT id, payload, weird, price FROM children ORDER BY id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].get("payload").unwrap().is_null());
    assert!(rows[0].get("weird").unwrap().is_null());
    assert_eq!(rows[0].get("price").unwrap().as_f64(), Some(1.5));
    assert_eq!(
        rows[1].get("payload").unwrap().as_bytes(),
        Some(&[9_u8, 8, 7][..])
    );
    assert_eq!(rows[1].get("weird").unwrap().as_str(), Some("12:34:56"));
}

/// Columns come from `PRAGMA table_xinfo`; regular columns are not generated and
/// `SQLite`-family backends have no enum types. (Turso only accepts generated
/// columns behind its experimental `generated_columns` feature, which
/// `TursoDatabase` does not enable; the `hidden` mapping is unit-tested in
/// `sqlite_common`.)
#[test_log::test(switchy_async::test(no_simulator))]
async fn regular_columns_are_not_generated_or_enums() {
    let db = setup().await;
    let info = db
        .get_relation_info(&RelationRef::new("children"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.columns.len(), 5);
    assert_eq!(info.columns["weird"].ordinal_position, 5);
    assert!(info.columns.values().all(|c| c.generated.is_none()));
    assert!(info.columns.values().all(|c| c.enum_values.is_none()));

    let view = db
        .get_relation_info(&RelationRef::new("parent_names"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.columns.len(), 2);
    assert!(view.columns.values().all(|c| c.generated.is_none()));
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn lists_foreign_keys_in_both_directions() {
    use switchy_database::schema::ForeignKeyScope;
    let db = setup().await;
    let keys = db
        .list_foreign_keys(&ForeignKeyScope::Relation(RelationRef::new("parents")))
        .await
        .unwrap();
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert_eq!(keys[0].table.name, "children");
    assert_eq!(keys[0].foreign_key.columns, vec!["parent_id"]);
    assert_eq!(keys[0].foreign_key.referenced_columns, vec!["id"]);
    assert_eq!(keys[0].foreign_key.on_delete.as_deref(), Some("CASCADE"));
    let referencing = db
        .get_referencing_foreign_keys(&RelationRef::new("parents"))
        .await
        .unwrap();
    assert_eq!(referencing, keys);
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn composite_foreign_keys_report_every_column() {
    use switchy_database::schema::ForeignKeyScope;
    let db = TursoDatabase::new(":memory:").await.unwrap();
    db.exec_raw(
        "CREATE TABLE regions (country TEXT, \"region code\" TEXT, \
         PRIMARY KEY (country, \"region code\"))",
    )
    .await
    .unwrap();
    db.exec_raw(
        "CREATE TABLE offices (id INTEGER PRIMARY KEY, country TEXT, region TEXT, \
         FOREIGN KEY (country, region) REFERENCES regions (country, \"region code\") \
         ON DELETE CASCADE)",
    )
    .await
    .unwrap();

    let info = db
        .get_relation_info(&RelationRef::new("offices"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.foreign_keys.len(), 1, "{:?}", info.foreign_keys);
    let (name, key) = info.foreign_keys.iter().next().unwrap();
    assert_eq!(name, "offices_country_regions_country");
    assert_eq!(key.columns, vec!["country", "region"]);
    assert_eq!(key.referenced_columns, vec!["country", "region code"]);
    assert_eq!(key.column, "country");
    assert_eq!(key.referenced_column, "country");
    assert_eq!(key.on_delete.as_deref(), Some("CASCADE"));

    let referencing = db
        .list_foreign_keys(&ForeignKeyScope::Relation(RelationRef::new("regions")))
        .await
        .unwrap();
    assert_eq!(referencing.len(), 1, "{referencing:?}");
    assert_eq!(referencing[0].foreign_key, *key);
}

#[test_log::test(switchy_async::test(no_simulator))]
async fn statistics_list_tables_without_row_counts_before_analyze() {
    let db = setup().await;
    let statistics = db.list_relation_statistics(None).await.unwrap();
    let mut names: Vec<&str> = statistics.iter().map(|t| t.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["children", "parents"], "views have none");
    let parents = statistics.iter().find(|t| t.name == "parents").unwrap();
    assert_eq!(parents.schema.as_deref(), Some("main"));
    assert_eq!(parents.planner_rows, None);
    assert_eq!(parents.ever_analyzed, Some(false));
    assert!(parents.analyzable);
}
