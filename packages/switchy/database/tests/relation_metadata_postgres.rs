//! `PostgreSQL` multi-schema relation introspection, lossless decoding and
//! result-set metadata tests for both the raw (`tokio-postgres`) and sqlx
//! backends.
//!
//! Gated on `POSTGRES_TEST_URL`; tests return early when it is unset.

#![cfg(all(feature = "schema", feature = "raw-sql"))]
#![cfg(any(feature = "postgres-raw", feature = "postgres-sqlx"))]

use switchy_database::{
    Database, DatabaseValue,
    schema::{DataType, GeneratedKind, RelationKind, RelationRef},
};

async fn exec_all(db: &dyn Database, statements: &[String]) {
    for statement in statements {
        db.exec_raw(statement)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
}

async fn setup(db: &dyn Database, s: &str) {
    exec_all(
        db,
        &[
            format!("DROP SCHEMA IF EXISTS {s} CASCADE"),
            format!("CREATE SCHEMA {s}"),
            format!("CREATE TYPE {s}.mood AS ENUM ('sad', 'happy')"),
            format!("ALTER TYPE {s}.mood ADD VALUE 'it''s, meh' BEFORE 'happy'"),
            format!("CREATE DOMAIN {s}.posint AS integer CHECK (VALUE > 0)"),
            format!(
                "CREATE TABLE {s}.parent (id SERIAL PRIMARY KEY, name VARCHAR(40) NOT NULL UNIQUE)"
            ),
            format!("COMMENT ON TABLE {s}.parent IS 'Parent rows'"),
            format!(
                "CREATE TABLE {s}.child (
                    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                    parent_id INTEGER REFERENCES {s}.parent(id) ON DELETE CASCADE,
                    feeling {s}.mood,
                    span INTERVAL,
                    tags INTEGER[],
                    addr INET,
                    cost MONEY,
                    qty {s}.posint,
                    born DATE,
                    alarm TIME,
                    blob BYTEA,
                    doc JSONB,
                    stamp TIMESTAMPTZ,
                    price NUMERIC(10,2),
                    moods {s}.mood[],
                    price_x2 NUMERIC GENERATED ALWAYS AS (price * 2) STORED
                )"
            ),
            format!("CREATE VIEW {s}.child_view AS SELECT id, feeling FROM {s}.child"),
            format!("CREATE MATERIALIZED VIEW {s}.parent_mv AS SELECT id, name FROM {s}.parent"),
            format!("INSERT INTO {s}.parent (name) VALUES ('a'), ('b')"),
            format!(
                "INSERT INTO {s}.child (parent_id, feeling, span, tags, addr, cost, qty, born, alarm, blob, doc, stamp, price)
                 VALUES (1, 'happy', '1 day 2 hours', '{{1,2,NULL}}', '10.0.0.1', 12.34, 5,
                         '2024-02-29', '13:14:15', '\\x0102ff', '{{\"a\": [1, true]}}',
                         '2024-01-02 03:04:05+02', 1234.5)"
            ),
            format!("ANALYZE {s}.parent"),
        ],
    )
    .await;
}

async fn teardown(db: &dyn Database, s: &str) {
    db.exec_raw(&format!("DROP SCHEMA IF EXISTS {s} CASCADE"))
        .await
        .unwrap();
}

