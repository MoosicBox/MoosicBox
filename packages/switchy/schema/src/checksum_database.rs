#![allow(clippy::items_after_test_module)]

use crate::digest::Digest;
use async_trait::async_trait;
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use switchy_async::sync::Mutex;
use switchy_database::query::{
    DeleteStatement, Expression, ExpressionType, InsertStatement, SelectQuery, UpdateStatement,
    UpsertMultiStatement, UpsertStatement,
};
use switchy_database::{Database, DatabaseError, DatabaseTransaction, DatabaseValue, Row};

#[derive(Debug)]
pub struct ChecksumDatabase {
    hasher: Arc<Mutex<Sha256>>,
    transaction_depth: Arc<AtomicUsize>,
}

impl Default for ChecksumDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl ChecksumDatabase {
    #[must_use]
    pub fn new() -> Self {
        Self {
            hasher: Arc::new(Mutex::new(Sha256::new())),
            transaction_depth: Arc::new(AtomicUsize::new(0)),
        }
    }

    const fn with_hasher(hasher: Arc<Mutex<Sha256>>, transaction_depth: Arc<AtomicUsize>) -> Self {
        Self {
            hasher,
            transaction_depth,
        }
    }

    pub async fn finalize(self) -> bytes::Bytes {
        match Arc::try_unwrap(self.hasher) {
            Ok(mutex) => {
                let hasher = mutex.into_inner();
                bytes::Bytes::from(hasher.finalize().to_vec())
            }
            Err(arc) => {
                let hasher = arc.lock().await;
                let cloned = hasher.clone();
                drop(hasher);
                bytes::Bytes::from(cloned.finalize().to_vec())
            }
        }
    }
}

#[async_trait]
impl Database for ChecksumDatabase {
    // Query builders use default implementations
    // fn select, update, insert, etc. return query builders

    async fn query(
        &self,
        query: &switchy_database::query::SelectQuery<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"QUERY:");
        query.update_digest(&mut hasher);
        drop(hasher);
        Ok(vec![])
    }

    async fn query_first(
        &self,
        query: &switchy_database::query::SelectQuery<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"QUERY_FIRST:");
        query.update_digest(&mut hasher);
        drop(hasher);
        Ok(None)
    }

    async fn exec_update(
        &self,
        statement: &switchy_database::query::UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"UPDATE:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(vec![])
    }

    async fn exec_update_first(
        &self,
        statement: &switchy_database::query::UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"UPDATE_FIRST:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(None)
    }

    async fn exec_insert(
        &self,
        statement: &switchy_database::query::InsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"INSERT:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(Row { columns: vec![] }) // Empty row using known struct layout
    }

    async fn exec_upsert(
        &self,
        statement: &switchy_database::query::UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"UPSERT:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(vec![])
    }

    async fn exec_upsert_first(
        &self,
        statement: &switchy_database::query::UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"UPSERT_FIRST:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(Row { columns: vec![] })
    }

    async fn exec_upsert_multi(
        &self,
        statement: &switchy_database::query::UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"UPSERT_MULTI:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(vec![])
    }

    async fn exec_delete(
        &self,
        statement: &switchy_database::query::DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"DELETE:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(vec![])
    }

    async fn exec_delete_first(
        &self,
        statement: &switchy_database::query::DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"DELETE_FIRST:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(None)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"EXEC_RAW:");
        hasher.update(statement.as_bytes());
        drop(hasher);
        Ok(())
    }

    fn trigger_close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn exec_create_table(
        &self,
        statement: &switchy_database::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"CREATE_TABLE:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(())
    }

    async fn exec_drop_table(
        &self,
        statement: &switchy_database::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"DROP_TABLE:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(())
    }

    async fn exec_create_index(
        &self,
        statement: &switchy_database::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"CREATE_INDEX:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(())
    }

    async fn exec_drop_index(
        &self,
        statement: &switchy_database::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"DROP_INDEX:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(())
    }

    async fn exec_alter_table(
        &self,
        statement: &switchy_database::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        let mut hasher = self.hasher.lock().await;
        hasher.update(b"ALTER_TABLE:");
        statement.update_digest(&mut hasher);
        drop(hasher);
        Ok(())
    }

    async fn table_exists(&self, _table_name: &str) -> Result<bool, DatabaseError> {
        // TODO: Implement checksum tracking for table existence checking
        unimplemented!("table_exists not yet implemented for ChecksumDatabase")
    }

    async fn get_table_info(
        &self,
        _table_name: &str,
    ) -> Result<Option<switchy_database::schema::TableInfo>, DatabaseError> {
        // TODO: Implement checksum tracking for table info retrieval
        unimplemented!("get_table_info not yet implemented for ChecksumDatabase")
    }

    async fn get_table_columns(
        &self,
        _table_name: &str,
    ) -> Result<Vec<switchy_database::schema::ColumnInfo>, DatabaseError> {
        // TODO: Implement checksum tracking for column info retrieval
        unimplemented!("get_table_columns not yet implemented for ChecksumDatabase")
    }

    async fn column_exists(
        &self,
        _table_name: &str,
        _column_name: &str,
    ) -> Result<bool, DatabaseError> {
        // TODO: Implement checksum tracking for column existence checking
        unimplemented!("column_exists not yet implemented for ChecksumDatabase")
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let depth = self.transaction_depth.fetch_add(1, Ordering::SeqCst);
        let mut hasher = self.hasher.lock().await;
        if depth > 0 {
            hasher.update(format!("D{}:", depth + 1).as_bytes());
        }
        hasher.update(b"BEGIN_TRANSACTION:");
        drop(hasher);

        let tx = Self::with_hasher(self.hasher.clone(), self.transaction_depth.clone());
        Ok(Box::new(tx))
    }
}

