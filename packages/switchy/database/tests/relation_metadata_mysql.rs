//! `MySQL`/`MariaDB` relation introspection, lossless decoding and result metadata tests.
//!
//! Requires `MYSQL_TEST_URL` (e.g. `mysql://user:pass@127.0.0.1:3306/testdb`); tests
//! are skipped when it is unset. The user must be able to create a second database
//! named `<db>_relmeta_other`.

#![cfg(all(feature = "mysql-sqlx", feature = "schema", feature = "raw-sql"))]

use std::sync::Arc;

use chrono::{NaiveDate, NaiveTime, TimeZone as _};
use sqlx::MySqlPool;
use switchy_async::sync::Mutex;
use switchy_database::{
    Database, DatabaseValue,
    query::FilterableQuery as _,
    schema::{DataType, RelationKind, RelationRef},
    sqlx::mysql::MySqlSqlxDatabase,
};

async fn get_database() -> Option<MySqlSqlxDatabase> {
    let url = std::env::var("MYSQL_TEST_URL").ok()?;
    let pool = MySqlPool::connect(&url).await.ok()?;
    Some(MySqlSqlxDatabase::new(Arc::new(Mutex::new(pool))))
}

async fn current_database(db: &MySqlSqlxDatabase) -> String {
    db.query_raw("SELECT CAST(DATABASE() AS CHAR) AS db")
        .await
        .unwrap()[0]
        .get("db")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

#[test_log::test(tokio::test)]
async fn mysql_lists_schemas_relations_and_views_across_databases() {
    let Some(db) = get_database().await else {
        return;
    };
    let current = current_database(&db).await;
    let other = format!("{current}_relmeta_other");

    db.exec_raw("DROP VIEW IF EXISTS relmeta_users_v")
        .await
        .unwrap();
    db.exec_raw("DROP TABLE IF EXISTS relmeta_users")
        .await
        .unwrap();
    db.exec_raw(&format!("DROP DATABASE IF EXISTS `{other}`"))
        .await
        .unwrap();
    db.exec_raw(
        "CREATE TABLE relmeta_users (
            id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(64) NOT NULL,
            mood ENUM('happy','sad') NULL,
            flags INT(10) UNSIGNED NULL
        ) COMMENT = 'application users'",
    )
    .await
    .unwrap();
    db.exec_raw("CREATE VIEW relmeta_users_v AS SELECT id, name FROM relmeta_users")
        .await
        .unwrap();
    db.exec_raw(&format!("CREATE DATABASE `{other}`"))
        .await
        .unwrap();
    db.exec_raw(&format!(
        "CREATE TABLE `{other}`.parents (id INT NOT NULL PRIMARY KEY) ENGINE=InnoDB"
    ))
    .await
    .unwrap();
    db.exec_raw(&format!(
        "CREATE TABLE `{other}`.children (
            id INT NOT NULL PRIMARY KEY,
            parent_id INT NOT NULL,
            payload LONGBLOB NULL,
            INDEX children_parent_idx (parent_id),
            CONSTRAINT children_parent_fk FOREIGN KEY (parent_id)
                REFERENCES `{other}`.parents (id) ON DELETE CASCADE
        ) ENGINE=InnoDB"
    ))
    .await
    .unwrap();

    // Schemas
    let schemas = db.list_schemas().await.unwrap();
    let current_info = schemas.iter().find(|s| s.name == current).unwrap();
    assert!(current_info.is_default);
    assert!(!current_info.is_system);
    let other_info = schemas.iter().find(|s| s.name == other).unwrap();
    assert!(!other_info.is_default);
    assert!(
        schemas
            .iter()
            .find(|s| s.name == "information_schema")
            .unwrap()
            .is_system
    );

    // Relations in the default schema
    let relations = db.list_relations(None).await.unwrap();
    let users = relations
        .iter()
        .find(|r| r.name == "relmeta_users")
        .unwrap();
    assert_eq!(users.kind, RelationKind::Table);
    assert_eq!(users.schema.as_deref(), Some(current.as_str()));
    assert_eq!(users.comment.as_deref(), Some("application users"));
    assert!(users.estimated_rows.is_some());
    let view = relations
        .iter()
        .find(|r| r.name == "relmeta_users_v")
        .unwrap();
    assert_eq!(view.kind, RelationKind::View);
    assert_eq!(view.comment, None);
    assert_eq!(view.estimated_rows, None);

    // Relations in another schema
    let other_relations = db.list_relations(Some(&other)).await.unwrap();
    let names: Vec<_> = other_relations.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["children", "parents"]);
    assert!(
        other_relations
            .iter()
            .all(|r| r.schema.as_deref() == Some(other.as_str()))
    );

    // System schema relations
    let system = db.list_relations(Some("information_schema")).await.unwrap();
    assert!(
        system
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("TABLES") && r.kind == RelationKind::SystemTable)
    );

    // Table info in the default schema (compat path)
    let info = db.get_table_info("relmeta_users").await.unwrap().unwrap();
    assert_eq!(info.schema.as_deref(), Some(current.as_str()));
    assert_eq!(info.kind, RelationKind::Table);
    assert_eq!(info.comment.as_deref(), Some("application users"));
    let mood = &info.columns["mood"];
    assert_eq!(mood.native_type, "enum('happy','sad')");
    assert_eq!(
        mood.data_type,
        DataType::Custom("enum('happy','sad')".to_string())
    );
    assert!(info.columns["flags"].native_type.contains("unsigned"));
    assert_eq!(info.columns["name"].data_type, DataType::VarChar(64));
    assert!(info.indexes.contains_key("PRIMARY"));

    // View info
    let view_info = db
        .get_relation_info(&RelationRef::new("relmeta_users_v"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view_info.kind, RelationKind::View);
    assert_eq!(view_info.columns.len(), 2);
    assert!(view_info.indexes.is_empty());

    // Table info in another schema with indexes and foreign keys
    let children = db
        .get_relation_info(&RelationRef::with_schema(other.clone(), "children"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(children.schema.as_deref(), Some(other.as_str()));
    assert_eq!(children.columns["payload"].data_type, DataType::Blob);
    assert!(children.indexes.contains_key("children_parent_idx"));
    let fk = &children.foreign_keys["children_parent_fk"];
    assert_eq!(fk.referenced_table, "parents");
    assert_eq!(fk.on_delete.as_deref(), Some("CASCADE"));

    // Missing relations
    assert!(
        db.get_relation_info(&RelationRef::with_schema(other.clone(), "missing"))
            .await
            .unwrap()
            .is_none()
    );

    // View definitions
    let definition = db
        .get_view_definition(&RelationRef::new("relmeta_users_v"))
        .await
        .unwrap()
        .unwrap();
    assert!(definition.contains("relmeta_users"), "{definition}");
    assert!(
        db.get_view_definition(&RelationRef::new("relmeta_users"))
            .await
            .unwrap()
            .is_none()
    );

    // Transactions use the same implementation
    let tx = db.begin_transaction().await.unwrap();
    assert!(
        tx.list_relations(Some(&other))
            .await
            .unwrap()
            .iter()
            .any(|r| r.name == "parents")
    );
    assert!(
        tx.get_relation_info(&RelationRef::with_schema(other.clone(), "parents"))
            .await
            .unwrap()
            .is_some()
    );
    tx.rollback().await.unwrap();

    db.exec_raw("DROP VIEW relmeta_users_v").await.unwrap();
    db.exec_raw("DROP TABLE relmeta_users").await.unwrap();
    db.exec_raw(&format!("DROP DATABASE `{other}`"))
        .await
        .unwrap();
}

#[test_log::test(tokio::test)]
async fn mysql_decodes_every_column_type_without_errors() {
    let Some(db) = get_database().await else {
        return;
    };

    db.exec_raw("DROP TABLE IF EXISTS relmeta_types")
        .await
        .unwrap();
    db.exec_raw(
        "CREATE TABLE relmeta_types (
            id INT NOT NULL PRIMARY KEY,
            d DATE NULL,
            t TIME NULL,
            t_neg TIME NULL,
            dt DATETIME NULL,
            b BLOB NULL,
            vb VARBINARY(8) NULL,
            u8 TINYINT UNSIGNED NULL,
            u64 BIGINT UNSIGNED NULL,
            y YEAR NULL,
            bits BIT(8) NULL,
            e ENUM('a','b') NULL,
            s SET('x','y') NULL,
            dec_col DECIMAL(10,2) NULL,
            j JSON NULL,
            bin_text VARCHAR(8) COLLATE utf8mb4_bin NULL,
            g GEOMETRY NULL
        )",
    )
    .await
    .unwrap();
    db.exec_raw(
        "INSERT INTO relmeta_types VALUES (
            1, '2024-02-29', '13:14:15', '-838:59:59', '2024-02-29 13:14:15',
            X'00FF10', X'0102', 255, 18446744073709551615, 2024, b'10100101',
            'b', 'x,y', 12.34, '{\"k\": [1, 2]}', 'Case', ST_GeomFromText('POINT(1 1)')
        )",
    )
    .await
    .unwrap();
    db.exec_raw("INSERT INTO relmeta_types (id) VALUES (2)")
        .await
        .unwrap();

    let check = |rows: Vec<switchy_database::Row>| {
        let row = &rows[0];
        assert_eq!(
            row.get("d").unwrap(),
            DatabaseValue::Date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
        );
        assert_eq!(
            row.get("t").unwrap(),
            DatabaseValue::Time(NaiveTime::from_hms_opt(13, 14, 15).unwrap())
        );
        assert_eq!(
            row.get("t_neg").unwrap(),
            DatabaseValue::Other {
                type_name: "TIME".to_string(),
                text: "-838:59:59".to_string(),
            }
        );
        assert!(matches!(row.get("dt").unwrap(), DatabaseValue::DateTime(_)));
        assert_eq!(
            row.get("b").unwrap(),
            DatabaseValue::Bytes(vec![0, 255, 16])
        );
        assert_eq!(row.get("vb").unwrap(), DatabaseValue::Bytes(vec![1, 2]));
        assert_eq!(row.get("u8").unwrap(), DatabaseValue::UInt8(255));
        assert_eq!(row.get("u64").unwrap(), DatabaseValue::UInt64(u64::MAX));
        assert_eq!(row.get("y").unwrap(), DatabaseValue::UInt16(2024));
        assert_eq!(row.get("bits").unwrap(), DatabaseValue::UInt64(0b1010_0101));
        assert_eq!(
            row.get("e").unwrap(),
            DatabaseValue::String("b".to_string())
        );
        assert_eq!(
            row.get("s").unwrap(),
            DatabaseValue::String("x,y".to_string())
        );
        match row.get("dec_col").unwrap() {
            #[cfg(feature = "decimal")]
            DatabaseValue::Decimal(value) => assert_eq!(value.to_string(), "12.34"),
            DatabaseValue::Other { text, .. } => assert_eq!(text, "12.34"),
            other => panic!("unexpected decimal value {other:?}"),
        }
        // MariaDB reports JSON columns as LONGTEXT
        match row.get("j").unwrap() {
            #[cfg(feature = "json")]
            DatabaseValue::Json(value) => assert_eq!(value, serde_json::json!({"k": [1, 2]})),
            DatabaseValue::String(text) => assert!(text.contains("\"k\"")),
            other => panic!("unexpected json value {other:?}"),
        }
        assert_eq!(
            row.get("bin_text").unwrap(),
            DatabaseValue::String("Case".to_string())
        );
        match row.get("g").unwrap() {
            DatabaseValue::Other { type_name, text } => {
                assert_eq!(type_name, "GEOMETRY");
                assert!(text.starts_with("0x"), "{text}");
            }
            DatabaseValue::Bytes(_) => {}
            other => panic!("unexpected geometry value {other:?}"),
        }
        assert!(rows[1].columns.iter().skip(1).all(|(_, v)| v.is_null()));
    };

    // Text protocol (unprepared) and binary protocol (prepared)
    check(
        db.query_raw("SELECT * FROM relmeta_types ORDER BY id")
            .await
            .unwrap(),
    );
    check(
        db.query_raw_params(
            "SELECT * FROM relmeta_types WHERE id >= ? ORDER BY id",
            &[DatabaseValue::Int32(0)],
        )
        .await
        .unwrap(),
    );
    let tx = db.begin_transaction().await.unwrap();
    check(
        tx.query_raw_params(
            "SELECT * FROM relmeta_types WHERE id >= ? ORDER BY id",
            &[DatabaseValue::Int32(0)],
        )
        .await
        .unwrap(),
    );
    tx.rollback().await.unwrap();

    db.exec_raw("DROP TABLE relmeta_types").await.unwrap();
}