#[allow(clippy::too_many_lines)]
async fn check_introspection(db: &dyn Database, s: &str) {
    let schemas = db.list_schemas().await.unwrap();
    let mine = schemas.iter().find(|x| x.name == s).unwrap();
    assert!(!mine.is_default && !mine.is_system);
    assert!(
        schemas
            .iter()
            .any(|x| x.name == "pg_catalog" && x.is_system)
    );
    assert!(
        schemas
            .iter()
            .any(|x| x.name == "information_schema" && x.is_system)
    );
    assert!(schemas.iter().any(|x| x.name == "public" && x.is_default));

    let databases = db.list_databases().await.unwrap();
    let current = databases
        .iter()
        .filter(|d| d.is_current)
        .collect::<Vec<_>>();
    assert_eq!(current.len(), 1);
    assert!(databases.iter().all(|d| !d.is_system));
    assert!(!databases.iter().any(|d| d.name.starts_with("template")));

    let relations = db.list_relations(Some(s)).await.unwrap();
    let find = |name: &str| relations.iter().find(|r| r.name == name).unwrap();
    let parent = find("parent");
    assert_eq!(parent.kind, RelationKind::Table);
    assert_eq!(parent.schema.as_deref(), Some(s));
    assert_eq!(parent.comment.as_deref(), Some("Parent rows"));
    assert_eq!(parent.estimated_rows, Some(2));
    assert_eq!(find("child").kind, RelationKind::Table);
    assert_eq!(find("child_view").kind, RelationKind::View);
    assert_eq!(find("parent_mv").kind, RelationKind::MaterializedView);
    assert!(!relations.iter().any(|r| r.name == "parent_id_seq"));

    for relation in db.list_relations(None).await.unwrap() {
        assert_eq!(relation.schema.as_deref(), Some("public"));
    }

    let child = db
        .get_relation_info(&RelationRef::with_schema(s, "child"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child.schema.as_deref(), Some(s));
    assert_eq!(child.kind, RelationKind::Table);
    let column = |name: &str| child.columns.get(name).unwrap();
    assert!(column("id").auto_increment);
    assert!(column("id").is_primary_key);
    assert!(!column("id").nullable);
    assert_eq!(column("id").data_type, DataType::BigInt);
    assert!(matches!(column("feeling").data_type, DataType::Custom(_)));
    assert!(column("feeling").native_type.ends_with("mood"));
    assert_eq!(
        column("span").data_type,
        DataType::Custom("interval".to_string())
    );
    assert_eq!(
        column("tags").data_type,
        DataType::Array(Box::new(DataType::Int))
    );
    assert_eq!(column("tags").native_type, "integer[]");
    assert_eq!(column("addr").data_type, DataType::Inet);
    assert_eq!(column("cost").data_type, DataType::Money);
    assert!(column("qty").native_type.ends_with("posint"));
    assert_eq!(column("born").data_type, DataType::Date);
    assert_eq!(column("alarm").data_type, DataType::Time);
    assert_eq!(column("blob").data_type, DataType::Blob);
    assert_eq!(column("doc").data_type, DataType::Jsonb);
    assert_eq!(column("stamp").native_type, "timestamp with time zone");
    assert_eq!(column("price").data_type, DataType::Decimal(10, 2));
    assert_eq!(column("price").ordinal_position, 14);

    // Enum labels in sort order (including values added later), not for arrays
    assert_eq!(
        column("feeling").enum_values,
        Some(vec![
            "sad".to_string(),
            "it's, meh".to_string(),
            "happy".to_string()
        ])
    );
    assert_eq!(column("moods").enum_values, None);
    assert_eq!(column("tags").enum_values, None);
    assert_eq!(column("qty").enum_values, None);

    // Generated columns; identity columns stay auto-increment, not generated
    assert_eq!(column("price_x2").generated, Some(GeneratedKind::Stored));
    assert_eq!(column("price_x2").default_value, None);
    assert!(!column("price_x2").auto_increment);
    assert_eq!(column("id").generated, None);
    assert_eq!(column("price").generated, None);

    let fk = child.foreign_keys.values().next().unwrap();
    assert_eq!(fk.column, "parent_id");
    assert_eq!(fk.referenced_table, "parent");
    assert_eq!(fk.referenced_column, "id");
    assert_eq!(fk.on_delete.as_deref(), Some("CASCADE"));
    assert!(child.indexes.get("child_pkey").unwrap().is_primary);

    let parent = db
        .get_relation_info(&RelationRef::with_schema(s, "parent"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(parent.comment.as_deref(), Some("Parent rows"));
    assert!(parent.columns.get("id").unwrap().auto_increment);
    assert_eq!(
        parent.columns.get("name").unwrap().data_type,
        DataType::VarChar(40)
    );
    let unique = parent
        .indexes
        .values()
        .find(|i| i.unique && !i.is_primary)
        .unwrap();
    assert_eq!(unique.columns, vec!["name".to_string()]);

    let view = db
        .get_relation_info(&RelationRef::with_schema(s, "child_view"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.kind, RelationKind::View);
    assert_eq!(view.columns.len(), 2);
    assert!(view.indexes.is_empty());
    assert_eq!(
        view.columns.get("feeling").unwrap().enum_values,
        column("feeling").enum_values
    );

    let mv = db
        .get_relation_info(&RelationRef::with_schema(s, "parent_mv"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mv.kind, RelationKind::MaterializedView);
    assert!(mv.columns.contains_key("name"));

    assert!(
        db.get_relation_info(&RelationRef::with_schema(s, "missing"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.get_relation_info(&RelationRef::new("child"))
            .await
            .unwrap()
            .is_none(),
        "default schema must not see other schemas"
    );

    let definition = db
        .get_view_definition(&RelationRef::with_schema(s, "child_view"))
        .await
        .unwrap()
        .unwrap();
    assert!(definition.to_uppercase().contains("FROM"), "{definition}");
    assert!(definition.contains("child"), "{definition}");
    assert!(
        db.get_view_definition(&RelationRef::with_schema(s, "parent_mv"))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        db.get_view_definition(&RelationRef::with_schema(s, "parent"))
            .await
            .unwrap()
            .is_none()
    );
}

#[allow(clippy::too_many_lines)]
async fn check_decoding(db: &dyn Database, s: &str) {
    let result = db
        .query_raw_result(
            &format!(
                "SELECT feeling, span, tags, addr, cost, qty, born, alarm, blob, doc, stamp, price,
                        1 AS dup, 'x' AS dup FROM {s}.child"
            ),
            &[],
        )
        .await
        .unwrap();
    let names: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "feeling", "span", "tags", "addr", "cost", "qty", "born", "alarm", "blob", "doc",
            "stamp", "price", "dup", "dup"
        ]
    );
    let type_name = |i: usize| result.columns[i].type_name.as_deref().unwrap();
    assert!(type_name(0).ends_with("mood"));
    assert_eq!(type_name(1), "interval");
    assert_eq!(type_name(2), "int4[]");
    assert_eq!(type_name(6), "date");
    assert_eq!(type_name(10), "timestamptz");
    assert_eq!(type_name(13), "text");

    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];
    assert_eq!(row.len(), 14);
    let other = |i: usize| row[i].as_other().map(|(_, text)| text.to_string());
    assert_eq!(other(0).as_deref(), Some("happy"));
    assert_eq!(other(1).as_deref(), Some("1 day 02:00:00"));
    assert_eq!(other(2).as_deref(), Some("{1,2,NULL}"));
    assert_eq!(other(3).as_deref(), Some("10.0.0.1"));
    assert_eq!(other(4).as_deref(), Some("12.34"));
    assert_eq!(
        row[5].as_i64().or_else(|| row[5].as_i32().map(i64::from)),
        Some(5)
    );
    assert_eq!(
        row[6].as_date(),
        chrono::NaiveDate::from_ymd_opt(2024, 2, 29)
    );
    assert_eq!(
        row[7].as_time(),
        chrono::NaiveTime::from_hms_opt(13, 14, 15)
    );
    assert_eq!(row[8].as_bytes(), Some(&[1u8, 2, 0xff][..]));
    #[cfg(feature = "json")]
    assert_eq!(row[9].as_json(), Some(&serde_json::json!({"a": [1, true]})));
    #[cfg(not(feature = "json"))]
    assert_eq!(row[9].as_str(), Some("{\"a\": [1, true]}"));
    assert_eq!(
        row[10].as_datetime(),
        chrono::NaiveDate::from_ymd_opt(2024, 1, 2).and_then(|d| d.and_hms_opt(1, 4, 5))
    );
    #[cfg(feature = "decimal")]
    assert_eq!(
        row[11].as_decimal(),
        Some(rust_decimal::Decimal::new(123_450, 2))
    );
    assert!(row[12].as_i64().or_else(|| row[12].as_i32().map(i64::from)) == Some(1));
    assert_eq!(row[13].as_str(), Some("x"));

    // Row-based API decodes the same unknown types without failing
    let rows = db
        .query_raw(&format!("SELECT feeling, span, tags FROM {s}.child"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0].get("span"),
        Some(DatabaseValue::Other { .. })
    ));

    // Zero-row results still describe their columns
    let empty = db
        .query_raw_result(
            &format!("SELECT id, feeling FROM {s}.child WHERE false"),
            &[],
        )
        .await
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 2);
    assert_eq!(empty.columns[0].name, "id");
    assert_eq!(empty.columns[0].type_name.as_deref(), Some("int8"));

    // Binding round-trips for the new value variants
    let date = chrono::NaiveDate::from_ymd_opt(1999, 12, 31).unwrap();
    let time = chrono::NaiveTime::from_hms_micro_opt(1, 2, 3, 456_000).unwrap();
    let stamp = chrono::DateTime::parse_from_rfc3339("2020-05-06T07:08:09-05:00").unwrap();
    #[cfg(feature = "json")]
    let doc = DatabaseValue::Json(serde_json::json!({"k": "v"}));
    #[cfg(not(feature = "json"))]
    let doc = DatabaseValue::Null;
    db.exec_raw_params(
        &format!(
            "INSERT INTO {s}.child (born, alarm, blob, stamp, span, doc) \
             VALUES ($1, $2, $3, $4, $5::interval, $6::jsonb)"
        ),
        &[
            DatabaseValue::Date(date),
            DatabaseValue::Time(time),
            DatabaseValue::Bytes(vec![0, 1, 2]),
            DatabaseValue::DateTimeTz(stamp),
            DatabaseValue::Other {
                type_name: "interval".to_string(),
                text: "3 days".to_string(),
            },
            doc,
        ],
    )
    .await
    .unwrap();
    let back = db
        .query_raw_result(
            &format!("SELECT alarm, blob, stamp, span, doc FROM {s}.child WHERE born = $1"),
            &[DatabaseValue::Date(date)],
        )
        .await
        .unwrap();
    assert_eq!(back.rows.len(), 1);
    let row = &back.rows[0];
    assert_eq!(row[0].as_time(), Some(time));
    assert_eq!(row[1].as_bytes(), Some(&[0u8, 1, 2][..]));
    assert_eq!(row[2].as_datetime(), Some(stamp.naive_utc()));
    assert_eq!(row[3].as_other().map(|(_, t)| t), Some("3 days"));
    #[cfg(feature = "json")]
    assert_eq!(row[4].as_json(), Some(&serde_json::json!({"k": "v"})));

    // Typed NULLs for the new variants bind as parameters
    let nulls = db
        .query_raw_result(
            "SELECT $1::bytea IS NULL, $2::date IS NULL, $3::time IS NULL, $4::timestamptz IS NULL",
            &[
                DatabaseValue::BytesOpt(None),
                DatabaseValue::DateOpt(None),
                DatabaseValue::TimeOpt(None),
                DatabaseValue::DateTimeTzOpt(None),
            ],
        )
        .await
        .unwrap();
    assert!(nulls.rows[0].iter().all(|v| v.as_bool() == Some(true)));

    // Transactions expose the same APIs on their own connection
    let tx = db.begin_transaction().await.unwrap();
    tx.exec_raw(&format!("CREATE TABLE {s}.tx_only (x INT)"))
        .await
        .unwrap();
    assert!(
        tx.list_relations(Some(s))
            .await
            .unwrap()
            .iter()
            .any(|r| r.name == "tx_only")
    );
    assert!(
        tx.get_relation_info(&RelationRef::with_schema(s, "tx_only"))
            .await
            .unwrap()
            .is_some()
    );
    assert!(!tx.list_schemas().await.unwrap().is_empty());
    assert!(!tx.list_databases().await.unwrap().is_empty());
    let tx_result = tx
        .query_raw_result(&format!("SELECT x FROM {s}.tx_only"), &[])
        .await
        .unwrap();
    assert_eq!(tx_result.columns[0].type_name.as_deref(), Some("int4"));
    tx.rollback().await.unwrap();
}

/// Query-builder placeholders stay aligned when new `*Opt(None)` variants
/// precede non-null values.
async fn check_builder_nulls(db: &dyn Database, table: &str) {
    exec_all(
        db,
        &[
            format!("DROP TABLE IF EXISTS {table}"),
            format!("CREATE TABLE {table} (b BYTEA, d DATE, n INTEGER, t TIME, x BYTEA)"),
        ],
    )
    .await;
    db.insert(table)
        .value("b", DatabaseValue::BytesOpt(None))
        .value("d", DatabaseValue::DateOpt(None))
        .value("n", 7)
        .value("t", DatabaseValue::TimeOpt(None))
        .value("x", DatabaseValue::Bytes(vec![9]))
        .execute(db)
        .await
        .unwrap();
    let result = db
        .query_raw_result(&format!("SELECT b, d, n, t, x FROM {table}"), &[])
        .await
        .unwrap();
    let row = &result.rows[0];
    assert!(row[0].is_null() && row[1].is_null() && row[3].is_null());
    assert_eq!(
        row[2].as_i64().or_else(|| row[2].as_i32().map(i64::from)),
        Some(7)
    );
    assert_eq!(row[4].as_bytes(), Some(&[9u8][..]));
    db.exec_raw(&format!("DROP TABLE {table}")).await.unwrap();
}

async fn check_routines(db: &dyn Database, s: &str) {
    use switchy_database::schema::RoutineKind;
    exec_all(
        db,
        &[
            format!(
                "CREATE FUNCTION {s}.add_one(x integer) RETURNS integer \
                 LANGUAGE sql AS 'SELECT x + 1'"
            ),
            format!(
                "CREATE FUNCTION {s}.touch() RETURNS trigger \
                 LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'"
            ),
        ],
    )
    .await;
    let routines = db.list_routines(Some(s)).await.unwrap();
    // Trigger functions are left out.
    assert!(routines.iter().all(|r| r.name != "touch"), "{routines:?}");
    let add_one = routines.iter().find(|r| r.name == "add_one").unwrap();
    assert_eq!(add_one.kind, RoutineKind::Function);
    assert_eq!(add_one.arguments.as_deref(), Some("x integer"));
    assert_eq!(add_one.result.as_deref(), Some("integer"));
    assert!(!add_one.builtin);
    let visible = db.list_routines(None).await.unwrap();
    assert!(visible.iter().any(|r| r.name == "lower" && r.builtin));
    assert!(
        visible
            .iter()
            .any(|r| r.name == "count" && r.kind == RoutineKind::Aggregate)
    );
}

async fn check_objects(db: &dyn Database, s: &str) {
    use switchy_database::schema::UserTypeKind;
    exec_all(
        db,
        &[
            format!("CREATE TYPE {s}.span AS (lo integer, hi integer)"),
            format!("CREATE TYPE {s}.window_range AS RANGE (subtype = integer)"),
            format!("CREATE SEQUENCE {s}.order_no AS bigint"),
            format!(
                "CREATE FUNCTION {s}.touch() RETURNS trigger LANGUAGE plpgsql \
                 AS 'BEGIN RETURN NEW; END'"
            ),
            format!(
                "CREATE TRIGGER parent_touch BEFORE INSERT OR UPDATE ON {s}.parent \
                 FOR EACH ROW EXECUTE FUNCTION {s}.touch()"
            ),
            format!("ALTER TABLE {s}.parent DISABLE TRIGGER parent_touch"),
        ],
    )
    .await;

    let triggers = db.list_triggers(Some(s)).await.unwrap();
    assert_eq!(triggers.len(), 1, "{triggers:?}");
    let touch = &triggers[0];
    assert_eq!(touch.name, "parent_touch");
    assert_eq!(touch.schema.as_deref(), Some(s));
    assert_eq!(touch.table.as_deref(), Some("parent"));
    assert_eq!(touch.timing.as_deref(), Some("BEFORE"));
    assert_eq!(touch.events, vec!["INSERT", "UPDATE"]);
    assert!(!touch.enabled);
    assert!(
        touch
            .definition
            .as_deref()
            .is_some_and(|sql| sql.starts_with("CREATE TRIGGER parent_touch"))
    );

    let sequences = db.list_sequences(Some(s)).await.unwrap();
    let order_no = sequences.iter().find(|q| q.name == "order_no").unwrap();
    assert_eq!(order_no.data_type.as_deref(), Some("bigint"));
    // SERIAL and identity columns own sequences too.
    assert!(
        sequences
            .iter()
            .any(|q| q.name == "parent_id_seq" && q.data_type.as_deref() == Some("integer"))
    );

    let types = db.list_user_types(Some(s)).await.unwrap();
    let kind = |name: &str| {
        types
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.kind.clone())
    };
    assert_eq!(kind("mood"), Some(UserTypeKind::Enum));
    assert_eq!(kind("posint"), Some(UserTypeKind::Domain));
    assert_eq!(kind("span"), Some(UserTypeKind::Composite));
    assert_eq!(kind("window_range"), Some(UserTypeKind::Range));
    // Row types of tables, array types, and multiranges are left out.
    assert_eq!(kind("parent"), None);
    assert_eq!(kind("_mood"), None);
    assert_eq!(types.len(), 4, "{types:?}");

    let roles = db.list_roles().await.unwrap();
    assert!(
        roles
            .iter()
            .any(|r| r.name == "pg_read_all_data" && r.builtin)
    );
    assert!(
        roles
            .iter()
            .any(|r| !r.builtin && r.can_login == Some(true) && r.superuser.is_some())
    );
}

/// Composite and cross-schema keys keep every column pair and the
/// referenced schema, defaults keep their SQL text, and keys are listed in
/// both directions.
async fn check_foreign_keys(db: &dyn Database, s: &str) {
    use switchy_database::schema::ForeignKeyScope;
    let other = format!("{s}_fk_other");
    exec_all(
        db,
        &[
            format!("DROP SCHEMA IF EXISTS {other} CASCADE"),
            format!("CREATE SCHEMA {other}"),
            format!(
                "CREATE TABLE {s}.tracks (album_id integer, track_no integer, \
                 title text DEFAULT 'untitled', PRIMARY KEY (album_id, track_no))"
            ),
            format!(
                "CREATE TABLE {other}.plays (id integer PRIMARY KEY, album integer, track integer, \
                 CONSTRAINT plays_track_fk FOREIGN KEY (album, track) \
                 REFERENCES {s}.tracks (album_id, track_no) ON DELETE CASCADE)"
            ),
        ],
    )
    .await;
    let tracks = db
        .get_relation_info(&RelationRef::with_schema(s, "tracks"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tracks.columns["title"].default_text.as_deref(),
        Some("'untitled'::text")
    );
    let plays = db
        .get_relation_info(&RelationRef::with_schema(other.as_str(), "plays"))
        .await
        .unwrap()
        .unwrap();
    let key = &plays.foreign_keys["plays_track_fk"];
    assert_eq!(key.columns, vec!["album", "track"]);
    assert_eq!(key.referenced_columns, vec!["album_id", "track_no"]);
    assert_eq!(key.referenced_schema.as_deref(), Some(s));
    assert_eq!(key.on_delete.as_deref(), Some("CASCADE"));

    let referencing = db
        .get_referencing_foreign_keys(&RelationRef::with_schema(s, "tracks"))
        .await
        .unwrap();
    assert_eq!(referencing.len(), 1, "{referencing:?}");
    let found = &referencing[0];
    assert_eq!(
        found.table,
        RelationRef::with_schema(other.as_str(), "plays")
    );
    assert_eq!(found.foreign_key.columns, key.columns);
    assert_eq!(found.foreign_key.referenced_columns, key.referenced_columns);
    assert_eq!(found.foreign_key.on_delete.as_deref(), Some("CASCADE"));
    assert_eq!(found.foreign_key.on_update.as_deref(), Some("NO ACTION"));
    let in_other = db
        .list_foreign_keys(&ForeignKeyScope::Schema(Some(other.clone())))
        .await
        .unwrap();
    assert_eq!(in_other.len(), 1);
    exec_all(db, &[format!("DROP SCHEMA {other} CASCADE")]).await;
}

async fn run_all(db: &dyn Database, schema: &str) {
    setup(db, schema).await;
    check_introspection(db, schema).await;
    check_routines(db, schema).await;
    check_objects(db, schema).await;
    check_foreign_keys(db, schema).await;
    check_decoding(db, schema).await;
    teardown(db, schema).await;
    check_builder_nulls(db, &format!("{schema}_builder_nulls")).await;
}

#[cfg(feature = "postgres-raw")]
mod raw {
    use super::run_all;
    use switchy_database::postgres::postgres::PostgresDatabase;

    #[tokio::test(flavor = "multi_thread")]
    async fn postgres_raw_relation_metadata() {
        let Ok(url) = std::env::var("POSTGRES_TEST_URL") else {
            return;
        };
        let mut cfg = deadpool_postgres::Config::new();
        cfg.url = Some(url);
        let pool = cfg
            .create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )
            .unwrap();
        let db = PostgresDatabase::new(pool);
        run_all(&db, "relmeta_raw").await;
    }
}

#[cfg(feature = "postgres-sqlx")]
mod sqlx_backend {
    use std::sync::Arc;

    use super::run_all;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    #[tokio::test(flavor = "multi_thread")]
    async fn postgres_sqlx_relation_metadata() {
        let Ok(url) = std::env::var("POSTGRES_TEST_URL") else {
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let db = PostgresSqlxDatabase::new(Arc::new(Mutex::new(pool)));
        run_all(&db, "relmeta_sqlx").await;
    }
}
