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
