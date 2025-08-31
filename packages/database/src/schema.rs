use crate::{Database, DatabaseError, DatabaseValue};

#[derive(Debug, Clone, Copy)]
pub enum DataType {
    VarChar(u16),
    Text,
    Bool,
    SmallInt,
    Int,
    BigInt,
    Real,
    Double,
    Decimal(u8, u8),
    DateTime,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub nullable: bool,
    pub auto_increment: bool,
    pub data_type: DataType,
    pub default: Option<DatabaseValue>,
}

pub struct CreateTableStatement<'a> {
    pub table_name: &'a str,
    pub if_not_exists: bool,
    pub columns: Vec<Column>,
    pub primary_key: Option<&'a str>,
    pub foreign_keys: Vec<(&'a str, &'a str)>,
}

#[must_use]
pub const fn create_table(table_name: &str) -> CreateTableStatement<'_> {
    CreateTableStatement {
        table_name,
        if_not_exists: false,
        columns: vec![],
        primary_key: None,
        foreign_keys: vec![],
    }
}

impl<'a> CreateTableStatement<'a> {
    #[must_use]
    pub const fn if_not_exists(mut self, if_not_exists: bool) -> Self {
        self.if_not_exists = if_not_exists;
        self
    }

    #[must_use]
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    #[must_use]
    pub fn columns(mut self, columns: Vec<Column>) -> Self {
        self.columns.extend(columns);
        self
    }

    #[must_use]
    pub const fn primary_key(mut self, primary_key: &'a str) -> Self {
        self.primary_key = Some(primary_key);
        self
    }

    #[must_use]
    pub fn foreign_key(mut self, foreign_key: (&'a str, &'a str)) -> Self {
        self.foreign_keys.push(foreign_key);
        self
    }

    #[must_use]
    pub fn foreign_keys(mut self, foreign_keys: Vec<(&'a str, &'a str)>) -> Self {
        self.foreign_keys = foreign_keys;
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the `exec_create_table` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_table(&self).await
    }
}

pub struct DropTableStatement<'a> {
    pub table_name: &'a str,
    pub if_exists: bool,
}

#[must_use]
pub const fn drop_table(table_name: &str) -> DropTableStatement<'_> {
    DropTableStatement {
        table_name,
        if_exists: false,
    }
}

impl DropTableStatement<'_> {
    #[must_use]
    pub const fn if_exists(mut self, if_exists: bool) -> Self {
        self.if_exists = if_exists;
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the `exec_drop_table` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_drop_table(&self).await
    }
}

pub struct CreateIndexStatement<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub columns: Vec<&'a str>,
    pub unique: bool,
    pub if_not_exists: bool,
}

#[must_use]
pub const fn create_index(index_name: &str) -> CreateIndexStatement<'_> {
    CreateIndexStatement {
        index_name,
        table_name: "",
        columns: vec![],
        unique: false,
        if_not_exists: false,
    }
}

impl<'a> CreateIndexStatement<'a> {
    #[must_use]
    pub const fn table(mut self, table_name: &'a str) -> Self {
        self.table_name = table_name;
        self
    }

    #[must_use]
    pub fn column(mut self, column: &'a str) -> Self {
        self.columns.push(column);
        self
    }

    #[must_use]
    pub fn columns(mut self, columns: Vec<&'a str>) -> Self {
        self.columns = columns;
        self
    }

    #[must_use]
    pub const fn unique(mut self, unique: bool) -> Self {
        self.unique = unique;
        self
    }

    /// Set whether to use IF NOT EXISTS clause
    ///
    /// # Database Compatibility
    ///
    /// * **`SQLite`**: Full support
    /// * **`PostgreSQL`**: Full support
    /// * **`MySQL`**: Requires `MySQL` 8.0.29 or later. Will produce a syntax error on older versions.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use switchy_database::schema::create_index;
    /// let stmt = create_index("idx_name")
    ///     .table("users")
    ///     .column("email")
    ///     .if_not_exists(true);  // MySQL 8.0.29+ required for this
    /// ```
    #[must_use]
    pub const fn if_not_exists(mut self, if_not_exists: bool) -> Self {
        self.if_not_exists = if_not_exists;
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the `exec_create_index` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_index(&self).await
    }
}

