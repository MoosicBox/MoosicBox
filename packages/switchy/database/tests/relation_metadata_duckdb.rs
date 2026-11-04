#![cfg(all(feature = "duckdb", feature = "schema", feature = "raw-sql"))]
//! `DuckDB` coverage for multi-schema relation introspection, native type
//! names, lossless row decoding and raw result-set metadata.

use std::sync::Arc;

use ::duckdb::Connection;
use switchy_async::sync::Mutex;
use switchy_database::{
    Database, DatabaseValue,
    duckdb::DuckDbDatabase,
    schema::{DataType, GeneratedKind, RelationKind, RelationRef},
};

fn setup() -> DuckDbDatabase {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE SCHEMA app;
         CREATE TYPE mood AS ENUM ('sad', 'happy');
         CREATE TABLE app.items (
             id INTEGER PRIMARY KEY,
             m mood,
             ts TIMESTAMPTZ,
             arr INTEGER[],
             d DECIMAL(10,2),
             s STRUCT(a INTEGER, b VARCHAR),
             u UUID,
             bl BLOB,
             iv INTERVAL,
             dt DATE,
             tm TIME
         );
         COMMENT ON TABLE app.items IS 'item table';
         CREATE VIEW app.item_moods AS SELECT id, m FROM app.items;
         CREATE TABLE app.children (id INTEGER, item_id INTEGER REFERENCES app.items(id));
         CREATE INDEX children_item_idx ON app.children(item_id);
         CREATE TABLE plain (id INTEGER);",
    )
    .unwrap();
    DuckDbDatabase::new(vec![Arc::new(Mutex::new(conn))])
}

#[test_log::test(switchy_async::test)]
async fn lists_schemas_with_default_and_system_flags() {
    let db = setup();
    let schemas = db.list_schemas().await.unwrap();
    let main = schemas.iter().find(|s| s.name == "main").unwrap();
    assert!(main.is_default);
    assert!(!main.is_system);
    let app = schemas.iter().find(|s| s.name == "app").unwrap();
    assert!(!app.is_default);
    assert!(!app.is_system);
    assert!(
        schemas
            .iter()
            .any(|s| s.name == "information_schema" && s.is_system)
    );
}

#[test_log::test(switchy_async::test)]
async fn lists_databases_with_current_and_system_flags() {
    let db = setup();
    let databases = db.list_databases().await.unwrap();
    let current = databases
        .iter()
        .filter(|d| d.is_current)
        .collect::<Vec<_>>();
    assert_eq!(current.len(), 1);
    assert!(!current[0].is_system);
    assert!(databases.iter().any(|d| d.name == "system" && d.is_system));
    assert!(databases.iter().any(|d| d.name == "temp" && d.is_system));

    let tx = db.begin_transaction().await.unwrap();
    assert_eq!(tx.list_databases().await.unwrap(), databases);
    tx.rollback().await.unwrap();
}

#[test_log::test(switchy_async::test)]
async fn lists_relations_in_default_and_named_schema() {
    let db = setup();
    let default = db.list_relations(None).await.unwrap();
    assert_eq!(
        default.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["plain"]
    );
    assert_eq!(default[0].schema.as_deref(), Some("main"));

    let app = db.list_relations(Some("app")).await.unwrap();
    let items = app.iter().find(|r| r.name == "items").unwrap();
    assert_eq!(items.kind, RelationKind::Table);
    assert_eq!(items.comment.as_deref(), Some("item table"));
    assert!(items.estimated_rows.is_some());
    let view = app.iter().find(|r| r.name == "item_moods").unwrap();
    assert_eq!(view.kind, RelationKind::View);
    assert_eq!(view.estimated_rows, None);

    // Legacy table APIs keep using the default schema
    assert_eq!(db.list_tables().await.unwrap(), vec!["plain".to_string()]);
    assert!(!db.table_exists("items").await.unwrap());
    assert!(db.get_table_info("items").await.unwrap().is_none());
}

