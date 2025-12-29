//! Comprehensive benchmarks for `switchy_database` backends
//!
//! This benchmark suite compares all database backends across:
//! - CRUD operations (insert, select, update, delete, upsert)
//! - Transaction operations (begin/commit, begin/rollback, savepoints)
//! - Schema operations (table_exists, column_exists, introspection)
//! - Scale benchmarks (bulk inserts, large result sets)
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # Run all benchmarks
//! cargo bench -p switchy_database
//!
//! # Run specific benchmark group
//! cargo bench -p switchy_database -- crud/
//! cargo bench -p switchy_database -- transactions/
//! cargo bench -p switchy_database -- schema/
//! cargo bench -p switchy_database -- scale/
//!
//! # Compare specific backends
//! cargo bench -p switchy_database -- sqlite
//! cargo bench -p switchy_database -- postgres
//! cargo bench -p switchy_database -- mysql
//! ```
//!
//! ## Environment Variables
//!
//! For PostgreSQL benchmarks, set:
//! - `POSTGRES_BENCH_URL`: PostgreSQL connection URL
//!   (e.g., `postgres://user:pass@localhost:5432/benchdb`)
//!
//! For MySQL benchmarks, set:
//! - `MYSQL_BENCH_URL`: MySQL connection URL
//!   (e.g., `mysql://user:pass@localhost:3306/benchdb`)
//!
//! ## HTML Reports
//!
//! After running benchmarks, open `target/criterion/report/index.html` for
//! detailed performance analysis with graphs.

#![allow(clippy::missing_panics_doc)]

use std::sync::{Arc, OnceLock};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use switchy_database::query::FilterableQuery as _;
use tokio::runtime::Runtime;

mod setup;
use setup::{BackendInfo, Db, db_ref, init_backends, unique_table_name};

/// Global runtime and backends shared across all benchmarks.
/// This avoids issues with sqlx pools being tied to specific runtimes.
struct BenchState {
    rt: Runtime,
    backends: Vec<BackendInfo>,
}

static BENCH_STATE: OnceLock<BenchState> = OnceLock::new();

fn get_bench_state() -> &'static BenchState {
    BENCH_STATE.get_or_init(|| {
        let rt = Runtime::new().expect("Failed to create Tokio runtime");
        let backends = init_backends(&rt);
        BenchState { rt, backends }
    })
}

// ============================================================================
// CRUD Benchmarks
// ============================================================================

