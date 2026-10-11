//! SQL query builder types and expressions
//!
//! This module provides a type-safe query builder API for constructing SQL queries
//! without writing raw SQL strings. It includes support for SELECT, INSERT, UPDATE,
//! DELETE, and UPSERT operations with WHERE clauses, JOINs, and complex expressions.
//!
//! # Query Builder Pattern
//!
//! The query builders use a fluent API that mirrors SQL syntax:
//!
//! ```rust,ignore
//! use switchy_database::{Database, DatabaseError};
//!
//! # async fn example(db: &dyn Database) -> Result<(), DatabaseError> {
//! // SELECT - methods like where_eq require importing FilterableQuery trait
//! let users = db.select("users")
//!     .columns(&["id", "name", "email"])
//!     .limit(10)
//!     .execute(db)
//!     .await?;
//!
//! // INSERT with values
//! let new_user = db.insert("users")
//!     .value("name", "Alice")
//!     .value("email", "alice@example.com")
//!     .execute(db)
//!     .await?;
//!
//! // UPDATE with values
//! let updated = db.update("users")
//!     .value("last_login", switchy_database::DatabaseValue::Now)
//!     .execute(db)
//!     .await?;
//!
//! // DELETE
//! let deleted = db.delete("users")
//!     .execute(db)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Expression System
//!
//! The module provides an [`Expression`](crate::query::Expression) trait and various expression types
//! for building complex WHERE clauses and query conditions:
//!
//! * **Comparison**: `Eq`, `NotEq`, `Gt`, `Gte`, `Lt`, `Lte`
//! * **Logical**: `And`, `Or`
//! * **List operations**: `In`, `NotIn`, `InList`
//! * **SQL functions**: `Coalesce`
//! * **Raw SQL**: `Literal` for SQL expressions, `Identifier` for column names
//!
//! # JOINs
//!
//! The query builder supports INNER and LEFT JOINs:
//!
//! ```rust,ignore
//! use switchy_database::{Database, DatabaseError};
//!
//! # async fn example(db: &dyn Database) -> Result<(), DatabaseError> {
//! let results = db.select("orders")
//!     .columns(&["orders.id", "users.name"])
//!     .join("users", (
//!         qualified_column("orders", "user_id"),
//!         qualified_column("users", "id"),
//!     ))  // INNER JOIN
//!     .left_join("addresses", (
//!         qualified_column("users", "address_id"),
//!         qualified_column("addresses", "id"),
//!     ))  // LEFT JOIN
//!     .execute(db)
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::fmt::Debug;

use crate::{Database, DatabaseError, DatabaseValue, Row};

/// Render an exact identifier, preserving the transitional raw-enabled behavior.
#[cfg(any(
    feature = "sqlite-rusqlite",
    feature = "duckdb",
    feature = "turso",
    feature = "mysql-sqlx",
    feature = "sqlite-sqlx",
    feature = "postgres-sqlx",
    feature = "postgres-raw"
))]
pub(crate) fn render_identifier(value: &str, delimiter: char) -> String {
    if cfg!(feature = "raw-sql") {
        value.to_owned()
    } else {
        let escaped = value.replace(delimiter, &format!("{delimiter}{delimiter}"));
        format!("{delimiter}{escaped}{delimiter}")
    }
}

/// Render a legacy SELECT column list without allowing fragments in no-raw builds.
/// The standalone wildcard retains its query-builder meaning; all other strings
/// are exact identifiers, including strings containing dots.
#[cfg(any(
    feature = "sqlite-rusqlite",
    feature = "sqlite-sqlx",
    feature = "postgres-sqlx",
    feature = "postgres-raw",
    feature = "mysql-sqlx",
    feature = "duckdb",
    feature = "turso"
))]
pub(crate) fn render_columns(columns: &[&str], delimiter: char) -> String {
    columns
        .iter()
        .map(|column| {
            if *column == "*" {
                "*".to_owned()
            } else {
                render_identifier(column, delimiter)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Sort direction for ORDER BY clauses
#[derive(Debug, Clone, Copy)]
pub enum SortDirection {
    /// Ascending order (smallest to largest, A to Z)
    Asc,
    /// Descending order (largest to smallest, Z to A)
    Desc,
}

/// Sort expression combining a column/expression with a sort direction
#[derive(Debug)]
pub struct Sort {
    /// The column or expression to sort by
    pub expression: Box<dyn Expression>,
    /// The sort direction (ascending or descending)
    pub direction: SortDirection,
}

impl Expression for Sort {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Sort(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}

/// JOIN clause specification for combining tables
#[derive(Debug, Clone)]
pub struct Join<'a> {
    /// Name of the table to join with
    pub table_name: &'a str,
    /// Typed equality condition, or a feature-gated raw predicate.
    pub on: JoinCondition,
    /// Whether this is a LEFT JOIN (true) or INNER JOIN (false)
    pub left: bool,
}

/// A JOIN predicate that cannot carry caller SQL without `raw-sql`.
#[cfg_attr(
    not(feature = "raw-sql"),
    doc = r#"
```compile_fail
use switchy_database::query::JoinCondition;
let _ = JoinCondition::Raw("1 = 1".to_owned());
```
```compile_fail
use switchy_database::query::join;
let _ = join("items", "1 = 1");
```
```compile_fail
use switchy_database::query::select;
let _ = select("items").join("other", "1 = 1");
```
```compile_fail
use switchy_database::query::left_join;
let _ = left_join("items", "1 = 1");
```
```compile_fail
use switchy_database::query::select;
let _ = select("items").left_join("other", "1 = 1");
```
"#
)]
#[derive(Debug, Clone)]
pub enum JoinCondition {
    /// Equality between independently quoted, qualified columns.
    ColumnsEqual(QualifiedColumn, QualifiedColumn),
    /// Conjunction of typed JOIN conditions. An empty conjunction is true.
    And(Vec<Self>),
    /// Arbitrary SQL retained only for compatibility callers.
    #[cfg(feature = "raw-sql")]
    Raw(String),
}

impl From<(QualifiedColumn, QualifiedColumn)> for JoinCondition {
    fn from((first, second): (QualifiedColumn, QualifiedColumn)) -> Self {
        Self::ColumnsEqual(first, second)
    }
}

#[cfg(feature = "raw-sql")]
impl From<&str> for JoinCondition {
    fn from(sql: &str) -> Self {
        Self::Raw(sql.to_owned())
    }
}

#[cfg(feature = "raw-sql")]
impl From<String> for JoinCondition {
    fn from(sql: String) -> Self {
        Self::Raw(sql)
    }
}

impl JoinCondition {
    #[cfg(feature = "_any_backend")]
    pub(crate) fn render(&self, delimiter: char) -> String {
        match self {
            Self::ColumnsEqual(left, right) => {
                format!("{} = {}", left.render(delimiter), right.render(delimiter))
            }
            Self::And(conditions) => {
                if conditions.is_empty() {
                    return "1 = 1".to_owned();
                }
                format!(
                    "({})",
                    conditions
                        .iter()
                        .map(|condition| condition.render(delimiter))
                        .collect::<Vec<_>>()
                        .join(" AND ")
                )
            }
            #[cfg(feature = "raw-sql")]
            Self::Raw(sql) => sql.clone(),
        }
    }
}

/// Creates a JOIN using equality of exact qualified column names.
///
/// Set `left` for a LEFT JOIN. Dots and quotes within each name are literal;
/// use [`qualified_column`] to specify qualification explicitly.
#[must_use]
pub const fn join_columns(
    table_name: &str,
    first: QualifiedColumn,
    second: QualifiedColumn,
    left: bool,
) -> Join<'_> {
    Join {
        table_name,
        on: JoinCondition::ColumnsEqual(first, second),
        left,
    }
}

impl Join<'_> {
    #[cfg(feature = "_any_backend")]
    pub(crate) fn render_table(&self, delimiter: char) -> String {
        #[cfg(feature = "raw-sql")]
        if matches!(self.on, JoinCondition::Raw(_)) {
            return self.table_name.to_owned();
        }
        let escaped = self
            .table_name
            .replace(delimiter, &format!("{delimiter}{delimiter}"));
        format!("{delimiter}{escaped}{delimiter}")
    }
}

impl Expression for Join<'_> {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Join(self)
    }
}