#[test_log::test(tokio::test)]
async fn mysql_binds_new_value_variants() {
    let Some(db) = get_database().await else {
        return;
    };

    db.exec_raw("DROP TABLE IF EXISTS relmeta_binds")
        .await
        .unwrap();
    db.exec_raw(
        "CREATE TABLE relmeta_binds (
            id INT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            b BLOB NULL,
            d DATE NULL,
            t TIME NULL,
            dt DATETIME NULL,
            txt TEXT NULL
        )",
    )
    .await
    .unwrap();

    let date = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
    let time = NaiveTime::from_hms_opt(23, 59, 58).unwrap();
    let offset = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
    let with_tz = offset.with_ymd_and_hms(2023, 12, 31, 12, 0, 0).unwrap();

    db.exec_raw_params(
        "INSERT INTO relmeta_binds VALUES (?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int32(1),
            DatabaseValue::Bytes(vec![9, 8, 7]),
            DatabaseValue::Date(date),
            DatabaseValue::Time(time),
            DatabaseValue::DateTimeTz(with_tz),
            DatabaseValue::Other {
                type_name: "custom".to_string(),
                text: "hello".to_string(),
            },
        ],
    )
    .await
    .unwrap();
    db.exec_raw_params(
        "INSERT INTO relmeta_binds VALUES (?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int32(2),
            DatabaseValue::BytesOpt(None),
            DatabaseValue::DateOpt(None),
            DatabaseValue::TimeOpt(None),
            DatabaseValue::DateTimeTzOpt(None),
            DatabaseValue::StringOpt(None),
        ],
    )
    .await
    .unwrap();

    // Query builder path
    db.insert("relmeta_binds")
        .value("b", vec![1_u8, 2, 3])
        .value("d", date)
        .value("t", DatabaseValue::TimeOpt(None))
        .execute(&db)
        .await
        .unwrap();

    // Null-like new variants before bound values keep placeholders aligned
    db.insert("relmeta_binds")
        .value("b", DatabaseValue::BytesOpt(None))
        .value("d", DatabaseValue::DateOpt(None))
        .value("dt", DatabaseValue::DateTimeTzOpt(None))
        .value("t", time)
        .value("txt", "after-nulls")
        .execute(&db)
        .await
        .unwrap();
    db.update("relmeta_binds")
        .value("b", DatabaseValue::BytesOpt(None))
        .value("txt", "updated")
        .where_eq("id", 3)
        .execute(&db)
        .await
        .unwrap();

    let rows = db
        .query_raw("SELECT * FROM relmeta_binds ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        rows[0].get("b").unwrap(),
        DatabaseValue::Bytes(vec![9, 8, 7])
    );
    assert_eq!(rows[0].get("d").unwrap(), DatabaseValue::Date(date));
    assert_eq!(rows[0].get("t").unwrap(), DatabaseValue::Time(time));
    // Offset-aware values are stored in UTC
    assert_eq!(
        rows[0].get("dt").unwrap(),
        DatabaseValue::DateTime(with_tz.naive_utc())
    );
    assert_eq!(
        rows[0].get("txt").unwrap(),
        DatabaseValue::String("hello".to_string())
    );
    assert!(rows[1].columns.iter().skip(1).all(|(_, v)| v.is_null()));
    assert_eq!(rows[2].get("d").unwrap(), DatabaseValue::Date(date));
    assert!(rows[2].get("t").unwrap().is_null());
    assert!(rows[2].get("b").unwrap().is_null());
    assert_eq!(
        rows[2].get("txt").unwrap(),
        DatabaseValue::String("updated".to_string())
    );
    assert!(rows[3].get("b").unwrap().is_null());
    assert!(rows[3].get("d").unwrap().is_null());
    assert!(rows[3].get("dt").unwrap().is_null());
    assert_eq!(rows[3].get("t").unwrap(), DatabaseValue::Time(time));
    assert_eq!(
        rows[3].get("txt").unwrap(),
        DatabaseValue::String("after-nulls".to_string())
    );

    db.exec_raw("DROP TABLE relmeta_binds").await.unwrap();
}

