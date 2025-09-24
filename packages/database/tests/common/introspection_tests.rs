use std::sync::Arc;
use switchy_database::schema::{Column, DataType, create_table};
use switchy_database::{Database, DatabaseValue};

/// Create standard test schema using schema builder API for backend compatibility
pub async fn create_standard_test_schema(
    db: &dyn Database,
) -> Result<(), switchy_database::DatabaseError> {
    // Users table with proper auto-increment and types
    create_table("users")
        .if_not_exists(true)
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::Int,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "email".to_string(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "age".to_string(),
            data_type: DataType::Int,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "created_at".to_string(),
            data_type: DataType::Timestamp,
            nullable: true,
            auto_increment: false,
            default: Some(DatabaseValue::Now),
        })
        .primary_key("id")
        .execute(db)
        .await?;

    // Posts table with foreign key
    create_table("posts")
        .if_not_exists(true)
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::Int,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .column(Column {
            name: "title".to_string(),
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "content".to_string(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "user_id".to_string(),
            data_type: DataType::Int,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "published".to_string(),
            data_type: DataType::Bool,
            nullable: true,
            auto_increment: false,
            default: Some(DatabaseValue::Bool(false)),
        })
        .primary_key("id")
        .foreign_key(("user_id", "users(id)"))
        .execute(db)
        .await?;

    // Edge cases table with various data types
    create_table("edge_cases")
        .if_not_exists(true)
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::Int,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .column(Column {
            name: "uuid_col".to_string(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "json_col".to_string(),
            data_type: DataType::Text, // Use Text for cross-backend compatibility
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "data_col".to_string(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .primary_key("id")
        .execute(db)
        .await?;

    Ok(())
}

/// Comprehensive introspection test suite trait
#[allow(unused)]
pub trait IntrospectionTestSuite {
    type DatabaseType: Database + Send + Sync;

    /// Get database instance for testing
    async fn get_database(&self) -> Option<Arc<Self::DatabaseType>>;

    /// Create the standard test schema using schema builder API
    async fn create_test_schema(&self, db: &Self::DatabaseType) {
        // Use schema builder API for backend compatibility
        let _ = create_standard_test_schema(db).await;
    }

    /// Test table existence detection
    async fn test_table_exists(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db).await;

        // Test existing tables
        assert!(db.table_exists("users").await.unwrap());
        assert!(db.table_exists("posts").await.unwrap());
        assert!(db.table_exists("edge_cases").await.unwrap());

        // Test non-existing table
        assert!(!db.table_exists("nonexistent_table").await.unwrap());
    }

    /// Test column existence detection
    async fn test_column_exists(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db).await;

        // Test existing columns
        assert!(db.column_exists("users", "id").await.unwrap());
        assert!(db.column_exists("users", "name").await.unwrap());
        assert!(db.column_exists("users", "email").await.unwrap());
        assert!(db.column_exists("posts", "title").await.unwrap());

        // Test non-existing columns
        assert!(
            !db.column_exists("users", "nonexistent_column")
                .await
                .unwrap()
        );
        assert!(!db.column_exists("nonexistent_table", "id").await.unwrap());
    }

    /// Test getting table columns with metadata
    async fn test_get_table_columns(&self) {
        use switchy_database::schema::DataType;

        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db).await;

        // Test users table columns
        let columns = db.get_table_columns("users").await.unwrap();
        assert!(!columns.is_empty());

        // Find specific columns
        let id_col = columns.iter().find(|c| c.name == "id").unwrap();
        let name_col = columns.iter().find(|c| c.name == "name").unwrap();
        let email_col = columns.iter().find(|c| c.name == "email").unwrap();

        // Verify column properties
        assert_eq!(id_col.name, "id");
        assert_eq!(name_col.name, "name");
        assert!(!name_col.nullable);
        assert_eq!(email_col.name, "email");

        // Test VARCHAR length preservation (backend-specific)
        // Note: SQLite treats VARCHAR as TEXT, other backends preserve length
        match name_col.data_type {
            DataType::VarChar(length) => assert!(length > 0), // PostgreSQL/MySQL preserve length
            DataType::Text => {}                              // SQLite maps all text types to TEXT
            _ => panic!(
                "Unexpected data type for name column: {:?}",
                name_col.data_type
            ),
        }
        match email_col.data_type {
            DataType::VarChar(length) => assert!(length > 0), // PostgreSQL/MySQL preserve length
            DataType::Text => {}                              // SQLite maps all text types to TEXT
            _ => panic!(
                "Unexpected data type for email column: {:?}",
                email_col.data_type
            ),
        }

        // Test non-existing table
        let result = db.get_table_columns("nonexistent_table").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// Test getting complete table information
    async fn test_get_table_info(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db).await;

        // Test users table info
        let table_info = db.get_table_info("users").await.unwrap();
        assert!(table_info.is_some());

        let table_info = table_info.unwrap();
        assert_eq!(table_info.name, "users");
        assert!(!table_info.columns.is_empty());

        // Verify we have expected columns
        let column_names: Vec<&str> = table_info
            .columns
            .keys()
            .map(|name| name.as_str())
            .collect();
        assert!(column_names.contains(&"id"));
        assert!(column_names.contains(&"name"));
        assert!(column_names.contains(&"email"));

        // Test non-existing table
        let result = db.get_table_info("nonexistent_table").await.unwrap();
        assert!(result.is_none());
    }

    /// Test handling of unsupported or edge case types
    async fn test_unsupported_types(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db).await;

        // Test edge_cases table
        let columns = db.get_table_columns("edge_cases").await.unwrap();
        assert!(!columns.is_empty());

        // Should be able to introspect even with edge case types
        let id_col = columns.iter().find(|c| c.name == "id").unwrap();
        assert_eq!(id_col.name, "id");

        // Other columns should be present even if type mapping is approximate
        assert!(columns.iter().any(|c| c.name == "uuid_col"));
        assert!(columns.iter().any(|c| c.name == "json_col"));
        assert!(columns.iter().any(|c| c.name == "data_col"));
    }

    /// Test introspection works within transaction context
    async fn test_transaction_context(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db).await;

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Introspection should work in transaction
        assert!(tx.table_exists("users").await.unwrap());
        assert!(tx.column_exists("users", "name").await.unwrap());

        let columns = tx.get_table_columns("users").await.unwrap();
        assert!(!columns.is_empty());

        let table_info = tx.get_table_info("users").await.unwrap();
        assert!(table_info.is_some());

        // Rollback to test transaction behavior
        tx.rollback().await.unwrap();
    }

    /// Test various edge cases and error conditions
    async fn test_edge_cases(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        // Test with empty database (no tables)
        assert!(!db.table_exists("any_table").await.unwrap());

        let columns = db.get_table_columns("any_table").await.unwrap();
        assert!(columns.is_empty());

        let table_info = db.get_table_info("any_table").await.unwrap();
        assert!(table_info.is_none());

        // Create a simple table for further testing
        self.create_test_schema(&*db).await;

        // Test with special characters in names
        let result = db.table_exists("table'with\"quotes").await;
        assert!(result.is_ok()); // Should not panic or error

        // Test column_exists on non-existent table (some backends may handle this differently)
        let result = db.column_exists("nonexistent_table", "any_column").await;
        assert!(result.is_ok()); // Should not panic, but result may vary

        // Test column_exists with special characters on existing table
        let result = db.column_exists("users", "column'with\"quotes").await;
        assert!(result.is_ok()); // Should not panic or error
    }

    /// Run all introspection tests
    async fn run_all_tests(&self) {
        self.test_table_exists().await;
        self.test_column_exists().await;
        self.test_get_table_columns().await;
        self.test_get_table_info().await;
        self.test_unsupported_types().await;
        self.test_transaction_context().await;
        self.test_edge_cases().await;
    }
}