#[async_trait]
impl DatabaseTransaction for ChecksumDatabase {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        self.transaction_depth.fetch_sub(1, Ordering::SeqCst);
        self.hasher.lock().await.update(b"COMMIT:");
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        self.transaction_depth.fetch_sub(1, Ordering::SeqCst);
        self.hasher.lock().await.update(b"ROLLBACK:");
        Ok(())
    }
}

#[must_use]
pub fn calculate_hash(content: &str) -> bytes::Bytes {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    bytes::Bytes::from(hasher.finalize().to_vec())
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use switchy_database::DatabaseValue;

    #[switchy_async::test]
    async fn test_same_operations_produce_identical_checksums() {
        let db1 = ChecksumDatabase::new();
        let db2 = ChecksumDatabase::new();

        // Perform identical operations on both
        let _ = db1.exec_raw("CREATE TABLE test").await;
        let _ = db2.exec_raw("CREATE TABLE test").await;

        let checksum1 = db1.finalize().await;
        let checksum2 = db2.finalize().await;

        assert_eq!(
            checksum1, checksum2,
            "Same operations should produce identical checksums"
        );
    }

    #[switchy_async::test]
    async fn test_different_operations_produce_different_checksums() {
        let db1 = ChecksumDatabase::new();
        let db2 = ChecksumDatabase::new();

        // Perform different operations
        let _ = db1.exec_raw("CREATE TABLE test1").await;
        let _ = db2.exec_raw("CREATE TABLE test2").await;

        let checksum1 = db1.finalize().await;
        let checksum2 = db2.finalize().await;

        assert_ne!(
            checksum1, checksum2,
            "Different operations should produce different checksums"
        );
    }

    #[switchy_async::test]
    async fn test_transaction_patterns_produce_different_checksums() {
        let db1 = ChecksumDatabase::new();
        let db2 = ChecksumDatabase::new();

        // Transaction with commit
        let tx1 = db1.begin_transaction().await.unwrap();
        let _ = tx1.commit().await;

        // Transaction with rollback
        let tx2 = db2.begin_transaction().await.unwrap();
        let _ = tx2.rollback().await;

        let checksum1 = db1.finalize().await;
        let checksum2 = db2.finalize().await;

        assert_ne!(
            checksum1, checksum2,
            "Commit vs rollback should produce different checksums"
        );
    }

    #[switchy_async::test]
    async fn test_graceful_finalize_with_multiple_arc_references() {
        let db = ChecksumDatabase::new();

        // Create multiple references to the hasher
        let hasher_ref = db.hasher.clone();

        // This should not panic even with multiple Arc references
        let checksum = db.finalize().await;

        assert_eq!(checksum.len(), 32, "Checksum should be exactly 32 bytes");

        // Verify the reference is still valid
        let _guard = hasher_ref.lock().await;
    }

    #[switchy_async::test]
    async fn test_shared_hasher_between_parent_and_transaction() {
        let db = ChecksumDatabase::new();

        // Perform operation on parent
        let _ = db.exec_raw("PARENT_OP").await;

        // Begin transaction and perform operation
        let tx = db.begin_transaction().await.unwrap();
        let _ = tx.exec_raw("TX_OP").await;
        let _ = tx.commit().await;

        // Perform another operation on parent
        let _ = db.exec_raw("PARENT_OP2").await;

        let checksum = db.finalize().await;

        // Create another database with the same sequence of operations
        let db2 = ChecksumDatabase::new();
        let _ = db2.exec_raw("PARENT_OP").await;

        let tx2 = db2.begin_transaction().await.unwrap();
        let _ = tx2.exec_raw("TX_OP").await;
        let _ = tx2.commit().await;

        let _ = db2.exec_raw("PARENT_OP2").await;

        let checksum2 = db2.finalize().await;

        assert_eq!(
            checksum, checksum2,
            "Parent and transaction operations should produce consistent checksums"
        );
    }

    #[switchy_async::test]
    async fn test_same_operations_with_without_transactions_differ() {
        // Without transaction
        let db1 = ChecksumDatabase::new();
        db1.exec_raw("INSERT INTO test VALUES (1)").await.unwrap();
        let checksum1 = db1.finalize().await;

        // With transaction
        let db2 = ChecksumDatabase::new();
        let tx = db2.begin_transaction().await.unwrap();
        tx.exec_raw("INSERT INTO test VALUES (1)").await.unwrap();
        tx.commit().await.unwrap();
        let checksum2 = db2.finalize().await;

        assert_ne!(
            checksum1, checksum2,
            "Transaction wrapper should affect checksum - same operations should produce different hashes"
        );

        // Verify both are valid 32-byte checksums
        assert_eq!(checksum1.len(), 32);
        assert_eq!(checksum2.len(), 32);

        // Test rollback produces different checksum than commit
        let db3 = ChecksumDatabase::new();
        let tx = db3.begin_transaction().await.unwrap();
        tx.exec_raw("INSERT INTO test VALUES (1)").await.unwrap();
        tx.rollback().await.unwrap();
        let checksum3 = db3.finalize().await;

        assert_ne!(
            checksum2, checksum3,
            "Commit vs rollback should produce different checksums"
        );
        assert_ne!(
            checksum1, checksum3,
            "Direct operation vs transaction rollback should produce different checksums"
        );
    }

    #[test]
    fn test_database_value_digest_coverage() {
        use sha2::{Digest as _, Sha256};

        let mut hasher = Sha256::new();

        // Test all DatabaseValue variants
        DatabaseValue::Null.update_digest(&mut hasher);
        DatabaseValue::String("test".to_string()).update_digest(&mut hasher);
        DatabaseValue::StringOpt(Some("test".to_string())).update_digest(&mut hasher);
        DatabaseValue::StringOpt(None).update_digest(&mut hasher);
        DatabaseValue::Bool(true).update_digest(&mut hasher);
        DatabaseValue::BoolOpt(Some(false)).update_digest(&mut hasher);
        DatabaseValue::BoolOpt(None).update_digest(&mut hasher);
        DatabaseValue::Number(42).update_digest(&mut hasher);
        DatabaseValue::NumberOpt(Some(42)).update_digest(&mut hasher);
        DatabaseValue::NumberOpt(None).update_digest(&mut hasher);
        DatabaseValue::UNumber(42u64).update_digest(&mut hasher);
        DatabaseValue::UNumberOpt(Some(42u64)).update_digest(&mut hasher);
        DatabaseValue::UNumberOpt(None).update_digest(&mut hasher);
        DatabaseValue::Real(std::f64::consts::PI).update_digest(&mut hasher);
        DatabaseValue::RealOpt(Some(std::f64::consts::PI)).update_digest(&mut hasher);
        DatabaseValue::RealOpt(None).update_digest(&mut hasher);
        DatabaseValue::Now.update_digest(&mut hasher);
        DatabaseValue::NowAdd("1 day".to_string()).update_digest(&mut hasher);

        let checksum = hasher.finalize();

        // Verify we got a valid checksum
        assert_eq!(checksum.len(), 32, "Checksum should be 32 bytes");
    }

    #[test]
    fn test_calculate_hash_function() {
        let hash1 = calculate_hash("test content");
        let hash2 = calculate_hash("test content");
        let hash3 = calculate_hash("different content");

        assert_eq!(hash1, hash2, "Same content should produce same hash");
        assert_ne!(
            hash1, hash3,
            "Different content should produce different hash"
        );
        assert_eq!(hash1.len(), 32, "Hash should be 32 bytes");
    }

    #[switchy_async::test]
    async fn test_all_database_methods_implemented() {
        let db = ChecksumDatabase::new();

        // Test basic query methods
        let query = db.select("test_table");
        let _ = db.query(&query).await.unwrap();
        let _ = db.query_first(&query).await.unwrap();

        // Test update methods
        let update = db.update("test_table");
        let _ = db.exec_update(&update).await.unwrap();
        let _ = db.exec_update_first(&update).await.unwrap();

        // Test insert methods
        let insert = db.insert("test_table");
        let row = db.exec_insert(&insert).await.unwrap();
        assert_eq!(row.columns.len(), 0, "Insert should return empty row");

        // Test upsert methods
        let upsert = db.upsert("test_table");
        let _ = db.exec_upsert(&upsert).await.unwrap();
        let row = db.exec_upsert_first(&upsert).await.unwrap();
        assert_eq!(row.columns.len(), 0, "Upsert should return empty row");

        let upsert_multi = db.upsert_multi("test_table");
        let _ = db.exec_upsert_multi(&upsert_multi).await.unwrap();

        // Test delete methods
        let delete = db.delete("test_table");
        let _ = db.exec_delete(&delete).await.unwrap();
        let _ = db.exec_delete_first(&delete).await.unwrap();

        // Test raw execution
        db.exec_raw("SELECT 1").await.unwrap();

        // Test close methods
        db.trigger_close().unwrap();
        db.close().await.unwrap();

        // Test schema methods
        let create_table = db.create_table("test_table");
        db.exec_create_table(&create_table).await.unwrap();

        let drop_table = db.drop_table("test_table");
        db.exec_drop_table(&drop_table).await.unwrap();

        let create_index = db.create_index("test_index");
        db.exec_create_index(&create_index).await.unwrap();

        let drop_index = db.drop_index("test_index", "test_table");
        db.exec_drop_index(&drop_index).await.unwrap();

        let alter_table = db.alter_table("test_table");
        db.exec_alter_table(&alter_table).await.unwrap();

        // Test transaction
        let tx = db.begin_transaction().await.unwrap();
        tx.commit().await.unwrap();

        let checksum = db.finalize().await;
        assert_eq!(checksum.len(), 32, "Checksum should be 32 bytes");
    }

    #[switchy_async::test]
    async fn test_row_construction() {
        let db = ChecksumDatabase::new();
        let insert = db.insert("test_table");
        let row = db.exec_insert(&insert).await.unwrap();

        // Verify Row construction works as expected
        assert_eq!(row.columns.len(), 0);
        assert_eq!(row.columns, vec![]);
    }

    #[switchy_async::test]
    async fn test_transaction_digest_updates() {
        let db = ChecksumDatabase::new();

        // Capture initial state
        let initial_db = ChecksumDatabase::new();
        let initial_checksum = initial_db.finalize().await;

        // Begin transaction (should update digest)
        let tx = db.begin_transaction().await.unwrap();

        // Commit (should update digest differently than rollback)
        tx.commit().await.unwrap();

        let final_checksum = db.finalize().await;

        assert_ne!(
            initial_checksum, final_checksum,
            "Transaction operations should change checksum"
        );
    }

    #[switchy_async::test]
    async fn test_nested_transactions_produce_different_checksums() {
        let db1 = ChecksumDatabase::new();
        let tx1 = db1.begin_transaction().await.unwrap();
        tx1.commit().await.unwrap();
        let checksum1 = db1.finalize().await;

        let db2 = ChecksumDatabase::new();
        let tx1 = db2.begin_transaction().await.unwrap();
        let tx2 = tx1.begin_transaction().await.unwrap();
        tx2.commit().await.unwrap();
        tx1.commit().await.unwrap();
        let checksum2 = db2.finalize().await;

        assert_ne!(
            checksum1, checksum2,
            "Single vs nested transactions should produce different checksums"
        );
    }
}