#[test_log::test(tokio::test)]
async fn mysql_query_raw_result_reports_columns_positionally() {
    let Some(db) = get_database().await else {
        return;
    };

    db.exec_raw("DROP TABLE IF EXISTS relmeta_result")
        .await
        .unwrap();
    db.exec_raw("CREATE TABLE relmeta_result (id INT NOT NULL PRIMARY KEY, label VARCHAR(10))")
        .await
        .unwrap();

    // Zero rows still report columns and native type names
    let empty = db
        .query_raw_result(
            "SELECT id, label FROM relmeta_result WHERE id > ?",
            &[DatabaseValue::Int32(0)],
        )
        .await
        .unwrap();
    assert!(empty.rows.is_empty());
    let names: Vec<_> = empty.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "label"]);
    assert_eq!(empty.columns[0].type_name.as_deref(), Some("INT"));
    assert_eq!(empty.columns[1].type_name.as_deref(), Some("VARCHAR"));

    // Duplicate names and order are preserved
    let dup = db
        .query_raw_result("SELECT 1 AS a, 'x' AS a, NOW() AS n", &[])
        .await
        .unwrap();
    let names: Vec<_> = dup.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a", "a", "n"]);
    assert_eq!(dup.rows.len(), 1);
    assert_eq!(dup.rows[0][1], DatabaseValue::String("x".to_string()));
    assert!(matches!(
        dup.rows[0][0],
        DatabaseValue::Int32(1) | DatabaseValue::Int64(1)
    ));

    // Transactions
    let tx = db.begin_transaction().await.unwrap();
    tx.exec_raw_params(
        "INSERT INTO relmeta_result VALUES (?, ?)",
        &[
            DatabaseValue::Int32(5),
            DatabaseValue::String("five".to_string()),
        ],
    )
    .await
    .unwrap();
    let in_tx = tx
        .query_raw_result("SELECT label, id FROM relmeta_result", &[])
        .await
        .unwrap();
    assert_eq!(
        in_tx.rows,
        vec![vec![
            DatabaseValue::String("five".to_string()),
            DatabaseValue::Int32(5)
        ]]
    );
    tx.rollback().await.unwrap();

    db.exec_raw("DROP TABLE relmeta_result").await.unwrap();
}
