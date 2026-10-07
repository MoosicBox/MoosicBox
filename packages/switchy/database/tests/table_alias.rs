#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use switchy_database::{
    Database,
    query::{FilterableQuery, SortDirection, qualified_column, select},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType, create_index},
};

#[tokio::test]
async fn correlated_latest_per_node_aliases_preserve_bounds_and_order() {
    let db = RusqliteDatabase::new(vec![std::sync::Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("activations")
        .column(Column {
            name: "node".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "generation".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    create_index("node_generation")
        .table("activations")
        .columns(vec!["node", "generation"])
        .execute(&db)
        .await
        .unwrap();
    let tx = db.begin_transaction().await.unwrap();
    for node in 0..8 {
        for generation in 0..512 {
            tx.insert("activations")
                .value("node", node)
                .value("generation", generation)
                .execute(tx.as_ref())
                .await
                .unwrap();
        }
    }
    tx.commit().await.unwrap();
    for outer in ["a", "a\"`; --"] {
        let newer = select("activations")
            .table_alias("newer")
            .columns(&["node"])
            .where_eq_expression(
                qualified_column("newer", "node"),
                qualified_column(outer, "node"),
            )
            .where_gt("generation", qualified_column(outer, "generation"));
        let rows = db
            .select("activations")
            .table_alias(outer)
            .where_not_in("node", newer)
            .sort("node", SortDirection::Asc)
            .limit(3)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        for (node, row) in rows.iter().enumerate() {
            assert_eq!(
                row.get("node"),
                Some(switchy_database::DatabaseValue::Int64(
                    i64::try_from(node).unwrap()
                ))
            );
            assert_eq!(
                row.get("generation"),
                Some(switchy_database::DatabaseValue::Int64(511))
            );
        }
    }
    db.close().await.unwrap();
}