// Digest implementations for database types
impl Digest for DatabaseValue {
    fn update_digest(&self, hasher: &mut Sha256) {
        match self {
            Self::Null => hasher.update(b"NULL"),
            Self::String(s) => {
                hasher.update(b"STR:");
                hasher.update(s.as_bytes());
            }
            Self::StringOpt(opt) => {
                hasher.update(b"STROPT:");
                if let Some(s) = opt {
                    hasher.update(s.as_bytes());
                } else {
                    hasher.update(b"NONE");
                }
            }
            Self::Bool(b) => {
                hasher.update(b"BOOL:");
                hasher.update([u8::from(*b)]);
            }
            Self::BoolOpt(opt) => {
                hasher.update(b"BOOLOPT:");
                if let Some(b) = opt {
                    hasher.update([u8::from(*b)]);
                } else {
                    hasher.update(b"NONE");
                }
            }
            Self::Number(n) => {
                hasher.update(b"NUM:");
                hasher.update(n.to_le_bytes());
            }
            Self::NumberOpt(opt) => {
                hasher.update(b"NUMOPT:");
                if let Some(n) = opt {
                    hasher.update(n.to_le_bytes());
                } else {
                    hasher.update(b"NONE");
                }
            }
            Self::UNumber(n) => {
                hasher.update(b"UNUM:");
                hasher.update(n.to_le_bytes());
            }
            Self::UNumberOpt(opt) => {
                hasher.update(b"UNUMOPT:");
                if let Some(n) = opt {
                    hasher.update(n.to_le_bytes());
                } else {
                    hasher.update(b"NONE");
                }
            }
            Self::Real(r) => {
                hasher.update(b"REAL:");
                hasher.update(r.to_le_bytes());
            }
            Self::RealOpt(opt) => {
                hasher.update(b"REALOPT:");
                if let Some(r) = opt {
                    hasher.update(r.to_le_bytes());
                } else {
                    hasher.update(b"NONE");
                }
            }
            Self::Now => hasher.update(b"NOW"),
            Self::NowAdd(s) => {
                hasher.update(b"NOWADD:");
                hasher.update(s.as_bytes());
            }
            Self::DateTime(dt) => {
                hasher.update(b"DT:");
                hasher.update(dt.to_string().as_bytes());
            }
        }
    }
}