fn bench_insert_single(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        eprintln!("No backends available for benchmarking");
        return;
    }

    let mut group = c.benchmark_group("crud/insert_single");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_insert");

        // Setup: create table
        rt.block_on(async {
            db.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL, value INTEGER)"
            ))
            .await
            .expect("Failed to create table");
        });

        group.bench_function(BenchmarkId::new(*name, "row"), |b| {
            let mut counter = 0i64;
            b.to_async(rt).iter(|| {
                counter += 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.insert(&table)
                        .value("name", format!("user_{counter}"))
                        .value("value", counter)
                        .execute(&**db)
                        .await
                        .expect("Insert failed");
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_select_by_pk(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("crud/select_by_pk");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_select_pk");

        // Setup: create table and insert test data
        rt.block_on(async {
            let db_r = db_ref(db);
            db_r.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
            ))
            .await
            .expect("Failed to create table");

            // Insert 100 rows for selection
            for i in 1..=100 {
                db_r.insert(&table)
                    .value("name", format!("user_{i}"))
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        group.bench_function(BenchmarkId::new(*name, "single_row"), |b| {
            let mut id = 0i64;
            b.to_async(rt).iter(|| {
                id = (id % 100) + 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.select(&table)
                        .where_eq("id", id)
                        .execute(&**db)
                        .await
                        .expect("Select failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_select_all(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("crud/select_all");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_select_all");

        // Setup
        rt.block_on(async {
            let db_r = db_ref(db);
            db_r.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
            ))
            .await
            .expect("Failed to create table");

            for i in 1..=100 {
                db_r.insert(&table)
                    .value("name", format!("user_{i}"))
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        group.throughput(Throughput::Elements(100));
        group.bench_function(BenchmarkId::new(*name, "100_rows"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.select(&table)
                        .execute(&**db)
                        .await
                        .expect("Select failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_update(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("crud/update");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_update");

        // Setup
        rt.block_on(async {
            let db_r = db_ref(db);
            db_r.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL, counter INTEGER DEFAULT 0)"
            ))
            .await
            .expect("Failed to create table");

            for i in 1..=100 {
                db_r.insert(&table)
                    .value("name", format!("user_{i}"))
                    .value("counter", 0i64)
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        group.bench_function(BenchmarkId::new(*name, "single_row"), |b| {
            let mut id = 0i64;
            let mut counter = 0i64;
            b.to_async(rt).iter(|| {
                id = (id % 100) + 1;
                counter += 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.update(&table)
                        .value("counter", counter)
                        .where_eq("id", id)
                        .execute(&**db)
                        .await
                        .expect("Update failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_delete(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("crud/delete");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_delete");

        // Setup: create table and insert many rows (we'll delete them one by one)
        rt.block_on(async {
            let db_r = db_ref(db);
            db_r.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
            ))
            .await
            .expect("Failed to create table");

            // Insert enough rows for the benchmark
            for i in 0..1000 {
                db_r.insert(&table)
                    .value("name", format!("to_delete_{i}"))
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        group.bench_function(BenchmarkId::new(*name, "single_row"), |b| {
            let mut counter = 0i64;
            b.to_async(rt).iter(|| {
                counter += 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    // Delete a specific row by name
                    db.delete(&table)
                        .where_eq("name", format!("to_delete_{}", counter % 1000))
                        .execute(&**db)
                        .await
                        .expect("Delete failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_upsert(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("crud/upsert");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_upsert");

        // Setup
        rt.block_on(async {
            let db_r = db_ref(db);
            db_r.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL, counter INTEGER DEFAULT 0)"
            ))
            .await
            .expect("Failed to create table");

            // Insert initial rows
            for i in 1..=50 {
                db_r.insert(&table)
                    .value("name", format!("user_{i}"))
                    .value("counter", 0i64)
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        group.bench_function(BenchmarkId::new(*name, "existing_row"), |b| {
            let mut id = 0i64;
            let mut counter = 0i64;
            b.to_async(rt).iter(|| {
                id = (id % 50) + 1;
                counter += 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.upsert(&table)
                        .value("name", format!("user_{id}"))
                        .value("counter", counter)
                        .where_eq("id", id)
                        .execute(&**db)
                        .await
                        .expect("Upsert failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

// ============================================================================
// Transaction Benchmarks
// ============================================================================

fn bench_transaction_commit(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("transactions/commit");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_tx_commit");

        // Setup
        rt.block_on(async {
            db.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
            ))
            .await
            .expect("Failed to create table");
        });

        group.bench_function(BenchmarkId::new(*name, "with_insert"), |b| {
            let mut counter = 0i64;
            b.to_async(rt).iter(|| {
                counter += 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    let tx = db.begin_transaction().await.expect("Failed to begin tx");
                    tx.insert(&table)
                        .value("name", format!("tx_user_{counter}"))
                        .execute(&*tx)
                        .await
                        .expect("Insert in tx failed");
                    tx.commit().await.expect("Commit failed");
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_transaction_rollback(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("transactions/rollback");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_tx_rollback");

        // Setup
        rt.block_on(async {
            db.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
            ))
            .await
            .expect("Failed to create table");
        });

        group.bench_function(BenchmarkId::new(*name, "with_insert"), |b| {
            let mut counter = 0i64;
            b.to_async(rt).iter(|| {
                counter += 1;
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    let tx = db.begin_transaction().await.expect("Failed to begin tx");
                    tx.insert(&table)
                        .value("name", format!("rollback_user_{counter}"))
                        .execute(&*tx)
                        .await
                        .expect("Insert in tx failed");
                    tx.rollback().await.expect("Rollback failed");
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

fn bench_transaction_empty(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("transactions/overhead");

    for BackendInfo { name, db } in backends {
        group.bench_function(BenchmarkId::new(*name, "begin_commit"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                async move {
                    let tx = db.begin_transaction().await.expect("Failed to begin tx");
                    tx.commit().await.expect("Commit failed");
                }
            });
        });
    }

    group.finish();
}

// ============================================================================
// Schema Benchmarks
// ============================================================================

#[cfg(feature = "schema")]
fn bench_table_exists(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("schema/table_exists");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_exists");

        // Setup
        rt.block_on(async {
            db.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY)"
            ))
            .await
            .expect("Failed to create table");
        });

        group.bench_function(BenchmarkId::new(*name, "existing"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move { db.table_exists(&table).await.expect("table_exists failed") }
            });
        });

        group.bench_function(BenchmarkId::new(*name, "non_existing"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                async move {
                    db.table_exists("nonexistent_table_xyz")
                        .await
                        .expect("table_exists failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

#[cfg(feature = "schema")]
fn bench_column_exists(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("schema/column_exists");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_col_exists");

        // Setup
        rt.block_on(async {
            db.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT, email TEXT)"
            ))
            .await
            .expect("Failed to create table");
        });

        group.bench_function(BenchmarkId::new(*name, "existing"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.column_exists(&table, "name")
                        .await
                        .expect("column_exists failed")
                }
            });
        });

        group.bench_function(BenchmarkId::new(*name, "non_existing"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.column_exists(&table, "nonexistent_column")
                        .await
                        .expect("column_exists failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

#[cfg(feature = "schema")]
fn bench_get_table_info(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("schema/get_table_info");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_table_info");

        // Setup: create table with multiple columns
        rt.block_on(async {
            db.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    email TEXT,
                    age INTEGER,
                    created_at TEXT
                )"
            ))
            .await
            .expect("Failed to create table");
        });

        group.bench_function(BenchmarkId::new(*name, "5_columns"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.get_table_info(&table)
                        .await
                        .expect("get_table_info failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

// ============================================================================
// Scale Benchmarks
// ============================================================================

fn bench_bulk_insert(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("scale/bulk_insert");
    group.sample_size(20); // Fewer samples for expensive operations

    for size in [10, 100, 500] {
        for BackendInfo { name, db } in backends {
            let table = unique_table_name(&format!("bench_bulk_{size}"));

            // Setup: create table
            rt.block_on(async {
                let db_r = db_ref(db);
                let _ = db_r.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
                db_r.exec_raw(&format!(
                    "CREATE TABLE {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL, value INTEGER)"
                ))
                .await
                .expect("Failed to create table");
            });

            group.throughput(Throughput::Elements(size as u64));
            group.bench_function(BenchmarkId::new(*name, size), |b| {
                let mut batch = 0i64;
                b.to_async(rt).iter(|| {
                    batch += 1;
                    let db: Db = Arc::clone(db);
                    let table = table.clone();
                    async move {
                        for i in 0..size {
                            db.insert(&table)
                                .value("name", format!("bulk_user_{batch}_{i}"))
                                .value("value", i as i64)
                                .execute(&**db)
                                .await
                                .expect("Bulk insert failed");
                        }
                    }
                });
            });

            // Cleanup
            rt.block_on(async {
                let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
            });
        }
    }

    group.finish();
}

fn bench_large_result_set(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("scale/large_select");
    group.sample_size(20);

    for size in [100, 500, 1000] {
        for BackendInfo { name, db } in backends {
            let table = unique_table_name(&format!("bench_large_{size}"));

            // Setup: create and populate table
            rt.block_on(async {
                let db_r = db_ref(db);
                let _ = db_r.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
                db_r.exec_raw(&format!(
                    "CREATE TABLE {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL, value INTEGER)"
                ))
                .await
                .expect("Failed to create table");

                for i in 0..size {
                    db_r.insert(&table)
                        .value("name", format!("user_{i}"))
                        .value("value", i as i64)
                        .execute(db_r)
                        .await
                        .expect("Insert failed");
                }
            });

            group.throughput(Throughput::Elements(size as u64));
            group.bench_function(BenchmarkId::new(*name, size), |b| {
                b.to_async(rt).iter(|| {
                    let db: Db = Arc::clone(db);
                    let table = table.clone();
                    async move {
                        db.select(&table)
                            .execute(&**db)
                            .await
                            .expect("Select failed")
                    }
                });
            });

            // Cleanup
            rt.block_on(async {
                let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
            });
        }
    }

    group.finish();
}

fn bench_filtered_select(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("scale/filtered_select");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_filtered");

        // Setup: create and populate table with 1000 rows
        rt.block_on(async {
            let db_r = db_ref(db);
            let _ = db_r.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
            db_r.exec_raw(&format!(
                "CREATE TABLE {table} (id INTEGER PRIMARY KEY, category TEXT NOT NULL, value INTEGER)"
            ))
            .await
            .expect("Failed to create table");

            for i in 0..1000 {
                let category = format!("cat_{}", i % 10); // 10 categories
                db_r.insert(&table)
                    .value("category", category)
                    .value("value", i as i64)
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        // Select ~100 rows (10% of data)
        group.throughput(Throughput::Elements(100));
        group.bench_function(BenchmarkId::new(*name, "100_of_1000"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.select(&table)
                        .where_eq("category", "cat_0")
                        .execute(&**db)
                        .await
                        .expect("Select failed")
                }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

// ============================================================================
// Raw SQL vs Query Builder Benchmarks
// ============================================================================

fn bench_raw_vs_builder(c: &mut Criterion) {
    let state = get_bench_state();
    let rt = &state.rt;
    let backends = &state.backends;

    if backends.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("comparison/raw_vs_builder");

    for BackendInfo { name, db } in backends {
        let table = unique_table_name("bench_raw_builder");

        // Setup
        rt.block_on(async {
            let db_r = db_ref(db);
            db_r.exec_raw(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"
            ))
            .await
            .expect("Failed to create table");

            for i in 1..=100 {
                db_r.insert(&table)
                    .value("name", format!("user_{i}"))
                    .execute(db_r)
                    .await
                    .expect("Insert failed");
            }
        });

        group.bench_function(BenchmarkId::new(*name, "query_builder"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let table = table.clone();
                async move {
                    db.select(&table)
                        .where_eq("id", 50i64)
                        .execute(&**db)
                        .await
                        .expect("Select failed")
                }
            });
        });

        let raw_query = format!("SELECT * FROM {table} WHERE id = 50");
        group.bench_function(BenchmarkId::new(*name, "raw_sql"), |b| {
            b.to_async(rt).iter(|| {
                let db: Db = Arc::clone(db);
                let query = raw_query.clone();
                async move { db.query_raw(&query).await.expect("Raw query failed") }
            });
        });

        // Cleanup
        rt.block_on(async {
            let _ = db.exec_raw(&format!("DROP TABLE IF EXISTS {table}")).await;
        });
    }

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    crud_benches,
    bench_insert_single,
    bench_select_by_pk,
    bench_select_all,
    bench_update,
    bench_delete,
    bench_upsert,
);

criterion_group!(
    transaction_benches,
    bench_transaction_commit,
    bench_transaction_rollback,
    bench_transaction_empty,
);

#[cfg(feature = "schema")]
criterion_group!(
    schema_benches,
    bench_table_exists,
    bench_column_exists,
    bench_get_table_info,
);

criterion_group!(
    scale_benches,
    bench_bulk_insert,
    bench_large_result_set,
    bench_filtered_select,
);

criterion_group!(comparison_benches, bench_raw_vs_builder,);

#[cfg(feature = "schema")]
criterion_main!(
    crud_benches,
    transaction_benches,
    schema_benches,
    scale_benches,
    comparison_benches,
);

#[cfg(not(feature = "schema"))]
criterion_main!(
    crud_benches,
    transaction_benches,
    scale_benches,
    comparison_benches,
);