/// Tagged union representing different types of SQL expressions
///
/// This enum is used internally by the query builder to distinguish between
/// different expression types when generating SQL. Each variant holds a reference
/// to the actual expression object.
pub enum ExpressionType<'a> {
    /// SQLite JSON storage type predicate at a validated path.
    JsonTypeIs(&'a JsonTypeIs),
    /// SQLite storage-class observation.
    StorageType(&'a StorageType),
    /// Typed arithmetic addition.
    Add(&'a Add),
    /// SQLite zero-filled BLOB.
    ZeroBlob(&'a ZeroBlob),
    /// Typed SQLite storage-class cast.
    StorageCast(&'a StorageCast),
    #[cfg(feature = "schema")]
    /// A closed persistent schema predicate with escaped constants.
    SchemaPredicate(&'a SchemaPredicate),
    /// A bounded SQLite text projection preserving invalid rows with a fallback.
    BoundedText(&'a BoundedText),
    /// Greater-than comparison between structured expressions.
    GtExpression(&'a GtExpression),
    /// Equality between structured expressions.
    EqExpression(&'a EqExpression),
    /// Inequality between structured expressions.
    NotEqExpression(&'a NotEqExpression),
    /// Equality comparison expression
    Eq(&'a Eq),
    /// Greater than comparison expression
    Gt(&'a Gt),
    /// IN clause expression
    In(&'a In<'a>),
    /// Less than comparison expression
    Lt(&'a Lt),
    /// Logical OR expression
    Or(&'a Or),
    /// Logical AND expression
    And(&'a And),
    /// Greater than or equal comparison expression
    Gte(&'a Gte),
    /// Less than or equal comparison expression
    Lte(&'a Lte),
    /// JOIN clause expression
    Join(&'a Join<'a>),
    /// Sort expression
    Sort(&'a Sort),
    /// NOT IN clause expression
    NotIn(&'a NotIn<'a>),
    /// Not equal comparison expression
    NotEq(&'a NotEq),
    /// Negated SQL pattern comparison.
    NotLike(&'a NotLike),
    /// IN list expression (with explicit list of values)
    InList(&'a InList),
    /// Raw SQL literal expression
    #[cfg(feature = "raw-sql")]
    Literal(&'a Literal),
    /// UTF-8 byte length of a text expression (NULL remains NULL).
    ByteLength(&'a ByteLength),
    /// Minimum non-NULL value of a typed expression.
    Min(&'a Min),
    /// Maximum non-NULL value of a typed expression.
    Max(&'a Max),
    /// Number of rows, or non-NULL values of a typed expression.
    Count(&'a Count),
    /// COALESCE function expression
    Coalesce(&'a Coalesce),
    /// Explicitly qualified column with separately quoted components.
    QualifiedColumn(&'a QualifiedColumn),
    /// Column/table identifier expression
    Identifier(&'a Identifier),
    /// Subquery expression
    SelectQuery(&'a SelectQuery<'a>),
    /// Database value expression
    DatabaseValue(&'a DatabaseValue),
}

/// Base trait for all SQL expression types
///
/// This trait provides the common interface for all expression types in the query builder.
/// Expressions can be column references, comparisons, logical operations, literals, or subqueries.
pub trait Expression: Send + Sync + Debug {
    /// Returns the type tag for this expression
    fn expression_type(&self) -> ExpressionType<'_>;

    /// Extracts bindable parameters from this expression
    ///
    /// Returns only values that can be bound as query parameters, filtering out
    /// NULL values and SQL function expressions like `NOW()`.
    fn params(&self) -> Option<Vec<&DatabaseValue>> {
        self.values().map(|x| {
            x.into_iter()
                .filter(|value| {
                    !value.is_null()
                        && !matches!(value, DatabaseValue::Now | DatabaseValue::NowPlus(_))
                })
                .collect::<Vec<_>>()
        })
    }

    /// Extracts all database values from this expression
    ///
    /// Returns all [`DatabaseValue`] instances within this expression, including those
    /// that cannot be bound as parameters. Default implementation returns `None`.
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        None
    }

    /// Checks if this expression represents a NULL value
    ///
    /// Returns `true` if this expression is a NULL value, `false` otherwise.
    /// Default implementation returns `false`.
    fn is_null(&self) -> bool {
        false
    }
}

/// Raw SQL literal expression
///
/// Represents a raw SQL expression that will be inserted into the generated SQL
/// without escaping or parameterization. Use with caution to avoid SQL injection.
///
/// # Safety
///
/// The value is inserted directly into SQL. Never use with untrusted user input.
#[cfg(feature = "raw-sql")]
#[derive(Debug)]
pub struct Literal {
    /// The raw SQL expression text
    pub value: String,
}

#[cfg(feature = "raw-sql")]
impl From<&str> for Literal {
    fn from(val: &str) -> Self {
        Self {
            value: val.to_string(),
        }
    }
}

#[cfg(feature = "raw-sql")]
impl From<&String> for Literal {
    fn from(val: &String) -> Self {
        Self { value: val.clone() }
    }
}

#[cfg(feature = "raw-sql")]
impl From<String> for Literal {
    fn from(val: String) -> Self {
        Self { value: val }
    }
}

#[cfg(feature = "raw-sql")]
impl From<Literal> for Box<dyn Expression> {
    fn from(val: Literal) -> Self {
        Box::new(val)
    }
}

#[cfg(feature = "raw-sql")]
impl Expression for Literal {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Literal(self)
    }
}

/// Creates a raw SQL literal expression
///
/// # Safety
///
/// The value is inserted directly into SQL without escaping. Never use with untrusted user input.
#[must_use]
#[cfg(feature = "raw-sql")]
pub fn literal(value: &str) -> Literal {
    Literal {
        value: value.to_string(),
    }
}

/// Minimum non-NULL value of a typed expression.
///
/// An empty input or an input containing only NULL values produces SQL NULL.
#[derive(Debug)]
pub struct Min {
    /// Operand, rendered through the backend's typed expression implementation.
    pub expression: Box<dyn Expression>,
}

impl Expression for Min {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Min(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}

impl From<Min> for Box<dyn Expression> {
    fn from(value: Min) -> Self {
        Box::new(value)
    }
}

/// Returns the minimum non-NULL value of a typed expression.
///
/// Use a typed column expression for column operands. Bound values remain bound;
/// no caller-supplied SQL function name or fragment is accepted.
#[must_use]
pub fn min(expression: impl Into<Box<dyn Expression>>) -> Min {
    Min {
        expression: expression.into(),
    }
}

/// Maximum non-NULL value of a typed expression.
///
/// An empty input or an input containing only NULL values produces SQL NULL.
#[derive(Debug)]
pub struct Max {
    /// Typed operand; never an SQL fragment.
    pub expression: Box<dyn Expression>,
}

impl Expression for Max {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Max(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}

impl From<Max> for Box<dyn Expression> {
    fn from(value: Max) -> Self {
        Box::new(value)
    }
}

/// Returns the maximum non-NULL value of a typed expression.
#[must_use]
pub fn max(expression: impl Into<Box<dyn Expression>>) -> Max {
    Max {
        expression: expression.into(),
    }
}

/// Counts rows or non-NULL expression values. Empty input produces zero.
#[derive(Debug)]
pub struct Count {
    /// None counts all rows; Some counts non-NULL values of the typed operand.
    pub expression: Option<Box<dyn Expression>>,
}

impl Expression for Count {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Count(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.as_ref().and_then(|value| value.values())
    }
}

impl From<Count> for Box<dyn Expression> {
    fn from(value: Count) -> Self {
        Box::new(value)
    }
}

/// Counts non-NULL values of a typed expression, preserving bound parameters.
#[must_use]
pub fn count(expression: impl Into<Box<dyn Expression>>) -> Count {
    Count {
        expression: Some(expression.into()),
    }
}

/// Counts all rows, including rows containing NULL values.
#[must_use]
pub const fn count_all() -> Count {
    Count { expression: None }
}

/// UTF-8 byte length of a text expression, preserving SQL NULL.
///
/// This is not a character or grapheme count. The operand must be text;
/// behavior for other SQL types is backend-dependent.
#[derive(Debug)]
pub struct ByteLength {
    /// Typed text operand; never an SQL fragment.
    pub expression: Box<dyn Expression>,
}

impl Expression for ByteLength {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::ByteLength(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}

impl From<ByteLength> for Box<dyn Expression> {
    fn from(value: ByteLength) -> Self {
        Box::new(value)
    }
}

/// Counts UTF-8 bytes in a typed text expression, preserving SQL NULL.
///
/// Use [`identifier`] or [`qualified_column`] for column operands, or a
/// [`DatabaseValue`] for bound text. Strings are not interpreted as SQL.
#[must_use]
pub fn byte_length(expression: impl Into<Box<dyn Expression>>) -> ByteLength {
    ByteLength {
        expression: expression.into(),
    }
}

/// A column qualified by an exact table name or table alias.
///
/// Both components are always quoted independently, even with `raw-sql` enabled.
/// Dots within a component are literal name characters, never extra qualification.
#[derive(Debug, Clone)]
pub struct QualifiedColumn {
    /// Exact table name or alias.
    pub table: String,
    /// Exact column name.
    pub column: String,
}

impl Expression for QualifiedColumn {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::QualifiedColumn(self)
    }
}

impl From<QualifiedColumn> for Box<dyn Expression> {
    fn from(value: QualifiedColumn) -> Self {
        Box::new(value)
    }
}

/// Refers to a column through an explicit table name or alias.
///
/// Unlike `identifier("table.column")`, this produces two independently quoted
/// identifiers separated by a qualification dot. Neither argument accepts SQL.
#[must_use]
pub fn qualified_column(table: &str, column: &str) -> QualifiedColumn {
    QualifiedColumn {
        table: table.to_owned(),
        column: column.to_owned(),
    }
}

impl QualifiedColumn {
    #[cfg(any(
        feature = "sqlite-rusqlite",
        feature = "sqlite-sqlx",
        feature = "postgres-sqlx",
        feature = "postgres-raw",
        feature = "mysql-sqlx",
        feature = "duckdb",
        feature = "turso"
    ))]
    pub(crate) fn render(&self, delimiter: char) -> String {
        let quote = |name: &str| {
            let escaped = name.replace(delimiter, &format!("{delimiter}{delimiter}"));
            format!("{delimiter}{escaped}{delimiter}")
        };
        format!("{}.{}", quote(&self.table), quote(&self.column))
    }
}

/// SQL identifier (column or table name)
///
/// Represents a database identifier that will be properly quoted/escaped
/// for the target database backend.
#[derive(Debug)]
pub struct Identifier {
    /// The identifier name (column or table)
    pub value: String,
}

impl From<&str> for Identifier {
    fn from(val: &str) -> Self {
        Self {
            value: val.to_string(),
        }
    }
}

impl From<String> for Identifier {
    fn from(val: String) -> Self {
        Self { value: val }
    }
}

impl From<Identifier> for Box<dyn Expression> {
    fn from(val: Identifier) -> Self {
        Box::new(val)
    }
}

impl Expression for Identifier {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Identifier(self)
    }
}

/// Creates an SQL identifier expression for a column or table name
///
/// Without `raw-sql`, this expression denotes one exact identifier. Dots,
/// quotes, spaces, and SQL-looking content are part of its name, not SQL.
#[must_use]
pub fn identifier(value: &str) -> Identifier {
    Identifier {
        value: value.to_string(),
    }
}

impl Expression for DatabaseValue {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::DatabaseValue(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(vec![self])
    }

    fn is_null(&self) -> bool {
        #[cfg(all(not(feature = "decimal"), not(feature = "uuid")))]
        {
            matches!(
                self,
                Self::Null
                    | Self::BoolOpt(None)
                    | Self::Real64Opt(None)
                    | Self::Real32Opt(None)
                    | Self::StringOpt(None)
                    | Self::Int64Opt(None)
                    | Self::UInt64Opt(None)
            )
        }
        #[cfg(all(feature = "decimal", not(feature = "uuid")))]
        {
            matches!(
                self,
                Self::Null
                    | Self::BoolOpt(None)
                    | Self::Real64Opt(None)
                    | Self::Real32Opt(None)
                    | Self::StringOpt(None)
                    | Self::Int64Opt(None)
                    | Self::UInt64Opt(None)
                    | Self::DecimalOpt(None)
            )
        }
        #[cfg(all(not(feature = "decimal"), feature = "uuid"))]
        {
            matches!(
                self,
                Self::Null
                    | Self::BoolOpt(None)
                    | Self::Real64Opt(None)
                    | Self::Real32Opt(None)
                    | Self::StringOpt(None)
                    | Self::Int64Opt(None)
                    | Self::UInt64Opt(None)
                    | Self::UuidOpt(None)
            )
        }
        #[cfg(all(feature = "decimal", feature = "uuid"))]
        {
            matches!(
                self,
                Self::Null
                    | Self::BoolOpt(None)
                    | Self::Real64Opt(None)
                    | Self::Real32Opt(None)
                    | Self::StringOpt(None)
                    | Self::Int64Opt(None)
                    | Self::UInt64Opt(None)
                    | Self::DecimalOpt(None)
                    | Self::UuidOpt(None)
            )
        }
    }
}

impl<T: Into<DatabaseValue>> From<T> for Box<dyn Expression> {
    fn from(value: T) -> Self {
        Box::new(value.into())
    }
}

/// Marker trait for expressions that evaluate to boolean values
///
/// Used to ensure WHERE clause expressions are boolean-valued.
pub trait BooleanExpression: Expression {}

/// Logical AND expression combining multiple boolean conditions
#[derive(Debug)]
pub struct And {
    pub(crate) conditions: Vec<Box<dyn BooleanExpression>>,
}

impl BooleanExpression for And {}
impl Expression for And {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::And(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = self
            .conditions
            .iter()
            .filter_map(|x| x.values())
            .collect::<Vec<_>>()
            .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// Logical OR expression combining multiple boolean conditions
#[derive(Debug)]
pub struct Or {
    /// The boolean conditions to combine with OR
    pub conditions: Vec<Box<dyn BooleanExpression>>,
}

impl BooleanExpression for Or {}
impl Expression for Or {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Or(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = self
            .conditions
            .iter()
            .filter_map(|x| x.values())
            .collect::<Vec<_>>()
            .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// Negated SQL LIKE comparison. The pattern is bound as data.
#[derive(Debug)]
pub struct NotLike {
    /// Exact column identifier.
    pub left: Identifier,
    /// Pattern value.
    pub right: DatabaseValue,
}

impl BooleanExpression for NotLike {}
impl Expression for NotLike {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::NotLike(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(vec![&self.right])
    }
}

/// Exclude rows matching a SQL LIKE pattern (`%` and `_` are wildcards).
#[must_use]
pub fn where_not_like(column: impl Into<Identifier>, pattern: impl Into<String>) -> NotLike {
    NotLike {
        left: column.into(),
        right: DatabaseValue::String(pattern.into()),
    }
}

/// A validated JSON object-member path; callers cannot supply SQL or path fragments.
#[derive(Debug, Clone)]
pub struct JsonPath {
    member: String,
}
impl JsonPath {
    /// Exact validated object member represented by this path.
    #[must_use]
    pub fn member_name(&self) -> &str {
        &self.member
    }

    /// Select one exact object key using a conservative portable path segment.
    ///
    /// # Errors
    /// * Rejects empty keys and characters other than ASCII letters, digits and underscore.
    pub fn member(member: impl Into<String>) -> Result<Self, crate::DatabaseError> {
        let member = member.into();
        if member.is_empty()
            || !member
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(crate::DatabaseError::InvalidQuery(
                "Invalid JSON object path segment".into(),
            ));
        }
        Ok(Self { member })
    }
    pub(crate) fn sqlite(&self) -> String {
        format!("'$.{}'", self.member)
    }
}
/// Closed JSON value type names, distinct from SQL storage classes.
#[derive(Debug, Clone, Copy)]
pub enum JsonType {
    /// JSON integer.
    Integer,
    /// JSON non-integer number.
    Real,
    /// JSON string.
    Text,
    /// Explicit JSON null (not a missing member).
    Null,
    /// JSON object.
    Object,
    /// JSON array.
    Array,
    /// JSON true.
    True,
    /// JSON false.
    False,
}
impl JsonType {
    pub(crate) const fn sqlite(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Real => "real",
            Self::Text => "text",
            Self::Null => "null",
            Self::Object => "object",
            Self::Array => "array",
            Self::True => "true",
            Self::False => "false",
        }
    }
}
/// Test a JSON member's type without fetching or parsing the JSON in the caller.
#[derive(Debug)]
pub struct JsonTypeIs {
    /// Structured JSON operand.
    pub expression: Box<dyn Expression>,
    /// Validated object-member path.
    pub path: JsonPath,
    /// Required JSON type.
    pub kind: JsonType,
}
impl BooleanExpression for JsonTypeIs {}
impl Expression for JsonTypeIs {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::JsonTypeIs(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}
/// Require an exact JSON type. Malformed JSON remains a backend execution error.
#[must_use]
pub fn json_type_is(
    expression: impl Into<Box<dyn Expression>>,
    path: JsonPath,
    kind: JsonType,
) -> JsonTypeIs {
    JsonTypeIs {
        expression: expression.into(),
        path,
        kind,
    }
}

/// Observe SQLite's storage class without decoding the value.
#[derive(Debug)]
pub struct StorageType {
    /// Structured operand.
    pub expression: Box<dyn Expression>,
}
impl Expression for StorageType {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::StorageType(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}
impl From<StorageType> for Box<dyn Expression> {
    fn from(value: StorageType) -> Self {
        Box::new(value)
    }
}
/// Return SQLite's closed storage-class name (null/integer/real/text/blob).
#[must_use]
pub fn storage_type(expression: impl Into<Box<dyn Expression>>) -> StorageType {
    StorageType {
        expression: expression.into(),
    }
}

/// Typed arithmetic addition used in atomic assignments.
#[derive(Debug)]
pub struct Add {
    /// Structured left operand.
    pub left: Box<dyn Expression>,
    /// Structured right operand.
    pub right: Box<dyn Expression>,
}
impl Expression for Add {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Add(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(
            [
                self.left.values().unwrap_or_default(),
                self.right.values().unwrap_or_default(),
            ]
            .concat(),
        )
    }
}
impl From<Add> for Box<dyn Expression> {
    fn from(value: Add) -> Self {
        Box::new(value)
    }
}
/// Add structured operands; values remain bound, never SQL fragments.
#[must_use]
pub fn add(left: impl Into<Box<dyn Expression>>, right: impl Into<Box<dyn Expression>>) -> Add {
    Add {
        left: left.into(),
        right: right.into(),
    }
}

/// A zero-filled SQLite BLOB, allocated by SQLite rather than the caller.
#[derive(Debug)]
pub struct ZeroBlob {
    /// Requested byte length.
    pub length: u32,
}
impl Expression for ZeroBlob {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::ZeroBlob(self)
    }
}
impl From<ZeroBlob> for Box<dyn Expression> {
    fn from(value: ZeroBlob) -> Self {
        Box::new(value)
    }
}
/// Construct a SQLite zero-filled BLOB. Oversized allocations fail at execution.
#[must_use]
pub const fn zero_blob(length: u32) -> ZeroBlob {
    ZeroBlob { length }
}
/// Closed SQLite storage-class cast targets.
#[derive(Debug, Clone, Copy)]
pub enum StorageCastTarget {
    /// BLOB storage.
    Blob,
    /// TEXT storage.
    Text,
}
/// A typed storage-class cast with no caller type-name fragment.
#[derive(Debug)]
pub struct StorageCast {
    /// Structured operand.
    pub expression: Box<dyn Expression>,
    /// Closed target.
    pub target: StorageCastTarget,
}
impl Expression for StorageCast {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::StorageCast(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.expression.values()
    }
}
impl From<StorageCast> for Box<dyn Expression> {
    fn from(value: StorageCast) -> Self {
        Box::new(value)
    }
}
/// Cast a typed expression to SQLite BLOB storage.
#[must_use]
pub fn cast_blob(expression: impl Into<Box<dyn Expression>>) -> StorageCast {
    StorageCast {
        expression: expression.into(),
        target: StorageCastTarget::Blob,
    }
}
/// Cast a typed expression to SQLite TEXT storage.
#[must_use]
pub fn cast_text(expression: impl Into<Box<dyn Expression>>) -> StorageCast {
    StorageCast {
        expression: expression.into(),
        target: StorageCastTarget::Text,
    }
}

/// A closed persistent predicate rendered from the typed schema AST.
#[cfg(feature = "schema")]
#[derive(Debug)]
pub struct SchemaPredicate {
    pub(crate) sqlite: String,
    #[cfg(feature = "mysql-sqlx")]
    pub(crate) mysql: String,
}
#[cfg(feature = "schema")]
impl SchemaPredicate {
    /// Canonical escaped SQLite representation of this closed typed predicate.
    #[must_use]
    pub fn canonical_sqlite(&self) -> &str {
        &self.sqlite
    }
}
#[cfg(feature = "schema")]
impl BooleanExpression for SchemaPredicate {}
#[cfg(feature = "schema")]
impl Expression for SchemaPredicate {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::SchemaPredicate(self)
    }
}
/// Preserve a persistent predicate's literal AST for planner compatibility.
///
/// # Errors
/// * Rejects malformed typed schema constants or predicates.
#[cfg(feature = "schema")]
pub fn schema_predicate(
    predicate: &crate::schema::SchemaExpression,
) -> Result<SchemaPredicate, crate::DatabaseError> {
    Ok(SchemaPredicate {
        sqlite: predicate.render('"')?,
        #[cfg(feature = "mysql-sqlx")]
        mysql: predicate.render('`')?,
    })
}

/// A SQLite text projection that does not materialize oversized or wrongly typed values.
#[derive(Debug)]
pub struct BoundedText {
    /// Exact column/expression to inspect.
    pub expression: Box<dyn Expression>,
    /// UTF-8 byte budget.
    pub max_bytes: u64,
    /// Value returned for non-text and oversized inputs, including NULL.
    pub fallback: DatabaseValue,
}
impl Expression for BoundedText {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::BoundedText(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        // The inspected expression is rendered three times; retain bind order.
        let mut values = Vec::new();
        for _ in 0..3 {
            values.extend(self.expression.values().unwrap_or_default());
        }
        values.push(&self.fallback);
        Some(values)
    }
}
impl From<BoundedText> for Box<dyn Expression> {
    fn from(value: BoundedText) -> Self {
        Box::new(value)
    }
}
/// Project text only when its SQLite storage class is text and its byte length is bounded.
/// NULL, non-text and oversized inputs yield the caller's typed fallback, never drop rows.
#[must_use]
pub fn bounded_text(
    expression: impl Into<Box<dyn Expression>>,
    max_bytes: u64,
    fallback: DatabaseValue,
) -> BoundedText {
    BoundedText {
        expression: expression.into(),
        max_bytes,
        fallback,
    }
}

/// Equality between structured expressions, including qualified columns.
#[derive(Debug)]
pub struct EqExpression {
    /// Structured left operand.
    pub left: Box<dyn Expression>,
    /// Structured right operand.
    pub right: Box<dyn Expression>,
}
impl BooleanExpression for EqExpression {}
impl Expression for EqExpression {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::EqExpression(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(
            [
                self.left.values().unwrap_or_default(),
                self.right.values().unwrap_or_default(),
            ]
            .concat(),
        )
    }
}
/// Inequality between structured expressions, including qualified columns.
#[derive(Debug)]
pub struct NotEqExpression {
    /// Structured left operand.
    pub left: Box<dyn Expression>,
    /// Structured right operand.
    pub right: Box<dyn Expression>,
}
impl BooleanExpression for NotEqExpression {}
impl Expression for NotEqExpression {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::NotEqExpression(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(
            [
                self.left.values().unwrap_or_default(),
                self.right.values().unwrap_or_default(),
            ]
            .concat(),
        )
    }
}
/// Greater-than comparison between typed expressions.
#[derive(Debug)]
pub struct GtExpression {
    /// Structured left operand.
    pub left: Box<dyn Expression>,
    /// Structured right operand.
    pub right: Box<dyn Expression>,
}
impl BooleanExpression for GtExpression {}
impl Expression for GtExpression {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::GtExpression(self)
    }
    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(
            [
                self.left.values().unwrap_or_default(),
                self.right.values().unwrap_or_default(),
            ]
            .concat(),
        )
    }
}
/// Compare typed expressions using strict greater-than ordering.
#[must_use]
pub fn where_gt_expression(
    left: impl Into<Box<dyn Expression>>,
    right: impl Into<Box<dyn Expression>>,
) -> GtExpression {
    GtExpression {
        left: left.into(),
        right: right.into(),
    }
}

/// Compare typed expressions without flattening qualified column names.
#[must_use]
pub fn where_eq_expression(
    left: impl Into<Box<dyn Expression>>,
    right: impl Into<Box<dyn Expression>>,
) -> EqExpression {
    EqExpression {
        left: left.into(),
        right: right.into(),
    }
}
/// Exclude equal typed expressions; a NULL right operand means IS NOT NULL.
#[must_use]
pub fn where_not_eq_expression(
    left: impl Into<Box<dyn Expression>>,
    right: impl Into<Box<dyn Expression>>,
) -> NotEqExpression {
    NotEqExpression {
        left: left.into(),
        right: right.into(),
    }
}

/// Not equal comparison expression (!=)
#[derive(Debug)]
pub struct NotEq {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// Right-hand side (value or expression)
    pub right: Box<dyn Expression>,
}

impl BooleanExpression for NotEq {}
impl Expression for NotEq {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::NotEq(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

/// Equal comparison expression (=)
#[derive(Debug)]
pub struct Eq {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// Right-hand side (value or expression)
    pub right: Box<dyn Expression>,
}

impl BooleanExpression for Eq {}
impl Expression for Eq {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Eq(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

/// Greater than comparison expression (>)
#[derive(Debug)]
pub struct Gt {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// Right-hand side (value or expression)
    pub right: Box<dyn Expression>,
}

impl BooleanExpression for Gt {}
impl Expression for Gt {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Gt(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

/// Greater than or equal comparison expression (>=)
#[derive(Debug)]
pub struct Gte {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// Right-hand side (value or expression)
    pub right: Box<dyn Expression>,
}

impl BooleanExpression for Gte {}
impl Expression for Gte {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Gte(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

/// Less than comparison expression (<)
#[derive(Debug)]
pub struct Lt {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// Right-hand side (value or expression)
    pub right: Box<dyn Expression>,
}

impl BooleanExpression for Lt {}
impl Expression for Lt {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Lt(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

/// Less than or equal comparison expression (<=)
#[derive(Debug)]
pub struct Lte {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// Right-hand side (value or expression)
    pub right: Box<dyn Expression>,
}

impl BooleanExpression for Lte {}
impl Expression for Lte {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Lte(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

/// IN clause expression checking if column value is in a list
#[derive(Debug)]
pub struct In<'a> {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// List of values or subquery to check against
    pub values: Box<dyn List + 'a>,
}

impl BooleanExpression for In<'_> {}
impl Expression for In<'_> {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::In(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = [
            self.left.values().unwrap_or_default(),
            self.values.values().unwrap_or_default(),
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// NOT IN clause expression checking if column value is not in a list
#[derive(Debug)]
pub struct NotIn<'a> {
    /// Left-hand side (column identifier)
    pub left: Identifier,
    /// List of values or subquery to check against
    pub values: Box<dyn List + 'a>,
}

impl BooleanExpression for NotIn<'_> {}
impl Expression for NotIn<'_> {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::NotIn(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = [
            self.left.values().unwrap_or_default(),
            self.values.values().unwrap_or_default(),
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// Creates a sort expression for ORDER BY clauses
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::query::{sort, identifier, SortDirection};
///
/// // Sort by name ascending
/// let sort_asc = sort(identifier("name"), SortDirection::Asc);
///
/// // Sort by age descending
/// let sort_desc = sort(identifier("age"), SortDirection::Desc);
/// ```
#[must_use]
pub fn sort<T>(expression: T, direction: SortDirection) -> Sort
where
    T: Into<Box<dyn Expression>>,
{
    Sort {
        expression: expression.into(),
        direction,
    }
}

/// Creates an equality comparison expression (column = value)
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::query::where_eq;
/// use switchy_database::DatabaseValue;
///
/// // Compare column to value
/// let filter = where_eq("user_id", DatabaseValue::Int64(123));
/// ```
#[must_use]
pub fn where_eq<L, R>(left: L, right: R) -> Eq
where
    L: Into<Identifier>,
    R: Into<Box<dyn Expression>>,
{
    Eq {
        left: left.into(),
        right: right.into(),
    }
}

/// Creates a not-equal comparison expression (column != value)
#[must_use]
pub fn where_not_eq<L, R>(left: L, right: R) -> NotEq
where
    L: Into<Identifier>,
    R: Into<Box<dyn Expression>>,
{
    NotEq {
        left: left.into(),
        right: right.into(),
    }
}

/// Creates a greater-than comparison expression (column > value)
#[must_use]
pub fn where_gt<L, R>(left: L, right: R) -> Gt
where
    L: Into<Identifier>,
    R: Into<Box<dyn Expression>>,
{
    Gt {
        left: left.into(),
        right: right.into(),
    }
}

/// Creates a greater-than-or-equal comparison expression (column >= value)
#[must_use]
pub fn where_gte<L, R>(left: L, right: R) -> Gte
where
    L: Into<Identifier>,
    R: Into<Box<dyn Expression>>,
{
    Gte {
        left: left.into(),
        right: right.into(),
    }
}

/// Creates a less-than comparison expression (column < value)
#[must_use]
pub fn where_lt<L, R>(left: L, right: R) -> Lt
where
    L: Into<Identifier>,
    R: Into<Box<dyn Expression>>,
{
    Lt {
        left: left.into(),
        right: right.into(),
    }
}

/// Creates a less-than-or-equal comparison expression (column <= value)
#[must_use]
pub fn where_lte<L, R>(left: L, right: R) -> Lte
where
    L: Into<Identifier>,
    R: Into<Box<dyn Expression>>,
{
    Lte {
        left: left.into(),
        right: right.into(),
    }
}

/// Creates a logical AND expression combining multiple boolean expressions
///
/// All conditions must be true for the AND expression to evaluate to true.
#[must_use]
pub fn where_and(conditions: Vec<Box<dyn BooleanExpression>>) -> And {
    And { conditions }
}

/// Creates a logical OR expression combining multiple boolean expressions
///
/// At least one condition must be true for the OR expression to evaluate to true.
#[must_use]
pub fn where_or(conditions: Vec<Box<dyn BooleanExpression>>) -> Or {
    Or { conditions }
}

/// Creates an INNER JOIN clause with a typed column pair.
///
/// SQL-string predicates are accepted only with the `raw-sql` feature.
///
/// # Examples
///
/// ```
/// use switchy_database::query::{join, qualified_column};
///
/// let join_clause = join("orders", (
///     qualified_column("orders", "user_id"),
///     qualified_column("users", "id"),
/// ));
/// ```
#[must_use]
pub fn join(table_name: &str, on: impl Into<JoinCondition>) -> Join<'_> {
    Join {
        table_name,
        on: on.into(),
        left: false,
    }
}

/// Creates a LEFT JOIN clause with a typed column pair.
///
/// SQL-string predicates are accepted only with the `raw-sql` feature.
///
/// # Examples
///
/// ```
/// use switchy_database::query::{left_join, qualified_column};
///
/// let join_clause = left_join("orders", (
///     qualified_column("orders", "user_id"),
///     qualified_column("users", "id"),
/// ));
/// ```
#[must_use]
pub fn left_join(table_name: &str, on: impl Into<JoinCondition>) -> Join<'_> {
    Join {
        table_name,
        on: on.into(),
        left: true,
    }
}

/// COALESCE SQL function expression returning first non-NULL value
#[derive(Debug)]
pub struct Coalesce {
    /// List of expressions to evaluate in order
    pub values: Vec<Box<dyn Expression>>,
}

impl List for Coalesce {}
impl Expression for Coalesce {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::Coalesce(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = self
            .values
            .iter()
            .flat_map(|x| x.values().unwrap_or_default())
            .collect::<Vec<_>>();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// Creates a COALESCE expression that returns the first non-NULL value
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::query::{coalesce, identifier};
/// use switchy_database::DatabaseValue;
///
/// // Returns first non-NULL value from list
/// let expr = coalesce(vec![
///     Box::new(identifier("email")),
///     Box::new(DatabaseValue::String("unknown@example.com".to_string())),
/// ]);
/// ```
#[must_use]
pub fn coalesce(values: Vec<Box<dyn Expression>>) -> Coalesce {
    Coalesce { values }
}

/// List of expressions for IN clause
#[derive(Debug)]
pub struct InList {
    /// The expressions in the list
    pub values: Vec<Box<dyn Expression>>,
}

impl List for InList {}
impl Expression for InList {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::InList(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = self
            .values
            .iter()
            .flat_map(|x| x.values().unwrap_or_default())
            .collect::<Vec<_>>();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// Marker trait for expressions that represent lists of values
///
/// Used for IN and NOT IN clauses to ensure type safety.
pub trait List: Expression {}

impl<T> From<Vec<T>> for Box<dyn List>
where
    T: Into<Box<dyn Expression>> + Send + Sync,
{
    fn from(val: Vec<T>) -> Self {
        Box::new(InList {
            values: val.into_iter().map(std::convert::Into::into).collect(),
        })
    }
}

/// Creates an IN expression (column IN (values...))
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::query::where_in;
/// use switchy_database::DatabaseValue;
///
/// let filter = where_in("status", vec![
///     DatabaseValue::String("active".to_string()),
///     DatabaseValue::String("pending".to_string()),
/// ]);
/// ```
#[must_use]
pub fn where_in<'a, L, V>(left: L, values: V) -> In<'a>
where
    L: Into<Identifier>,
    V: Into<Box<dyn List + 'a>>,
{
    In {
        left: left.into(),
        values: values.into(),
    }
}

/// Creates a NOT IN expression (column NOT IN (values...))
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::query::where_not_in;
/// use switchy_database::DatabaseValue;
///
/// let filter = where_not_in("status", vec![
///     DatabaseValue::String("archived".to_string()),
///     DatabaseValue::String("deleted".to_string()),
/// ]);
/// ```
#[must_use]
pub fn where_not_in<'a, L, V>(left: L, values: V) -> NotIn<'a>
where
    L: Into<Identifier>,
    V: Into<Box<dyn List + 'a>>,
{
    NotIn {
        left: left.into(),
        values: values.into(),
    }
}

/// Helper macro to create boxed expression vectors for query builders
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::{boxed, query::{where_eq, where_gt}};
/// use switchy_database::DatabaseValue;
///
/// // Create a vec of boxed expressions
/// let conditions = boxed![
///     where_eq("user_id", DatabaseValue::Int64(123)),
///     where_gt("age", DatabaseValue::Int32(18)),
/// ];
/// ```
#[macro_export]
macro_rules! boxed {
    () => (
        Vec::new()
    );
    ($($x:expr),+ $(,)?) => (
        vec![$(Box::new($x)),+]
    );
}

/// Trait for query types that support WHERE clause filtering
///
/// Provides a fluent API for adding filter conditions to SELECT, UPDATE, and DELETE statements.
#[allow(clippy::module_name_repetitions)]
pub trait FilterableQuery
where
    Self: Sized,
{
    /// Add a strict greater-than predicate over structured expressions.
    #[must_use]
    fn where_gt_expression(
        self,
        left: impl Into<Box<dyn Expression>>,
        right: impl Into<Box<dyn Expression>>,
    ) -> Self {
        self.filter(Box::new(where_gt_expression(left, right)))
    }

    /// Add equality between structured expressions, including qualified columns.
    #[must_use]
    fn where_eq_expression(
        self,
        left: impl Into<Box<dyn Expression>>,
        right: impl Into<Box<dyn Expression>>,
    ) -> Self {
        self.filter(Box::new(where_eq_expression(left, right)))
    }

    /// Add inequality between structured expressions, retaining NULL semantics.
    #[must_use]
    fn where_not_eq_expression(
        self,
        left: impl Into<Box<dyn Expression>>,
        right: impl Into<Box<dyn Expression>>,
    ) -> Self {
        self.filter(Box::new(where_not_eq_expression(left, right)))
    }

    /// Adds multiple filter conditions to the query
    #[must_use]
    fn filters(self, filters: Vec<Box<dyn BooleanExpression>>) -> Self {
        let mut this = self;
        for filter in filters {
            this = this.filter(filter);
        }
        this
    }

    /// Excludes rows matching a bound SQL LIKE pattern.
    #[must_use]
    fn where_not_like(self, column: impl Into<Identifier>, pattern: impl Into<String>) -> Self {
        self.filter(Box::new(where_not_like(column, pattern)))
    }

    /// Adds a single filter condition to the WHERE clause.
    #[must_use]
    fn filter(self, filter: Box<dyn BooleanExpression>) -> Self;

    /// Conditionally adds a filter if the option contains a value
    #[must_use]
    fn filter_if_some<T: BooleanExpression + 'static>(self, filter: Option<T>) -> Self {
        if let Some(filter) = filter {
            self.filter(Box::new(filter))
        } else {
            self
        }
    }

    /// Adds an IN clause filter (column IN (values...))
    #[must_use]
    fn where_in<L, V>(self, left: L, values: V) -> Self
    where
        L: Into<Identifier>,
        V: Into<Box<dyn List>>,
    {
        self.filter(Box::new(where_in(left, values)))
    }

    /// Adds a NOT IN clause filter (column NOT IN (values...))
    #[must_use]
    fn where_not_in<L, V>(self, left: L, values: V) -> Self
    where
        L: Into<Identifier>,
        V: Into<Box<dyn List>>,
    {
        self.filter(Box::new(where_not_in(left, values)))
    }

    /// Adds a logical AND filter combining multiple conditions
    #[must_use]
    fn where_and(self, conditions: Vec<Box<dyn BooleanExpression>>) -> Self {
        self.filter(Box::new(where_and(conditions)))
    }

    /// Adds a logical OR filter combining multiple conditions
    #[must_use]
    fn where_or(self, conditions: Vec<Box<dyn BooleanExpression>>) -> Self {
        self.filter(Box::new(where_or(conditions)))
    }

    /// Adds an equality comparison filter (column = value)
    #[must_use]
    fn where_eq<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_eq(left, right)))
    }

    /// Adds a not-equal comparison filter (column != value)
    #[must_use]
    fn where_not_eq<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_not_eq(left, right)))
    }

    /// Adds a greater-than comparison filter (column > value)
    #[must_use]
    fn where_gt<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_gt(left, right)))
    }

    /// Adds a greater-than-or-equal comparison filter (column >= value)
    #[must_use]
    fn where_gte<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_gte(left, right)))
    }

    /// Adds a less-than comparison filter (column < value)
    #[must_use]
    fn where_lt<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_lt(left, right)))
    }

    /// Adds a less-than-or-equal comparison filter (column <= value)
    #[must_use]
    fn where_lte<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_lte(left, right)))
    }
}

impl<'a> From<SelectQuery<'a>> for Box<dyn List + 'a> {
    fn from(val: SelectQuery<'a>) -> Self {
        Box::new(val)
    }
}

/// A typed SELECT expression with an optional exact, quoted output name.
#[derive(Debug)]
pub struct Projection {
    /// Expression evaluated by the database.
    pub expression: Box<dyn Expression>,
    /// Exact output name, never interpreted as SQL (including with `raw-sql`).
    pub alias: Option<String>,
}

impl Projection {
    /// Construct an unaliased projection.
    #[must_use]
    pub fn new(expression: impl Into<Box<dyn Expression>>) -> Self {
        Self {
            expression: expression.into(),
            alias: None,
        }
    }

    /// Assign an exact output name.
    #[must_use]
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }
}

/// SELECT query builder for retrieving data from tables
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SelectQuery<'a> {
    /// Exact table alias, independently quoted from the table name.
    pub table_alias: Option<&'a str>,
    /// Force an exact SQLite index; unsupported backends reject this option.
    pub indexed_by: Option<&'a str>,
    /// Table to select from
    pub table_name: &'a str,
    /// Whether to return only distinct rows
    pub distinct: bool,
    /// Columns to retrieve (empty means *)
    pub columns: &'a [&'a str],
    /// Typed projections; when nonempty these replace the legacy columns.
    pub projections: Vec<Projection>,
    /// Typed GROUP BY expressions, in SQL order.
    pub groups: Vec<Box<dyn Expression>>,
    /// WHERE clause filters
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    /// JOIN clauses
    pub joins: Option<Vec<Join<'a>>>,
    /// ORDER BY clauses
    pub sorts: Option<Vec<Sort>>,
    /// Maximum number of rows to return
    pub limit: Option<usize>,
}

impl List for SelectQuery<'_> {}
impl Expression for SelectQuery<'_> {
    fn expression_type(&self) -> ExpressionType<'_> {
        ExpressionType::SelectQuery(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let joins_values = self
            .joins
            .as_ref()
            .map(|x| {
                x.iter()
                    .flat_map(|j| j.values().unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let filters_values = self
            .filters
            .as_ref()
            .map(|x| {
                x.iter()
                    .flat_map(|j| j.values().unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let sorts_values = self
            .sorts
            .as_ref()
            .map(|x| {
                x.iter()
                    .flat_map(|j| j.values().unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let projections_values = self
            .projections
            .iter()
            .flat_map(|p| p.expression.values().unwrap_or_default())
            .collect::<Vec<_>>();
        let groups_values = self
            .groups
            .iter()
            .flat_map(|p| p.values().unwrap_or_default())
            .collect::<Vec<_>>();
        let values: Vec<_> = [
            projections_values,
            joins_values,
            filters_values,
            groups_values,
            sorts_values,
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

/// Creates a SELECT query builder for the specified table
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::query::select;
///
/// let query = select("users")
///     .columns(&["id", "name"])
///     .execute(db)
///     .await?;
/// ```
#[must_use]
pub fn select(table_name: &str) -> SelectQuery<'_> {
    SelectQuery {
        table_alias: None,
        indexed_by: None,
        table_name,
        distinct: false,
        columns: &["*"],
        projections: Vec::new(),
        groups: Vec::new(),
        filters: None,
        joins: None,
        sorts: None,
        limit: None,
    }
}

impl FilterableQuery for SelectQuery<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(filters) = &mut self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
    }
}

impl<'a> SelectQuery<'a> {
    /// Alias the selected table using an exact identifier, never a SQL fragment.
    #[must_use]
    pub const fn table_alias(mut self, alias: &'a str) -> Self {
        self.table_alias = Some(alias);
        self
    }

    #[cfg(feature = "_any_backend")]
    pub(crate) fn render_table(&self, delimiter: char) -> String {
        let table = render_identifier(self.table_name, delimiter);
        self.table_alias.map_or_else(
            || table.clone(),
            |alias| {
                format!(
                    "{table} AS {delimiter}{}{delimiter}",
                    alias.replace(delimiter, &format!("{delimiter}{delimiter}"))
                )
            },
        )
    }
    /// Require the SQLite planner to use this exact index.
    #[must_use]
    pub const fn indexed_by(mut self, index: &'a str) -> Self {
        self.indexed_by = Some(index);
        self
    }
    /// Replace the legacy column list with typed projections.
    #[must_use]
    pub fn projections(mut self, projections: Vec<Projection>) -> Self {
        self.projections = projections;
        self
    }

    /// Append a typed SELECT expression.
    #[must_use]
    pub fn project(mut self, expression: impl Into<Box<dyn Expression>>) -> Self {
        self.projections.push(Projection::new(expression));
        self
    }

    /// Append a typed expression with an exact, quoted output name.
    ///
    /// ```
    /// use switchy_database::query::{select, identifier, count_all, byte_length};
    /// let inventory = select("events")
    ///     .project(identifier("kind"))
    ///     .project_as(count_all(), "event_count")
    ///     .group_by(identifier("kind"));
    /// let lengths = select("events")
    ///     .project_as(byte_length(identifier("payload")), "payload_bytes");
    /// ```
    #[must_use]
    pub fn project_as(
        mut self,
        expression: impl Into<Box<dyn Expression>>,
        alias: impl Into<String>,
    ) -> Self {
        self.projections
            .push(Projection::new(expression).alias(alias));
        self
    }

    /// Append a typed GROUP BY expression.
    #[must_use]
    pub fn group_by(mut self, expression: impl Into<Box<dyn Expression>>) -> Self {
        self.groups.push(expression.into());
        self
    }

    /// Adds DISTINCT modifier to return only unique rows
    #[must_use]
    pub const fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// Specifies which columns to retrieve (default is all columns)
    #[must_use]
    pub const fn columns(mut self, columns: &'a [&'a str]) -> Self {
        self.columns = columns;
        self
    }

    /// Adds multiple JOIN clauses to the query
    #[must_use]
    pub fn joins(mut self, joins: Vec<Join<'a>>) -> Self {
        for join in joins {
            if let Some(joins) = &mut self.joins {
                joins.push(join);
            } else {
                self.joins.replace(vec![join]);
            }
        }
        self
    }

    /// Adds an INNER JOIN with a typed column pair or a `JoinCondition`.
    ///
    /// SQL-string predicates require the `raw-sql` feature.
    #[must_use]
    pub fn join(mut self, table_name: &'a str, on: impl Into<JoinCondition>) -> Self {
        if let Some(joins) = &mut self.joins {
            joins.push(join(table_name, on));
        } else {
            self.joins.replace(vec![join(table_name, on)]);
        }
        self
    }

    /// Adds multiple LEFT JOIN clauses to the query
    #[must_use]
    pub fn left_joins(mut self, left_joins: Vec<Join<'a>>) -> Self {
        for left_join in left_joins {
            if let Some(left_joins) = &mut self.joins {
                left_joins.push(left_join);
            } else {
                self.joins.replace(vec![left_join]);
            }
        }
        self
    }

    /// Adds a LEFT JOIN with a typed column pair or a `JoinCondition`.
    ///
    /// SQL-string predicates require the `raw-sql` feature.
    #[must_use]
    pub fn left_join(mut self, table_name: &'a str, on: impl Into<JoinCondition>) -> Self {
        if let Some(left_joins) = &mut self.joins {
            left_joins.push(left_join(table_name, on));
        } else {
            self.joins.replace(vec![left_join(table_name, on)]);
        }
        self
    }

    /// Adds multiple ORDER BY clauses to the query
    #[must_use]
    pub fn sorts(mut self, sorts: Vec<Sort>) -> Self {
        for sort in sorts {
            if let Some(sorts) = &mut self.sorts {
                sorts.push(sort);
            } else {
                self.sorts.replace(vec![sort]);
            }
        }
        self
    }

    /// Adds a single ORDER BY clause
    #[must_use]
    pub fn sort<T>(mut self, expression: T, direction: SortDirection) -> Self
    where
        T: Into<Identifier>,
    {
        if let Some(sorts) = &mut self.sorts {
            sorts.push(sort(expression.into(), direction));
        } else {
            self.sorts.replace(vec![sort(expression.into(), direction)]);
        }
        self
    }

    /// Limits the number of rows returned
    #[must_use]
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the select query execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<Vec<Row>, DatabaseError> {
        db.query(&self).await
    }

    /// # Errors
    ///
    /// Will return `Err` if the select query execution failed.
    pub async fn execute_first(self, db: &dyn Database) -> Result<Option<Row>, DatabaseError> {
        let this = if self.limit.is_none() {
            self.limit(1)
        } else {
            self
        };

        db.query_first(&this).await
    }
}

/// UPSERT statement for inserting multiple rows or updating on conflict
pub struct UpsertMultiStatement<'a> {
    /// Table to upsert into
    pub table_name: &'a str,
    /// Multiple rows of column-value pairs to insert/update
    pub values: Vec<Vec<(&'a str, Box<dyn Expression>)>>,
    /// Columns that form the unique constraint to detect conflicts
    pub unique: Option<Vec<Box<dyn Expression>>>,
}

/// Creates a new multi-row UPSERT statement builder
///
/// Constructs an UPSERT statement that can insert or update multiple rows in a single operation.
#[must_use]
pub fn upsert_multi(table_name: &str) -> UpsertMultiStatement<'_> {
    UpsertMultiStatement {
        table_name,
        values: vec![],
        unique: None,
    }
}

impl<'a> UpsertMultiStatement<'a> {
    /// Sets the values to upsert as multiple rows
    ///
    /// Each inner vector represents one row with column-value pairs.
    pub fn values<T: Into<Box<dyn Expression>>>(
        &mut self,
        values: Vec<Vec<(&'a str, T)>>,
    ) -> &mut Self {
        self.values.extend(values.into_iter().map(|values| {
            values
                .into_iter()
                .map(|(key, value)| (key, value.into()))
                .collect()
        }));
        self
    }

    /// Sets the unique columns that identify conflicts
    ///
    /// These columns determine when to update instead of insert.
    pub fn unique(&mut self, unique: Vec<Box<dyn Expression>>) -> &mut Self {
        self.unique.replace(unique);
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the upsert multi execution failed.
    pub async fn execute(&self, db: &dyn Database) -> Result<Vec<Row>, DatabaseError> {
        db.exec_upsert_multi(self).await
    }
}

/// INSERT statement for adding new rows to a table
pub struct InsertStatement<'a> {
    /// Table to insert into
    pub table_name: &'a str,
    /// Column-value pairs to insert
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
}

/// Creates a new INSERT statement builder
///
/// Constructs an INSERT statement for adding rows to the specified table.
#[must_use]
pub fn insert(table_name: &str) -> InsertStatement<'_> {
    InsertStatement {
        table_name,
        values: vec![],
    }
}

/// INSERT with an exact conflict target and atomic affected-row count.
pub struct InsertConflictStatement<'a> {
    /// Structured insert.
    pub insert: InsertStatement<'a>,
    /// Exact unique columns whose conflict alone is ignored.
    pub columns: Vec<String>,
    /// SQLite OR IGNORE semantics, distinct from targeted conflict handling.
    pub or_ignore: bool,
}
impl InsertConflictStatement<'_> {
    /// Execute atomically, returning zero for an ignored conflict and one for insertion.
    ///
    /// # Errors
    /// * Returns unsupported, invalid conflict-target, or unrelated constraint failures.
    pub async fn execute_count(&self, db: &dyn Database) -> Result<u64, DatabaseError> {
        db.exec_insert_conflict_count(self).await
    }
}

impl<'a> InsertStatement<'a> {
    /// Ignore only a conflict on the exact listed unique columns.
    #[must_use]
    pub fn on_conflict_do_nothing(
        self,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> InsertConflictStatement<'a> {
        InsertConflictStatement {
            insert: self,
            columns: columns.into_iter().map(Into::into).collect(),
            or_ignore: false,
        }
    }
    /// Use SQLite OR IGNORE for UNIQUE, primary-key, NOT NULL and CHECK conflicts.
    /// Foreign-key failures are not suppressed.
    #[must_use]
    pub const fn or_ignore(self) -> InsertConflictStatement<'a> {
        InsertConflictStatement {
            insert: self,
            columns: Vec::new(),
            or_ignore: true,
        }
    }
    /// Sets multiple column-value pairs at once
    #[must_use]
    pub fn values<T: Into<Box<dyn Expression>>>(mut self, values: Vec<(&'a str, T)>) -> Self {
        for value in values {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    /// Adds a single column-value pair to the insert
    #[must_use]
    pub fn value<T: Into<Box<dyn Expression>>>(mut self, name: &'a str, value: T) -> Self {
        self.values.push((name, value.into()));
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the insert execution failed.
    pub async fn execute(&self, db: &dyn Database) -> Result<Row, DatabaseError> {
        db.exec_insert(self).await
    }
}

/// UPDATE statement for modifying existing rows in a table
pub struct UpdateStatement<'a> {
    /// Table to update
    pub table_name: &'a str,
    /// Column-value pairs to set
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
    /// WHERE clause filters
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    /// Unique columns for conflict resolution
    pub unique: Option<&'a [&'a str]>,
    /// Maximum number of rows to update
    pub limit: Option<usize>,
}

/// Creates a new UPDATE statement builder
///
/// Constructs an UPDATE statement for modifying existing rows in the specified table.
#[must_use]
pub fn update(table_name: &str) -> UpdateStatement<'_> {
    UpdateStatement {
        table_name,
        values: vec![],
        filters: None,
        unique: None,
        limit: None,
    }
}

impl FilterableQuery for UpdateStatement<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(filters) = &mut self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
    }
}

impl<'a> UpdateStatement<'a> {
    /// Sets multiple column-value pairs to update
    #[must_use]
    pub fn values<T: Into<Box<dyn Expression>>>(mut self, values: Vec<(&'a str, T)>) -> Self {
        for value in values {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    /// Sets a single column to a new value
    #[must_use]
    pub fn value<T: Into<Box<dyn Expression>>>(mut self, name: &'a str, value: T) -> Self {
        self.values.push((name, value.into()));
        self
    }

    /// Specifies unique columns for conflict resolution
    #[must_use]
    pub const fn unique(mut self, unique: &'a [&'a str]) -> Self {
        self.unique.replace(unique);
        self
    }

    /// Limits the number of rows to update
    #[must_use]
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the update execution failed.
    pub async fn execute(&self, db: &dyn Database) -> Result<Vec<Row>, DatabaseError> {
        db.exec_update(self).await
    }

    /// # Errors
    ///
    /// Will return `Err` if the update execution failed.
    pub async fn execute_first(&self, db: &dyn Database) -> Result<Option<Row>, DatabaseError> {
        db.exec_update_first(self).await
    }
}

/// UPSERT statement for inserting or updating a row on conflict
pub struct UpsertStatement<'a> {
    /// Table to upsert into
    pub table_name: &'a str,
    /// Column-value pairs to insert/update
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
    /// WHERE clause filters for conditional upsert
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    /// Columns that form the unique constraint to detect conflicts
    pub unique: Option<&'a [&'a str]>,
    /// Maximum number of rows to upsert
    pub limit: Option<usize>,
}

/// Creates a new UPSERT statement builder
///
/// Constructs an UPSERT statement for inserting or updating rows on conflict.
#[must_use]
pub fn upsert(table_name: &str) -> UpsertStatement<'_> {
    UpsertStatement {
        table_name,
        values: vec![],
        filters: None,
        unique: None,
        limit: None,
    }
}

impl FilterableQuery for UpsertStatement<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(filters) = &mut self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
    }
}

impl<'a> UpsertStatement<'a> {
    /// Sets multiple column-value pairs to upsert
    #[must_use]
    pub fn values<T: Into<Box<dyn Expression>>>(mut self, values: Vec<(&'a str, T)>) -> Self {
        for value in values {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    /// Sets a single column-value pair to upsert
    #[must_use]
    pub fn value<T: Into<Box<dyn Expression>>>(mut self, name: &'a str, value: T) -> Self {
        self.values.push((name, value.into()));
        self
    }

    /// Sets a column-value pair only if the value is `Some`
    ///
    /// This is useful for conditional updates where a value should only be set if present.
    #[must_use]
    pub fn value_opt<T: Into<Box<dyn Expression>>>(
        mut self,
        name: &'a str,
        value: Option<T>,
    ) -> Self {
        if let Some(value) = value {
            self.values.push((name, value.into()));
        }
        self
    }

    /// Specifies unique columns that identify conflicts
    ///
    /// These columns determine when to update instead of insert.
    #[must_use]
    pub const fn unique(mut self, unique: &'a [&'a str]) -> Self {
        self.unique.replace(unique);
        self
    }

    /// Limits the number of rows affected by the upsert
    #[must_use]
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the upsert execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<Vec<Row>, DatabaseError> {
        if self.values.is_empty() {
            return db.query(&self.into()).await;
        }
        db.exec_upsert(&self).await
    }

    /// # Errors
    ///
    /// Will return `Err` if the upsert execution failed.
    pub async fn execute_first(self, db: &dyn Database) -> Result<Row, DatabaseError> {
        if self.values.is_empty() {
            return db
                .query_first(&self.into())
                .await?
                .ok_or(DatabaseError::NoRow);
        }
        db.exec_upsert_first(&self).await
    }
}

impl<'a> From<UpsertStatement<'a>> for SelectQuery<'a> {
    fn from(value: UpsertStatement<'a>) -> Self {
        Self {
            table_alias: None,
            indexed_by: None,
            table_name: value.table_name,
            distinct: false,
            columns: &["*"],
            projections: Vec::new(),
            groups: Vec::new(),
            filters: value.filters,
            joins: None,
            sorts: None,
            limit: value.limit,
        }
    }
}

/// DELETE statement for removing rows from a table
pub struct DeleteStatement<'a> {
    /// Table to delete from
    pub table_name: &'a str,
    /// WHERE clause filters
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    /// Maximum number of rows to delete
    pub limit: Option<usize>,
}

/// Creates a new DELETE statement builder
///
/// Constructs a DELETE statement for removing rows from the specified table.
#[must_use]
pub fn delete(table_name: &str) -> DeleteStatement<'_> {
    DeleteStatement {
        table_name,
        filters: None,
        limit: None,
    }
}

impl FilterableQuery for DeleteStatement<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(filters) = &mut self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
    }
}

impl DeleteStatement<'_> {
    /// Limits the number of rows to delete
    #[must_use]
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the delete execution failed.
    pub async fn execute(&self, db: &dyn Database) -> Result<Vec<Row>, DatabaseError> {
        db.exec_delete(self).await
    }

    /// # Errors
    ///
    /// Will return `Err` if the delete execution failed.
    pub async fn execute_first(&self, db: &dyn Database) -> Result<Option<Row>, DatabaseError> {
        db.exec_delete_first(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseValue, sql_interval::SqlInterval};

    mod expression_params_tests {
        use super::*;

        #[test_log::test]
        fn test_params_filters_null_values() {
            let val = DatabaseValue::Null;
            let params = val.params();
            assert!(params.is_some());
            assert!(params.unwrap().is_empty());
        }

        #[test_log::test]
        fn test_params_filters_now() {
            let val = DatabaseValue::Now;
            let params = val.params();
            assert!(params.is_some());
            assert!(params.unwrap().is_empty());
        }

        #[test_log::test]
        fn test_params_filters_now_plus() {
            let val = DatabaseValue::NowPlus(SqlInterval::from_days(1));
            let params = val.params();
            assert!(params.is_some());
            assert!(params.unwrap().is_empty());
        }

        #[test_log::test]
        fn test_params_includes_regular_values() {
            let val = DatabaseValue::Int64(42);
            let params = val.params();
            assert!(params.is_some());
            let params = params.unwrap();
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], &DatabaseValue::Int64(42));
        }

        #[test_log::test]
        fn test_params_includes_string_values() {
            let val = DatabaseValue::String("test".to_string());
            let params = val.params();
            assert!(params.is_some());
            let params = params.unwrap();
            assert_eq!(params.len(), 1);
        }
    }

    mod and_expression_tests {
        use super::*;

        #[test_log::test]
        fn test_and_with_no_conditions_returns_none() {
            let and_expr = And { conditions: vec![] };
            assert!(and_expr.values().is_none());
        }

        #[test_log::test]
        fn test_and_collects_values_from_conditions() {
            let eq1 = where_eq("col1", DatabaseValue::Int64(1));
            let eq2 = where_eq("col2", DatabaseValue::String("test".to_string()));

            let and_expr = where_and(vec![Box::new(eq1), Box::new(eq2)]);
            let values = and_expr.values();
            assert!(values.is_some());
            let values = values.unwrap();
            assert_eq!(values.len(), 2);
        }

        #[test_log::test]
        fn test_and_params_filters_non_bindable() {
            let eq1 = where_eq("col1", DatabaseValue::Now);
            let eq2 = where_eq("col2", DatabaseValue::Int64(42));

            let and_expr = where_and(vec![Box::new(eq1), Box::new(eq2)]);
            let params = and_expr.params();
            assert!(params.is_some());
            let params = params.unwrap();
            // NOW() should be filtered out
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], &DatabaseValue::Int64(42));
        }
    }

    mod or_expression_tests {
        use super::*;

        #[test_log::test]
        fn test_or_with_no_conditions_returns_none() {
            let or_expr = Or { conditions: vec![] };
            assert!(or_expr.values().is_none());
        }

        #[test_log::test]
        fn test_or_collects_values_from_conditions() {
            let eq1 = where_eq("status", DatabaseValue::String("active".to_string()));
            let eq2 = where_eq("status", DatabaseValue::String("pending".to_string()));

            let or_expr = where_or(vec![Box::new(eq1), Box::new(eq2)]);
            let values = or_expr.values();
            assert!(values.is_some());
            let values = values.unwrap();
            assert_eq!(values.len(), 2);
        }
    }

    mod in_expression_tests {
        use super::*;

        #[test_log::test]
        fn test_in_values_collection() {
            let in_expr = where_in(
                "id",
                vec![
                    DatabaseValue::Int64(1),
                    DatabaseValue::Int64(2),
                    DatabaseValue::Int64(3),
                ],
            );
            let values = in_expr.values();
            assert!(values.is_some());
            let values = values.unwrap();
            assert_eq!(values.len(), 3);
        }

        #[test_log::test]
        fn test_not_in_values_collection() {
            let not_in = where_not_in(
                "status",
                vec![
                    DatabaseValue::String("deleted".to_string()),
                    DatabaseValue::String("archived".to_string()),
                ],
            );
            let values = not_in.values();
            assert!(values.is_some());
            let values = values.unwrap();
            assert_eq!(values.len(), 2);
        }
    }

    mod coalesce_tests {
        use super::*;

        #[test_log::test]
        fn test_coalesce_empty_returns_none() {
            let coal = coalesce(vec![]);
            assert!(coal.values().is_none());
        }

        #[test_log::test]
        fn test_coalesce_collects_values() {
            let coal = coalesce(vec![
                Box::new(DatabaseValue::StringOpt(None)),
                Box::new(DatabaseValue::String("default".to_string())),
            ]);
            let values = coal.values();
            assert!(values.is_some());
            let values = values.unwrap();
            assert_eq!(values.len(), 2);
        }
    }

    mod inlist_tests {
        use super::*;

        #[test_log::test]
        fn test_inlist_empty_returns_none() {
            let list = InList { values: vec![] };
            assert!(list.values().is_none());
        }

        #[test_log::test]
        fn test_inlist_collects_values() {
            let list = InList {
                values: vec![
                    Box::new(DatabaseValue::Int64(1)),
                    Box::new(DatabaseValue::Int64(2)),
                ],
            };
            let values = list.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 2);
        }
    }

    mod comparison_expression_tests {
        use super::*;

        #[test_log::test]
        fn test_eq_values() {
            let eq = where_eq("col", DatabaseValue::Int64(42));
            let values = eq.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_not_eq_values() {
            let neq = where_not_eq("col", DatabaseValue::String("test".to_string()));
            let values = neq.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_gt_values() {
            let gt = where_gt("age", DatabaseValue::Int32(18));
            let values = gt.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_gte_values() {
            let gte = where_gte("score", DatabaseValue::Int32(100));
            let values = gte.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_lt_values() {
            let lt = where_lt("quantity", DatabaseValue::Int32(10));
            let values = lt.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_lte_values() {
            let lte = where_lte("price", DatabaseValue::Real64(99.99));
            let values = lte.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }
    }

    mod select_query_tests {
        use super::*;

        #[test_log::test]
        fn test_select_query_no_filters_returns_none() {
            let query = select("users");
            assert!(query.values().is_none());
        }

        #[test_log::test]
        fn test_select_query_with_filter_values() {
            let query = select("users").where_eq("id", DatabaseValue::Int64(1));
            let values = query.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_select_query_with_multiple_filters() {
            let query = select("users")
                .where_eq("active", DatabaseValue::Bool(true))
                .where_gt("age", DatabaseValue::Int32(18));
            let values = query.values();
            assert!(values.is_some());
            assert_eq!(values.unwrap().len(), 2);
        }
    }

    #[cfg(feature = "raw-sql")]
    mod literal_tests {
        use super::*;

        #[test_log::test]
        fn test_literal_from_str() {
            let lit: Literal = "COUNT(*)".into();
            assert_eq!(lit.value, "COUNT(*)");
        }

        #[test_log::test]
        fn test_literal_from_string() {
            let lit: Literal = String::from("SUM(amount)").into();
            assert_eq!(lit.value, "SUM(amount)");
        }

        #[test_log::test]
        fn test_literal_function() {
            let lit = literal("NOW()");
            assert_eq!(lit.value, "NOW()");
        }
    }

    #[cfg(all(test, feature = "sqlite-rusqlite"))]
    mod qualified_column_tests {
        use super::*;

        #[test]
        fn qualification_quotes_each_exact_component() {
            let column = qualified_column("a.b\"c", "d.e\"f");
            assert_eq!(column.render('"'), "\"a.b\"\"c\".\"d.e\"\"f\"");
            let column = qualified_column("a.b`c", "d.e`f");
            assert_eq!(column.render('`'), "`a.b``c`.`d.e``f`");
        }
    }

    mod identifier_tests {
        use super::*;

        #[test_log::test]
        fn test_identifier_from_str() {
            let id: Identifier = "user_id".into();
            assert_eq!(id.value, "user_id");
        }

        #[test_log::test]
        fn test_identifier_from_string() {
            let id: Identifier = String::from("column_name").into();
            assert_eq!(id.value, "column_name");
        }

        #[test_log::test]
        fn test_identifier_function() {
            let id = identifier("table.column");
            assert_eq!(id.value, "table.column");
        }
    }

    mod typed_join_tests {
        use super::*;

        #[test_log::test]
        fn typed_join_helpers_and_chaining() {
            let condition = || {
                (
                    qualified_column("orders", "user_id"),
                    qualified_column("users", "id"),
                )
            };
            let inner = join("users", condition());
            let left = left_join("users", condition());
            assert!(!inner.left);
            assert!(left.left);
            assert!(matches!(inner.on, JoinCondition::ColumnsEqual(..)));
            assert!(matches!(left.on, JoinCondition::ColumnsEqual(..)));

            let query = select("orders")
                .join("users", condition())
                .left_join("addresses", JoinCondition::from(condition()))
                .join("more", condition());
            let joins = query.joins.unwrap();
            assert_eq!(joins.len(), 3);
            assert_eq!(joins[0].table_name, "users");
            assert!(!joins[0].left);
            assert_eq!(joins[1].table_name, "addresses");
            assert!(joins[1].left);
            assert_eq!(joins[2].table_name, "more");
            assert!(!joins[2].left);
            assert!(
                joins
                    .iter()
                    .all(|join| matches!(join.on, JoinCondition::ColumnsEqual(..)))
            );
        }
    }

    #[cfg(feature = "raw-sql")]
    mod join_tests {
        use super::*;

        #[test_log::test]
        fn test_inner_join_creation() {
            let j = join("orders", "orders.user_id = users.id");
            assert_eq!(j.table_name, "orders");
            assert!(matches!(&j.on, JoinCondition::Raw(sql) if sql == "orders.user_id = users.id"));
            assert!(!j.left);
        }

        #[test_log::test]
        fn test_left_join_creation() {
            let j = left_join("addresses", "users.address_id = addresses.id");
            assert_eq!(j.table_name, "addresses");
            assert!(j.left);
        }
    }

    mod sort_tests {
        use super::*;

        #[test_log::test]
        fn test_sort_ascending() {
            let s = sort(identifier("name"), SortDirection::Asc);
            assert!(matches!(s.direction, SortDirection::Asc));
        }

        #[test_log::test]
        fn test_sort_descending() {
            let s = sort(identifier("created_at"), SortDirection::Desc);
            assert!(matches!(s.direction, SortDirection::Desc));
        }
    }

    mod boxed_macro_tests {
        use super::*;

        #[test_log::test]
        fn test_boxed_empty() {
            let v: Vec<Box<dyn BooleanExpression>> = boxed![];
            assert!(v.is_empty());
        }

        #[test_log::test]
        fn test_boxed_single() {
            let v: Vec<Box<dyn BooleanExpression>> =
                boxed![where_eq("id", DatabaseValue::Int64(1))];
            assert_eq!(v.len(), 1);
        }

        #[test_log::test]
        fn test_boxed_multiple() {
            let v: Vec<Box<dyn BooleanExpression>> = boxed![
                where_eq("id", DatabaseValue::Int64(1)),
                where_gt("age", DatabaseValue::Int32(18)),
            ];
            assert_eq!(v.len(), 2);
        }
    }

    mod filterable_query_tests {
        use super::*;

        #[test_log::test]
        fn test_filter_if_some_with_value() {
            let query =
                select("users").filter_if_some(Some(where_eq("id", DatabaseValue::Int64(1))));
            assert!(query.filters.is_some());
            assert_eq!(query.filters.unwrap().len(), 1);
        }

        #[test_log::test]
        fn test_filter_if_some_with_none() {
            let query = select("users").filter_if_some::<Eq>(None);
            assert!(query.filters.is_none());
        }

        #[test_log::test]
        fn test_filters_adds_multiple() {
            let conditions: Vec<Box<dyn BooleanExpression>> = vec![
                Box::new(where_eq("a", DatabaseValue::Int64(1))),
                Box::new(where_eq("b", DatabaseValue::Int64(2))),
            ];
            let query = select("table").filters(conditions);
            assert!(query.filters.is_some());
            assert_eq!(query.filters.unwrap().len(), 2);
        }
    }
}