// Digest implementation for ExpressionType enum
impl Digest for ExpressionType<'_> {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn update_digest(&self, hasher: &mut Sha256) {
        match self {
            ExpressionType::Eq(expr) => {
                hasher.update(b"EQ:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Gt(expr) => {
                hasher.update(b"GT:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Lt(expr) => {
                hasher.update(b"LT:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Gte(expr) => {
                hasher.update(b"GTE:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Lte(expr) => {
                hasher.update(b"LTE:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::NotEq(expr) => {
                hasher.update(b"NOTEQ:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::In(expr) => {
                hasher.update(b"IN:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::NotIn(expr) => {
                hasher.update(b"NOTIN:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::InList(expr) => {
                hasher.update(b"INLIST:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Or(expr) => {
                hasher.update(b"OR:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::And(expr) => {
                hasher.update(b"AND:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Join(expr) => {
                hasher.update(b"JOIN:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Sort(expr) => {
                hasher.update(b"SORT:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Literal(expr) => {
                hasher.update(b"LITERAL:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Coalesce(expr) => {
                hasher.update(b"COALESCE:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::Identifier(expr) => {
                hasher.update(b"IDENTIFIER:");
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
            ExpressionType::SelectQuery(query) => {
                hasher.update(b"SUBQUERY:");
                query.update_digest(hasher);
            }
            ExpressionType::DatabaseValue(value) => {
                hasher.update(b"VALUE:");
                value.update_digest(hasher);
            }
        }
    }
}

impl Digest for SelectQuery<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"SELECT:");

        if self.distinct {
            hasher.update(b"DISTINCT");
        }

        // Sort columns for deterministic order
        let mut columns: Vec<_> = self.columns.iter().collect();
        columns.sort();

        for col in columns {
            hasher.update(b"COLUMN:");
            hasher.update(col.as_bytes());
        }

        hasher.update(b"FROM:");
        hasher.update(self.table_name.as_bytes());

        if let Some(filters) = &self.filters {
            hasher.update(b"WHERE:");
            // Sort filters for deterministic order
            let mut sorted_filters: Vec<_> = filters.iter().collect();
            sorted_filters.sort_by_key(|f| format!("{f:?}"));
            for filter in sorted_filters {
                hasher.update(b"FILTER:");
                if let Some(values) = filter.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
        }

        if let Some(joins) = &self.joins {
            hasher.update(b"JOINS:");
            // Sort joins for deterministic order
            let mut sorted_joins: Vec<_> = joins.iter().collect();
            sorted_joins.sort_by_key(|j| format!("{j:?}"));
            for join in sorted_joins {
                hasher.update(b"JOIN:");
                hasher.update(format!("{join:?}").as_bytes());
            }
        }

        if let Some(sorts) = &self.sorts {
            hasher.update(b"ORDERBY:");
            // Sort order by clauses for deterministic order
            let mut sorted_order: Vec<_> = sorts.iter().collect();
            sorted_order.sort_by_key(|o| format!("{o:?}"));
            for order in sorted_order {
                hasher.update(b"ORDER:");
                hasher.update(format!("{order:?}").as_bytes());
            }
        }

        if let Some(limit) = &self.limit {
            hasher.update(b"LIMIT:");
            hasher.update(limit.to_le_bytes());
        }
    }
}

impl Digest for UpdateStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"UPDATE:");
        hasher.update(self.table_name.as_bytes());

        hasher.update(b"SET:");
        // Sort values for deterministic order
        let mut sorted_values: BTreeMap<&str, _> = BTreeMap::new();
        for (k, v) in &self.values {
            sorted_values.insert(k, v);
        }
        for (key, value) in sorted_values {
            hasher.update(b"SETCLAUSE:");
            hasher.update(key.as_bytes());
            hasher.update(b"=");
            if let Some(values) = value.values() {
                for val in values {
                    val.update_digest(hasher);
                }
            }
        }

        if let Some(filters) = &self.filters {
            hasher.update(b"WHERE:");
            let mut sorted_filters: Vec<_> = filters.iter().collect();
            sorted_filters.sort_by_key(|f| format!("{f:?}"));
            for filter in sorted_filters {
                hasher.update(b"FILTER:");
                if let Some(values) = filter.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
        }

        if let Some(unique) = &self.unique {
            hasher.update(b"UNIQUE:");
            let mut sorted_unique: Vec<_> = unique.iter().collect();
            sorted_unique.sort();
            for col in sorted_unique {
                hasher.update(col.as_bytes());
            }
        }

        if let Some(limit) = &self.limit {
            hasher.update(b"LIMIT:");
            hasher.update(limit.to_le_bytes());
        }
    }
}

impl Digest for InsertStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"INSERT:");
        hasher.update(self.table_name.as_bytes());

        hasher.update(b"VALUES:");
        // Sort values for deterministic order
        let mut sorted_values: BTreeMap<&str, _> = BTreeMap::new();
        for (k, v) in &self.values {
            sorted_values.insert(k, v);
        }
        for (key, value) in sorted_values {
            hasher.update(key.as_bytes());
            hasher.update(b"=");
            if let Some(values) = value.values() {
                for val in values {
                    val.update_digest(hasher);
                }
            }
        }
    }
}

impl Digest for UpsertStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"UPSERT:");
        hasher.update(self.table_name.as_bytes());

        hasher.update(b"VALUES:");
        // Sort values for deterministic order
        let mut sorted_values: BTreeMap<&str, _> = BTreeMap::new();
        for (k, v) in &self.values {
            sorted_values.insert(k, v);
        }
        for (key, value) in sorted_values {
            hasher.update(key.as_bytes());
            hasher.update(b"=");
            if let Some(values) = value.values() {
                for val in values {
                    val.update_digest(hasher);
                }
            }
        }

        if let Some(filters) = &self.filters {
            hasher.update(b"WHERE:");
            let mut sorted_filters: Vec<_> = filters.iter().collect();
            sorted_filters.sort_by_key(|f| format!("{f:?}"));
            for filter in sorted_filters {
                hasher.update(b"FILTER:");
                if let Some(values) = filter.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
        }

        if let Some(unique) = &self.unique {
            hasher.update(b"UNIQUE:");
            let mut sorted_unique: Vec<_> = unique.iter().collect();
            sorted_unique.sort();
            for col in sorted_unique {
                hasher.update(col.as_bytes());
            }
        }

        if let Some(limit) = &self.limit {
            hasher.update(b"LIMIT:");
            hasher.update(limit.to_le_bytes());
        }
    }
}

impl Digest for UpsertMultiStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"UPSERT_MULTI:");
        hasher.update(self.table_name.as_bytes());

        // Process multiple value sets with deterministic ordering
        hasher.update(b"VALUES:");
        for values in &self.values {
            hasher.update(b"VALUESET:");
            let mut sorted_values: BTreeMap<&str, _> = BTreeMap::new();
            for (k, v) in values {
                sorted_values.insert(k, v);
            }
            for (key, value) in sorted_values {
                hasher.update(key.as_bytes());
                hasher.update(b"=");
                if let Some(values) = value.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
        }

        if let Some(unique) = &self.unique {
            hasher.update(b"UNIQUE:");
            for expr in unique {
                if let Some(values) = expr.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
        }
    }
}

impl Digest for DeleteStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"DELETE:");
        hasher.update(self.table_name.as_bytes());

        if let Some(filters) = &self.filters {
            hasher.update(b"WHERE:");
            let mut sorted_filters: Vec<_> = filters.iter().collect();
            sorted_filters.sort_by_key(|f| format!("{f:?}"));
            for filter in sorted_filters {
                hasher.update(b"FILTER:");
                if let Some(values) = filter.values() {
                    for val in values {
                        val.update_digest(hasher);
                    }
                }
            }
        }

        if let Some(limit) = &self.limit {
            hasher.update(b"LIMIT:");
            hasher.update(limit.to_le_bytes());
        }
    }
}

impl Digest for switchy_database::schema::CreateTableStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"CREATE_TABLE:");
        hasher.update(self.table_name.as_bytes());

        if self.if_not_exists {
            hasher.update(b"IFNOTEXISTS");
        }

        // Sort columns for deterministic order
        hasher.update(b"COLUMNS:");
        let mut sorted_columns: BTreeMap<String, _> = BTreeMap::new();
        for col in &self.columns {
            sorted_columns.insert(col.name.clone(), col);
        }
        for (name, column) in sorted_columns {
            hasher.update(b"COLUMN:");
            hasher.update(name.as_bytes());
            hasher.update(b"TYPE:");
            hasher.update(format!("{:?}", column.data_type).as_bytes());
            if !column.nullable {
                hasher.update(b"NOTNULL");
            }
            if column.auto_increment {
                hasher.update(b"AUTOINCREMENT");
            }
            if let Some(default) = &column.default {
                hasher.update(b"DEFAULT:");
                default.update_digest(hasher);
            }
        }

        if let Some(primary_key) = &self.primary_key {
            hasher.update(b"PRIMARYKEY:");
            hasher.update(primary_key.as_bytes());
        }

        hasher.update(b"FOREIGNKEYS:");
        let mut sorted_fks: Vec<_> = self.foreign_keys.iter().collect();
        sorted_fks.sort();
        for (col, ref_col) in sorted_fks {
            hasher.update(b"FK:");
            hasher.update(col.as_bytes());
            hasher.update(b"->");
            hasher.update(ref_col.as_bytes());
        }
    }
}

