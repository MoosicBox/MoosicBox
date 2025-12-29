#![cfg(feature = "schema")]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::{FilterableQuery as _, SortDirection},
    schema::{Column, DataType, create_table, drop_table},
};

#[allow(unused)]
pub trait DataTypeTestSuite {
    type DatabaseType: Database + Send + Sync;

    async fn get_database(&self) -> Option<Arc<Self::DatabaseType>>;

    fn get_table_name(&self, test_suffix: &str) -> String {
        format!("data_type_test_{test_suffix}")
    }

    async fn test_integer_types_boundary_values(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("int_boundary_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "small_int_col".to_string(),
                data_type: DataType::SmallInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "int_col".to_string(),
                data_type: DataType::Int,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "big_int_col".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create int_boundary_test table");

        let small_min = i16::MIN;
        let small_max = i16::MAX;

        db.insert(table_name)
            .value("small_int_col", small_min)
            .value("int_col", 0)
            .value("big_int_col", 0i64)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("small_int_col", small_max)
            .value("int_col", 0)
            .value("big_int_col", 0i64)
            .execute(&*db)
            .await
            .unwrap();

        let int_min = i32::MIN;
        let int_max = i32::MAX;

        db.insert(table_name)
            .value("small_int_col", 0i16)
            .value("int_col", int_min)
            .value("big_int_col", 0i64)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("small_int_col", 0i16)
            .value("int_col", int_max)
            .value("big_int_col", 0i64)
            .execute(&*db)
            .await
            .unwrap();

        let big_min = i64::MIN;
        let big_max = i64::MAX;

        db.insert(table_name)
            .value("small_int_col", 0i16)
            .value("int_col", 0)
            .value("big_int_col", big_min)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("small_int_col", 0i16)
            .value("int_col", 0)
            .value("big_int_col", big_max)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 6);

        assert_eq!(
            rows[0].get("small_int_col").unwrap().as_i64().unwrap(),
            i64::from(small_min)
        );
        assert_eq!(
            rows[1].get("small_int_col").unwrap().as_i64().unwrap(),
            i64::from(small_max)
        );
        assert_eq!(
            rows[2].get("int_col").unwrap().as_i64().unwrap(),
            i64::from(int_min)
        );
        assert_eq!(
            rows[3].get("int_col").unwrap().as_i64().unwrap(),
            i64::from(int_max)
        );
        assert_eq!(
            rows[4].get("big_int_col").unwrap().as_i64().unwrap(),
            big_min
        );
        assert_eq!(
            rows[5].get("big_int_col").unwrap().as_i64().unwrap(),
            big_max
        );
    }

