#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
use switchy_database::{
    Database, DatabaseValue,
    query::{FilterableQuery, JsonPath, JsonType, SortDirection, identifier, json_type_is},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn json_integer_filter_precedes_order_limit_and_rejects_malformed_json() {
    let db = RusqliteDatabase::new(vec![std::sync::Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("outputs")
        .column(Column {
            name: "sequence".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "value_json".into(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    for (sequence, json) in [
        (0, "{\"iterations_completed\":1.0}"),
        (1, "{\"iterations_completed\":\"1\"}"),
        (2, "{\"iterations_completed\":null}"),
        (3, "{}"),
        (4, "{\"iterations_completed\":true}"),
        (5, "{\"iterations_completed\":2}"),
        (6, "{\"iterations_completed\":3}"),
    ] {
        db.insert("outputs")
            .value("sequence", sequence)
            .value("value_json", json)
            .execute(&db)
            .await
            .unwrap();
    }
    let rows = db
        .select("outputs")
        .filter(Box::new(json_type_is(
            identifier("value_json"),
            JsonPath::member("iterations_completed").unwrap(),
            JsonType::Integer,
        )))
        .sort("sequence", SortDirection::Asc)
        .limit(1)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("sequence"), Some(DatabaseValue::Int64(5)));
    // A bound JSON operand traverses its parameter exactly once.
    let rows = db
        .select("outputs")
        .filter(Box::new(json_type_is(
            DatabaseValue::String("{\"iterations_completed\":2}".into()),
            JsonPath::member("iterations_completed").unwrap(),
            JsonType::Integer,
        )))
        .limit(1)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    for path in [
        "",
        "iterations_completed'); DROP TABLE outputs; --",
        "a.b",
        "a[0]",
        "a\"b",
    ] {
        assert!(JsonPath::member(path).is_err());
    }
    db.insert("outputs")
        .value("sequence", -1)
        .value("value_json", "{malformed")
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.select("outputs")
            .filter(Box::new(json_type_is(
                identifier("value_json"),
                JsonPath::member("iterations_completed").unwrap(),
                JsonType::Integer
            )))
            .sort("sequence", SortDirection::Asc)
            .limit(1)
            .execute(&db)
            .await
            .is_err()
    );
    db.close().await.unwrap();
}