impl Digest for switchy_database::schema::DropTableStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"DROP_TABLE:");
        hasher.update(self.table_name.as_bytes());
        if self.if_exists {
            hasher.update(b"IFEXISTS");
        }
    }
}

impl Digest for switchy_database::schema::CreateIndexStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"CREATE_INDEX:");
        hasher.update(self.index_name.as_bytes());
        hasher.update(b"ON:");
        hasher.update(self.table_name.as_bytes());

        hasher.update(b"COLUMNS:");
        // Sort columns for deterministic order
        let mut sorted_columns: Vec<_> = self.columns.iter().collect();
        sorted_columns.sort();
        for col in sorted_columns {
            hasher.update(col.as_bytes());
            hasher.update(b",");
        }

        if self.unique {
            hasher.update(b"UNIQUE");
        }

        if self.if_not_exists {
            hasher.update(b"IFNOTEXISTS");
        }
    }
}

impl Digest for switchy_database::schema::DropIndexStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"DROP_INDEX:");
        hasher.update(self.index_name.as_bytes());
        hasher.update(b"ON:");
        hasher.update(self.table_name.as_bytes());
        if self.if_exists {
            hasher.update(b"IFEXISTS");
        }
    }
}

impl Digest for switchy_database::schema::AlterTableStatement<'_> {
    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(b"ALTER_TABLE:");
        hasher.update(self.table_name.as_bytes());

        // Process operations with deterministic ordering
        hasher.update(b"OPERATIONS:");
        // Sort operations for deterministic order
        let mut sorted_operations: Vec<_> = self.operations.iter().collect();
        sorted_operations.sort_by_key(|op| format!("{op:?}"));
        for operation in sorted_operations {
            hasher.update(b"OPERATION:");
            hasher.update(format!("{operation:?}").as_bytes());
        }
    }
}