    async fn test_int_vs_bigint_type_safety(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("type_safety_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "int_col".to_string(),
                data_type: DataType::Int,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "big_int_col".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create type_safety_test table");

        let result = db
            .insert(table_name)
            .value("int_col", i32::MAX)
            .value("big_int_col", 0i64)
            .execute(&*db)
            .await;
        assert!(result.is_ok(), "i32::MAX should fit in Int column");

        let result = db
            .insert(table_name)
            .value("int_col", 0)
            .value("big_int_col", i64::MAX)
            .execute(&*db)
            .await;
        assert!(result.is_ok(), "i64::MAX should fit in BigInt column");

        let rows = db.select(table_name).execute(&*db).await.unwrap();
        assert_eq!(rows.len(), 2);

        assert!(matches!(
            rows[0].get("int_col").unwrap(),
            DatabaseValue::Int32(_) | DatabaseValue::Int64(_)
        ));
        assert!(matches!(
            rows[0].get("big_int_col").unwrap(),
            DatabaseValue::Int64(_) | DatabaseValue::Int32(_)
        ));
    }

    async fn test_string_types_varchar_text_char(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("string_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "varchar_col".to_string(),
                data_type: DataType::VarChar(100),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "text_col".to_string(),
                data_type: DataType::Text,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "char_col".to_string(),
                data_type: DataType::Char(10),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create string_test table");

        let short_str = "short";
        let medium_str = "a".repeat(50);
        let max_str = "b".repeat(100);

        db.insert(table_name)
            .value("varchar_col", short_str)
            .value("text_col", "text1")
            .value("char_col", "char1")
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("varchar_col", &medium_str)
            .value("text_col", "text2")
            .value("char_col", "char2")
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("varchar_col", &max_str)
            .value("text_col", "text3")
            .value("char_col", "char3")
            .execute(&*db)
            .await
            .unwrap();

        let large_text = "x".repeat(10000);
        db.insert(table_name)
            .value("varchar_col", "test")
            .value("text_col", &large_text)
            .value("char_col", "large")
            .execute(&*db)
            .await
            .unwrap();

        let emoji_str = "Hello 👋 World 🌍";
        db.insert(table_name)
            .value("varchar_col", emoji_str)
            .value("text_col", "emoji test")
            .value("char_col", "emoji")
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows[0].get("varchar_col").unwrap().as_str().unwrap(),
            short_str
        );
        assert_eq!(
            rows[1].get("varchar_col").unwrap().as_str().unwrap(),
            &medium_str
        );
        assert_eq!(
            rows[2].get("varchar_col").unwrap().as_str().unwrap(),
            &max_str
        );
        assert_eq!(
            rows[3].get("text_col").unwrap().as_str().unwrap(),
            &large_text
        );
        assert_eq!(
            rows[4].get("varchar_col").unwrap().as_str().unwrap(),
            emoji_str
        );
    }

    async fn test_floating_point_types(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("float_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "real_col".to_string(),
                data_type: DataType::Real,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "double_col".to_string(),
                data_type: DataType::Double,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create float_test table");

        #[allow(clippy::approx_constant)]
        let real_val = 3.14159_f32;
        db.insert(table_name)
            .value("real_col", real_val)
            .value("double_col", 0.0)
            .execute(&*db)
            .await
            .unwrap();

        #[allow(clippy::approx_constant)]
        let double_val = 3.141592653589793_f64;
        db.insert(table_name)
            .value("real_col", 0.0_f32)
            .value("double_col", double_val)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 2);

        let real_retrieved = rows[0].get("real_col").unwrap().as_f64().unwrap();
        assert!((real_retrieved - f64::from(real_val)).abs() < 0.0001);

        let double_retrieved = rows[1].get("double_col").unwrap().as_f64().unwrap();
        assert!((double_retrieved - double_val).abs() < 1e-10);
    }

    #[cfg(feature = "decimal")]
    async fn test_decimal_precision(&self) {
        use rust_decimal_macros::dec;

        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("decimal_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "decimal_col".to_string(),
                data_type: DataType::Decimal(10, 2),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create decimal_test table");

        let price1 = dec!(123.45);
        let price2 = dec!(999.99);
        let price3 = dec!(0.01);

        db.insert(table_name)
            .value("decimal_col", price1)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("decimal_col", price2)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("decimal_col", price3)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("decimal_col", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        let val0 = rows[0].get("decimal_col").unwrap().as_decimal().unwrap();
        let val1 = rows[1].get("decimal_col").unwrap().as_decimal().unwrap();
        let val2 = rows[2].get("decimal_col").unwrap().as_decimal().unwrap();

        assert_eq!(val0, price3);
        assert_eq!(val1, price1);
        assert_eq!(val2, price2);
    }

    #[cfg(feature = "uuid")]
    async fn test_uuid_storage(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("uuid_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "uuid_col".to_string(),
                data_type: DataType::Uuid,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create uuid_test table");

        let uuid1 = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let uuid2 = uuid::Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let uuid3 = uuid::Uuid::parse_str("6ba7b811-9dad-11d1-80b4-00c04fd430c8").unwrap();

        db.insert(table_name)
            .value("uuid_col", uuid1)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("uuid_col", uuid2)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("uuid_col", uuid3)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        let val0 = rows[0].get("uuid_col").unwrap().as_uuid().unwrap();
        let val1 = rows[1].get("uuid_col").unwrap().as_uuid().unwrap();
        let val2 = rows[2].get("uuid_col").unwrap().as_uuid().unwrap();

        assert_eq!(val0, uuid1);
        assert_eq!(val1, uuid2);
        assert_eq!(val2, uuid3);
    }

    async fn test_boolean_type(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("bool_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "bool_col".to_string(),
                data_type: DataType::Bool,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create bool_test table");

        db.insert(table_name)
            .value("bool_col", true)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("bool_col", false)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("bool_col", DatabaseValue::Null)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        let val0 = rows[0].get("bool_col").unwrap();
        let val1 = rows[1].get("bool_col").unwrap();
        let val2 = rows[2].get("bool_col").unwrap();

        assert!(
            val0.as_bool()
                .unwrap_or_else(|| val0.as_i64().unwrap() != 0)
        );
        assert!(
            !val1
                .as_bool()
                .unwrap_or_else(|| val1.as_i64().unwrap() != 0)
        );
        assert!(matches!(val2, DatabaseValue::Null));

        let true_rows = db
            .select(table_name)
            .where_eq("bool_col", true)
            .execute(&*db)
            .await
            .unwrap();
        assert_eq!(true_rows.len(), 1);
    }

    async fn test_datetime_types(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("datetime_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "datetime_col".to_string(),
                data_type: DataType::DateTime,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create datetime_test table");

        use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

        let date_val = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let time_val = NaiveTime::from_hms_opt(14, 30, 45).unwrap();
        let datetime_val = NaiveDateTime::new(date_val, time_val);

        db.insert(table_name)
            .value("datetime_col", DatabaseValue::DateTime(datetime_val))
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("datetime_col", DatabaseValue::Now)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 2);

        if let Some(retrieved_dt) = rows[0].get("datetime_col").unwrap().as_datetime() {
            assert_eq!(retrieved_dt.date(), datetime_val.date());
        }

        if let Some(now_dt) = rows[1].get("datetime_col").unwrap().as_datetime() {
            let diff = chrono::Utc::now().naive_utc().signed_duration_since(now_dt);
            assert!(diff.num_seconds().abs() < 10);
        }
    }

    async fn test_null_handling_all_types(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("null_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "int_col".to_string(),
                data_type: DataType::Int,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "big_int_col".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "varchar_col".to_string(),
                data_type: DataType::VarChar(50),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "bool_col".to_string(),
                data_type: DataType::Bool,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "real_col".to_string(),
                data_type: DataType::Real,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create null_test table");

        db.insert(table_name)
            .value("int_col", DatabaseValue::Null)
            .value("big_int_col", DatabaseValue::Null)
            .value("varchar_col", DatabaseValue::Null)
            .value("bool_col", DatabaseValue::Null)
            .value("real_col", DatabaseValue::Null)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db.select(table_name).execute(&*db).await.unwrap();
        assert_eq!(rows.len(), 1);

        assert!(matches!(
            rows[0].get("int_col").unwrap(),
            DatabaseValue::Null
        ));
        assert!(matches!(
            rows[0].get("big_int_col").unwrap(),
            DatabaseValue::Null
        ));
        assert!(matches!(
            rows[0].get("varchar_col").unwrap(),
            DatabaseValue::Null
        ));
        assert!(matches!(
            rows[0].get("bool_col").unwrap(),
            DatabaseValue::Null
        ));
        assert!(matches!(
            rows[0].get("real_col").unwrap(),
            DatabaseValue::Null
        ));

        db.update(table_name)
            .value("int_col", 42)
            .execute(&*db)
            .await
            .unwrap();

        db.update(table_name)
            .value("int_col", DatabaseValue::Null)
            .execute(&*db)
            .await
            .unwrap();

        let updated = db.select(table_name).execute(&*db).await.unwrap();
        assert!(matches!(
            updated[0].get("int_col").unwrap(),
            DatabaseValue::Null
        ));
    }

    async fn test_serial_auto_increment(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("serial_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigSerial,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "value".to_string(),
                data_type: DataType::VarChar(50),
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create serial_test table");

        db.insert(table_name)
            .value("value", "first")
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("value", "second")
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 2);
        assert!(rows[0].get("id").unwrap().as_i64().unwrap() > 0);
        assert!(
            rows[1].get("id").unwrap().as_i64().unwrap()
                > rows[0].get("id").unwrap().as_i64().unwrap()
        );
    }

    async fn test_default_values_all_types(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("default_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "int_with_default".to_string(),
                data_type: DataType::Int,
                nullable: false,
                auto_increment: false,
                default: Some(DatabaseValue::Int32(42)),
            })
            .column(Column {
                name: "bigint_with_default".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: Some(DatabaseValue::Int64(9999i64)),
            })
            .column(Column {
                name: "string_with_default".to_string(),
                data_type: DataType::VarChar(50),
                nullable: false,
                auto_increment: false,
                default: Some(DatabaseValue::String("default_value".to_string())),
            })
            .column(Column {
                name: "bool_with_default".to_string(),
                data_type: DataType::Bool,
                nullable: false,
                auto_increment: false,
                default: Some(DatabaseValue::Bool(true)),
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create default_test table");

        db.insert(table_name).execute(&*db).await.unwrap();

        let rows = db.select(table_name).execute(&*db).await.unwrap();
        assert_eq!(rows.len(), 1);

        assert_eq!(
            rows[0].get("int_with_default").unwrap().as_i64().unwrap(),
            42
        );
        assert_eq!(
            rows[0]
                .get("bigint_with_default")
                .unwrap()
                .as_i64()
                .unwrap(),
            9999
        );
        assert_eq!(
            rows[0]
                .get("string_with_default")
                .unwrap()
                .as_str()
                .unwrap(),
            "default_value"
        );

        let bool_val = rows[0].get("bool_with_default").unwrap();
        assert!(
            bool_val
                .as_bool()
                .unwrap_or_else(|| bool_val.as_i64().unwrap() != 0)
        );
    }

    async fn test_int8_specific_type_and_retrieval(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("int8_specific_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "tiny_int_col".to_string(),
                data_type: DataType::TinyInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "small_int_col".to_string(),
                data_type: DataType::SmallInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "int_col".to_string(),
                data_type: DataType::Int,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create int8_specific_test table");

        db.insert(table_name)
            .value("id", 1i64)
            .value("tiny_int_col", i8::MIN)
            .value("small_int_col", i16::MIN)
            .value("int_col", i32::MIN)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 2i64)
            .value("tiny_int_col", i8::MAX)
            .value("small_int_col", i16::MAX)
            .value("int_col", i32::MAX)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 3i64)
            .value("tiny_int_col", 50i8)
            .value("small_int_col", 100i16)
            .value("int_col", 1000)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        assert!(
            matches!(
                rows[0].get("tiny_int_col").unwrap(),
                DatabaseValue::Int8(_) | DatabaseValue::Int16(_) | DatabaseValue::Int64(_)
            ),
            "TINYINT column should return Int8 (or Int64 for SQLite, or Int16 for Postgres), got {:?}",
            rows[0].get("tiny_int_col").unwrap()
        );
        assert!(
            matches!(
                rows[0].get("small_int_col").unwrap(),
                DatabaseValue::Int16(_) | DatabaseValue::Int64(_)
            ),
            "SMALLINT column should return Int16 or (or Int64 for SQLite), got {:?}",
            rows[0].get("small_int_col").unwrap()
        );
        assert!(
            matches!(
                rows[0].get("int_col").unwrap(),
                DatabaseValue::Int32(_) | DatabaseValue::Int64(_)
            ),
            "INT column should return Int32 or (or Int64 for SQLite), got {:?}",
            rows[0].get("int_col").unwrap()
        );

        let tiny_val_0: i8 = rows[0]
            .get("tiny_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(tiny_val_0, i8::MIN);

        let tiny_val_1: i8 = rows[1]
            .get("tiny_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(tiny_val_1, i8::MAX);

        let tiny_val_2: i8 = rows[2]
            .get("tiny_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(tiny_val_2, 50i8);

        if rows[0].get("tiny_int_col").unwrap().as_i8().is_some() {
            assert_eq!(
                rows[0].get("tiny_int_col").unwrap().as_i8().unwrap(),
                i8::MIN
            );
            assert_eq!(
                rows[1].get("tiny_int_col").unwrap().as_i8().unwrap(),
                i8::MAX
            );
            assert_eq!(rows[2].get("tiny_int_col").unwrap().as_i8().unwrap(), 50i8);
        }

        let small_val_0: i16 = rows[0]
            .get("small_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(small_val_0, i16::MIN);

        let int_val_0: i32 = rows[0].get("int_col").unwrap().clone().try_into().unwrap();
        assert_eq!(int_val_0, i32::MIN);
    }

    async fn test_int16_specific_type_and_retrieval(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("int16_specific_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "small_int_col".to_string(),
                data_type: DataType::SmallInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "int_col".to_string(),
                data_type: DataType::Int,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "big_int_col".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create int16_specific_test table");

        db.insert(table_name)
            .value("small_int_col", i16::MIN)
            .value("int_col", i32::MIN)
            .value("big_int_col", i64::MIN)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("small_int_col", i16::MAX)
            .value("int_col", i32::MAX)
            .value("big_int_col", i64::MAX)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("small_int_col", 100i16)
            .value("int_col", 1000)
            .value("big_int_col", 10000i64)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        assert!(
            matches!(
                rows[0].get("small_int_col").unwrap(),
                DatabaseValue::Int16(_) | DatabaseValue::Int64(_)
            ),
            "SMALLINT column should return Int16 (or Int64 for SQLite), got {:?}",
            rows[0].get("small_int_col").unwrap()
        );
        assert!(
            matches!(
                rows[0].get("int_col").unwrap(),
                DatabaseValue::Int32(_) | DatabaseValue::Int64(_)
            ),
            "INT column should return Int32 or Int64, got {:?}",
            rows[0].get("int_col").unwrap()
        );
        assert!(
            matches!(
                rows[0].get("big_int_col").unwrap(),
                DatabaseValue::Int64(_) | DatabaseValue::Int32(_)
            ),
            "BIGINT column should return Int64, got {:?}",
            rows[0].get("big_int_col").unwrap()
        );

        let small_val_0: i16 = rows[0]
            .get("small_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(small_val_0, i16::MIN);

        let small_val_1: i16 = rows[1]
            .get("small_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(small_val_1, i16::MAX);

        let small_val_2: i16 = rows[2]
            .get("small_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(small_val_2, 100i16);

        if rows[0].get("small_int_col").unwrap().as_i16().is_some() {
            assert_eq!(
                rows[0].get("small_int_col").unwrap().as_i16().unwrap(),
                i16::MIN
            );
            assert_eq!(
                rows[1].get("small_int_col").unwrap().as_i16().unwrap(),
                i16::MAX
            );
            assert_eq!(
                rows[2].get("small_int_col").unwrap().as_i16().unwrap(),
                100i16
            );
        }

        let int_val_0: i32 = rows[0].get("int_col").unwrap().clone().try_into().unwrap();
        assert_eq!(int_val_0, i32::MIN);
        let int_val_1: i32 = rows[1].get("int_col").unwrap().clone().try_into().unwrap();
        assert_eq!(int_val_1, i32::MAX);

        let big_val_0: i64 = rows[0]
            .get("big_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(big_val_0, i64::MIN);
        let big_val_1: i64 = rows[1]
            .get("big_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(big_val_1, i64::MAX);

        let small_as_i32: i32 = rows[0]
            .get("small_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(small_as_i32, i32::from(i16::MIN));

        let small_as_i64: i64 = rows[0]
            .get("small_int_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(small_as_i64, i64::from(i16::MIN));
    }

    async fn test_uint8_specific_type_and_retrieval(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("uint8_specific_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "value_col".to_string(),
                data_type: DataType::SmallInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create uint8_specific_test table");

        db.insert(table_name)
            .value("id", 1i64)
            .value("value_col", 0u8)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 2i64)
            .value("value_col", 127u8)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 3i64)
            .value("value_col", 50u8)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        let val0: u8 = rows[0]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val0, 0u8);

        let val1: u8 = rows[1]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val1, 127u8);

        let val2: u8 = rows[2]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val2, 50u8);
    }

    async fn test_uint16_specific_type_and_retrieval(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("uint16_specific_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "value_col".to_string(),
                data_type: DataType::SmallInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create uint16_specific_test table");

        db.insert(table_name)
            .value("id", 1i64)
            .value("value_col", 0u16)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 2i64)
            .value("value_col", 32767u16)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 3i64)
            .value("value_col", 1000u16)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        let val0: u16 = rows[0]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val0, 0u16);

        let val1: u16 = rows[1]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val1, 32767u16);

        let val2: u16 = rows[2]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val2, 1000u16);
    }

    async fn test_uint32_specific_type_and_retrieval(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("uint32_specific_test");
        let table_name = &table_name;
        drop_table(table_name)
            .if_exists(true)
            .execute(&*db)
            .await
            .ok();

        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "value_col".to_string(),
                data_type: DataType::Int,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .expect("Failed to create uint32_specific_test table");

        db.insert(table_name)
            .value("id", 1i64)
            .value("value_col", 0u32)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 2i64)
            .value("value_col", 2_147_483_647u32)
            .execute(&*db)
            .await
            .unwrap();

        db.insert(table_name)
            .value("id", 3i64)
            .value("value_col", 100_000u32)
            .execute(&*db)
            .await
            .unwrap();

        let rows = db
            .select(table_name)
            .sort("id", SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 3);

        let val0: u32 = rows[0]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val0, 0u32);

        let val1: u32 = rows[1]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val1, 2_147_483_647u32);

        let val2: u32 = rows[2]
            .get("value_col")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(val2, 100_000u32);
    }
}