pub struct DropIndexStatement<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub if_exists: bool,
}

#[must_use]
pub const fn drop_index<'a>(index_name: &'a str, table_name: &'a str) -> DropIndexStatement<'a> {
    DropIndexStatement {
        index_name,
        table_name,
        if_exists: false,
    }
}

impl DropIndexStatement<'_> {
    #[must_use]
    pub const fn if_exists(mut self) -> Self {
        self.if_exists = true;
        self
    }

    /// Execute the drop index statement against the provided database.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the `exec_drop_index` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_drop_index(&self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_table_builder_default() {
        let statement = drop_table("test_table");
        assert_eq!(statement.table_name, "test_table");
        assert!(!statement.if_exists);
    }

    #[test]
    fn test_drop_table_builder_with_if_exists() {
        let statement = drop_table("test_table").if_exists(true);
        assert_eq!(statement.table_name, "test_table");
        assert!(statement.if_exists);
    }

    #[test]
    fn test_drop_table_builder_chain() {
        let statement = drop_table("users").if_exists(true);

        assert_eq!(statement.table_name, "users");
        assert!(statement.if_exists);
    }

    #[test]
    fn test_drop_table_builder_if_exists_false() {
        let statement = drop_table("test_table").if_exists(true).if_exists(false);

        assert_eq!(statement.table_name, "test_table");
        assert!(!statement.if_exists);
    }

    // CreateIndexStatement tests
    #[test]
    fn test_create_index_builder_default() {
        let statement = create_index("test_index");
        assert_eq!(statement.index_name, "test_index");
        assert_eq!(statement.table_name, "");
        assert!(statement.columns.is_empty());
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_single_column() {
        let statement = create_index("idx_name").table("users").column("name");

        assert_eq!(statement.index_name, "idx_name");
        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.columns, vec!["name"]);
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_multi_column() {
        let statement = create_index("idx_multi")
            .table("users")
            .columns(vec!["first_name", "last_name"]);

        assert_eq!(statement.index_name, "idx_multi");
        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.columns, vec!["first_name", "last_name"]);
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_unique() {
        let statement = create_index("idx_email")
            .table("users")
            .column("email")
            .unique(true);

        assert_eq!(statement.index_name, "idx_email");
        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.columns, vec!["email"]);
        assert!(statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_if_not_exists() {
        let statement = create_index("idx_test")
            .table("test")
            .column("col")
            .if_not_exists(true);

        assert_eq!(statement.index_name, "idx_test");
        assert_eq!(statement.table_name, "test");
        assert_eq!(statement.columns, vec!["col"]);
        assert!(!statement.unique);
        assert!(statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_method_chaining() {
        let statement = create_index("idx_complex")
            .table("products")
            .column("category_id")
            .column("price")
            .unique(true)
            .if_not_exists(true);

        assert_eq!(statement.index_name, "idx_complex");
        assert_eq!(statement.table_name, "products");
        assert_eq!(statement.columns, vec!["category_id", "price"]);
        assert!(statement.unique);
        assert!(statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_columns_overwrite() {
        let statement = create_index("idx_test")
            .table("test")
            .column("col1")
            .column("col2")
            .columns(vec!["col3", "col4"]); // This should overwrite

        assert_eq!(statement.index_name, "idx_test");
        assert_eq!(statement.table_name, "test");
        assert_eq!(statement.columns, vec!["col3", "col4"]);
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    // DropIndexStatement tests
    #[test]
    fn test_drop_index_builder_default() {
        let statement = drop_index("test_index", "test_table");
        assert_eq!(statement.index_name, "test_index");
        assert_eq!(statement.table_name, "test_table");
        assert!(!statement.if_exists);
    }

    #[test]
    fn test_drop_index_builder_with_if_exists() {
        let statement = drop_index("idx_email", "users").if_exists();
        assert_eq!(statement.index_name, "idx_email");
        assert_eq!(statement.table_name, "users");
        assert!(statement.if_exists);
    }

    #[test]
    fn test_drop_index_builder_if_exists_chaining() {
        let statement = drop_index("idx_complex", "products").if_exists();

        assert_eq!(statement.index_name, "idx_complex");
        assert_eq!(statement.table_name, "products");
        assert!(statement.if_exists);
    }
}