#[test_log::test(switchy_async::test)]
async fn relation_info_reports_enum_values_and_generated_columns() {
    let db = setup();
    db.exec_raw(
        "CREATE TYPE app.color AS ENUM ('red', 'it''s, green');
         CREATE TABLE app.extras (
             \"we, ird\" INTEGER DEFAULT 3,
             c app.color,
             inline_enum ENUM('x', 'y z'),
             moods mood[],
             label VARCHAR DEFAULT 'GENERATED ALWAYS AS',
             doubled INTEGER GENERATED ALWAYS AS (\"we, ird\" * 2) VIRTUAL,
             \"Short\" AS (\"we, ird\" + 1)
         );",
    )
    .await
    .unwrap();

    let info = db
        .get_relation_info(&RelationRef::with_schema("app", "extras"))
        .await
        .unwrap()
        .unwrap();
    let col = |name: &str| info.columns.get(name).unwrap();
    let labels = |values: &[&str]| Some(values.iter().map(ToString::to_string).collect());

    assert_eq!(col("c").enum_values, labels(&["red", "it's, green"]));
    assert_eq!(col("inline_enum").enum_values, labels(&["x", "y z"]));
    assert_eq!(col("moods").enum_values, None);
    assert_eq!(col("we, ird").enum_values, None);

    assert_eq!(col("we, ird").generated, None);
    assert_eq!(
        col("we, ird").default_value,
        Some(DatabaseValue::String("3".to_string()))
    );
    assert_eq!(col("label").generated, None);
    assert_eq!(col("doubled").generated, Some(GeneratedKind::Virtual));
    assert_eq!(col("doubled").default_value, None);
    assert_eq!(col("Short").generated, Some(GeneratedKind::Virtual));

    let items = db
        .get_relation_info(&RelationRef::with_schema("app", "items"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        items.columns.get("m").unwrap().enum_values,
        labels(&["sad", "happy"])
    );
    assert!(items.columns.values().all(|c| c.generated.is_none()));

    let view = db
        .get_relation_info(&RelationRef::with_schema("app", "item_moods"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        view.columns.get("m").unwrap().enum_values,
        labels(&["sad", "happy"])
    );
}

#[test_log::test(switchy_async::test)]
async fn relation_info_reports_native_types_indexes_and_foreign_keys() {
    let db = setup();
    let info = db
        .get_relation_info(&RelationRef::with_schema("app", "items"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.schema.as_deref(), Some("app"));
    assert_eq!(info.kind, RelationKind::Table);
    assert_eq!(info.comment.as_deref(), Some("item table"));

    let col = |name: &str| info.columns.get(name).unwrap();
    assert!(col("id").is_primary_key);
    assert_eq!(col("ts").native_type, "TIMESTAMP WITH TIME ZONE");
    assert_eq!(col("ts").data_type, DataType::Timestamp);
    assert_eq!(col("arr").native_type, "INTEGER[]");
    assert_eq!(
        col("arr").data_type,
        DataType::Array(Box::new(DataType::Int))
    );
    assert_eq!(col("d").data_type, DataType::Decimal(10, 2));
    assert_eq!(col("u").data_type, DataType::Uuid);
    assert_eq!(col("bl").data_type, DataType::Blob);
    assert_eq!(col("dt").data_type, DataType::Date);
    assert_eq!(col("tm").data_type, DataType::Time);
    assert!(col("m").native_type.starts_with("ENUM("));
    assert_eq!(
        col("m").data_type,
        DataType::Custom(col("m").native_type.clone())
    );
    assert!(matches!(col("s").data_type, DataType::Custom(_)));
    assert!(matches!(col("iv").data_type, DataType::Custom(_)));

    let children = db
        .get_relation_info(&RelationRef::with_schema("app", "children"))
        .await
        .unwrap()
        .unwrap();
    let index = children.indexes.get("children_item_idx").unwrap();
    assert_eq!(index.columns, vec!["item_id".to_string()]);
    let fk = children.foreign_keys.values().next().unwrap();
    assert_eq!(fk.column, "item_id");
    assert_eq!(fk.referenced_table, "items");
    assert_eq!(fk.referenced_column, "id");

    let view = db
        .get_relation_info(&RelationRef::with_schema("app", "item_moods"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.kind, RelationKind::View);
    assert_eq!(view.columns.len(), 2);
    assert!(view.indexes.is_empty());

    assert!(
        db.get_relation_info(&RelationRef::with_schema("app", "missing"))
            .await
            .unwrap()
            .is_none()
    );
}

#[test_log::test(switchy_async::test)]
async fn view_definition_is_returned_for_views_only() {
    let db = setup();
    let definition = db
        .get_view_definition(&RelationRef::with_schema("app", "item_moods"))
        .await
        .unwrap()
        .unwrap();
    assert!(definition.to_uppercase().contains("SELECT"));
    assert!(
        db.get_view_definition(&RelationRef::with_schema("app", "items"))
            .await
            .unwrap()
            .is_none()
    );
}

#[test_log::test(switchy_async::test)]
async fn decodes_rich_types_without_errors() {
    let db = setup();
    db.exec_raw(
        "INSERT INTO app.items VALUES (1, 'happy', '2024-01-02 03:04:05+00', [1, 2], 12.34, \
         {'a': 1, 'b': 'x'}, 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', '\\x01\\x02'::BLOB, \
         INTERVAL 1 MONTH + INTERVAL 3 HOUR, '2024-05-06', '07:08:09')",
    )
    .await
    .unwrap();
    let result = db
        .query_raw_result(
            "SELECT m, ts, arr, d, s, bl, iv, dt, tm FROM app.items",
            &[],
        )
        .await
        .unwrap();
    let row = &result.rows[0];
    assert_eq!(
        row[0],
        DatabaseValue::Other {
            type_name: "ENUM".to_string(),
            text: "happy".to_string()
        }
    );
    assert_eq!(
        row[1].as_datetime().unwrap().to_string(),
        "2024-01-02 03:04:05"
    );
    assert_eq!(
        row[2],
        DatabaseValue::Other {
            type_name: "INTEGER[]".to_string(),
            text: "[1, 2]".to_string()
        }
    );
    #[cfg(feature = "decimal")]
    assert_eq!(row[3].as_decimal().unwrap().to_string(), "12.34");
    assert_eq!(row[4].as_other().unwrap().1, "{'a': 1, 'b': x}");
    assert_eq!(row[5].as_bytes().unwrap(), &[1_u8, 2]);
    assert_eq!(row[6].as_other().unwrap(), ("INTERVAL", "1 month 03:00:00"));
    assert_eq!(row[7].as_date().unwrap().to_string(), "2024-05-06");
    assert_eq!(row[8].as_time().unwrap().to_string(), "07:08:09");
}

#[test_log::test(switchy_async::test)]
async fn raw_result_reports_columns_for_empty_results_and_duplicates() {
    let db = setup();
    let result = db
        .query_raw_result(
            "SELECT id, id, d, ts FROM app.items WHERE id = ?",
            &[DatabaseValue::Int32(42)],
        )
        .await
        .unwrap();
    assert!(result.rows.is_empty());
    let names: Vec<_> = result.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "id", "d", "ts"]);
    assert_eq!(result.columns[0].type_name.as_deref(), Some("INTEGER"));
    assert_eq!(
        result.columns[2].type_name.as_deref(),
        Some("DECIMAL(10,2)")
    );
    assert_eq!(
        result.columns[3].type_name.as_deref(),
        Some("TIMESTAMP WITH TIME ZONE")
    );

    let tx = db.begin_transaction().await.unwrap();
    let result = tx
        .query_raw_result("SELECT 1 AS a, 'x' AS a", &[])
        .await
        .unwrap();
    assert_eq!(result.columns.len(), 2);
    assert_eq!(
        result.rows,
        vec![vec![
            DatabaseValue::Int32(1),
            DatabaseValue::String("x".to_string())
        ]]
    );
    let relations = tx.list_relations(Some("app")).await.unwrap();
    assert!(relations.iter().any(|r| r.name == "items"));
    tx.commit().await.unwrap();
}

#[test_log::test(switchy_async::test)]
async fn binds_new_value_variants() {
    let db = setup();
    db.exec_raw("CREATE TABLE typed (b BLOB, d DATE, t TIME, tz TIMESTAMPTZ, s VARCHAR)")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2024, 2, 3).unwrap();
    let time = chrono::NaiveTime::from_hms_opt(4, 5, 6).unwrap();
    let tz = chrono::DateTime::parse_from_rfc3339("2024-02-03T04:05:06+02:00").unwrap();
    db.exec_raw_params(
        "INSERT INTO typed VALUES (?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Bytes(vec![0, 255]),
            DatabaseValue::Date(date),
            DatabaseValue::Time(time),
            DatabaseValue::DateTimeTz(tz),
            DatabaseValue::BytesOpt(None),
        ],
    )
    .await
    .unwrap();
    let rows = db
        .query_raw_params("SELECT b, d, t, tz, s FROM typed", &[])
        .await
        .unwrap();
    let row = &rows[0];
    assert_eq!(row.get("b").unwrap().as_bytes().unwrap(), &[0_u8, 255]);
    assert_eq!(row.get("d").unwrap().as_date(), Some(date));
    assert_eq!(row.get("t").unwrap().as_time(), Some(time));
    assert_eq!(row.get("tz").unwrap().as_datetime(), Some(tz.naive_utc()));
    assert!(row.get("s").unwrap().is_null());
}

#[test_log::test(switchy_async::test)]
async fn query_builder_keeps_placeholders_aligned_for_new_null_variants() {
    let db = setup();
    db.exec_raw("CREATE TABLE nulls (b BLOB, d DATE, s VARCHAR)")
        .await
        .unwrap();
    db.insert("nulls")
        .value("b", DatabaseValue::BytesOpt(None))
        .value("d", DatabaseValue::DateOpt(None))
        .value("s", "after")
        .execute(&db)
        .await
        .unwrap();
    let rows = db
        .query_raw_params("SELECT b, d, s FROM nulls", &[])
        .await
        .unwrap();
    assert!(rows[0].get("b").unwrap().is_null());
    assert!(rows[0].get("d").unwrap().is_null());
    assert_eq!(rows[0].get("s").unwrap().as_str(), Some("after"));
}

#[test_log::test(switchy_async::test)]
async fn lists_builtin_and_user_routines() {
    use switchy_database::schema::RoutineKind;
    let db = setup();
    db.exec_raw("CREATE MACRO app.add_one(x) AS x + 1")
        .await
        .unwrap();
    let all = db.list_routines(None).await.unwrap();
    let lower = all.iter().find(|r| r.name == "lower").unwrap();
    assert!(lower.builtin);
    assert!(
        all.iter()
            .any(|r| r.name == "sum" && r.kind == RoutineKind::Aggregate)
    );
    let app = db.list_routines(Some("app")).await.unwrap();
    let add_one = app.iter().find(|r| r.name == "add_one").unwrap();
    assert!(!add_one.builtin);
    assert_eq!(add_one.schema.as_deref(), Some("app"));
    assert_eq!(add_one.arguments.as_deref(), Some("x"));
}
