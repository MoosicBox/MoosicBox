#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "rusqlite")]
pub mod rusqlite;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseValue {
    Null,
    String(String),
    StringOpt(Option<String>),
    Bool(bool),
    BoolOpt(Option<bool>),
    Number(i64),
    NumberOpt(Option<i64>),
    UNumber(u64),
    UNumberOpt(Option<u64>),
    Real(f64),
    RealOpt(Option<f64>),
    NowAdd(String),
}

impl DatabaseValue {
    fn to_sql(&self) -> String {
        match self {
            DatabaseValue::Null => format!("NULL"),
            DatabaseValue::BoolOpt(None) => format!("NULL"),
            DatabaseValue::StringOpt(None) => format!("NULL"),
            DatabaseValue::NumberOpt(None) => format!("NULL"),
            DatabaseValue::UNumberOpt(None) => format!("NULL"),
            DatabaseValue::RealOpt(None) => format!("NULL"),
            DatabaseValue::NowAdd(add) => {
                format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
            }
            _ => format!("?"),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            DatabaseValue::String(value) | DatabaseValue::StringOpt(Some(value)) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum TryFromError {
    #[error("Could not convert to type '{0}'")]
    CouldNotConvert(String),
}

impl TryFrom<DatabaseValue> for u64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) => Ok(value as u64),
            DatabaseValue::NumberOpt(Some(value)) => Ok(value as u64),
            DatabaseValue::UNumber(value) => Ok(value as u64),
            DatabaseValue::UNumberOpt(Some(value)) => Ok(value as u64),
            _ => Err(TryFromError::CouldNotConvert("u64".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) => Ok(value as i32),
            DatabaseValue::NumberOpt(Some(value)) => Ok(value as i32),
            DatabaseValue::UNumber(value) => Ok(value as i32),
            DatabaseValue::UNumberOpt(Some(value)) => Ok(value as i32),
            _ => Err(TryFromError::CouldNotConvert("i32".into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "rusqlite")]
    #[error(transparent)]
    Rusqlite(rusqlite::RusqliteDatabaseError),
}

pub struct DbConnection {
    #[cfg(feature = "rusqlite")]
    pub inner: ::rusqlite::Connection,
}

impl std::fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DbConnection")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub struct Sort {
    pub expression: Box<dyn Expression>,
    pub direction: SortDirection,
}

pub fn sort<T>(expression: T, direction: SortDirection) -> Sort
where
    T: Into<Box<dyn Expression>>,
{
    Sort {
        expression: expression.into(),
        direction,
    }
}

#[derive(Debug)]
pub struct Join<'a> {
    pub table_name: &'a str,
    pub on: &'a str,
    pub left: bool,
}

pub fn join<'a>(table_name: &'a str, on: &'a str) -> Join<'a> {
    Join {
        table_name,
        on,
        left: false,
    }
}

pub fn left_join<'a>(table_name: &'a str, on: &'a str) -> Join<'a> {
    Join {
        table_name,
        on,
        left: true,
    }
}

pub trait Expression: Send + Sync {
    fn to_sql(&self) -> String;

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        None
    }

    fn is_null(&self) -> bool {
        false
    }
}

pub struct Identifier {
    value: String,
}

impl Expression for Identifier {
    fn to_sql(&self) -> String {
        self.value.clone()
    }
}

impl Expression for DatabaseValue {
    fn to_sql(&self) -> String {
        self.to_sql()
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        Some(vec![self])
    }

    fn is_null(&self) -> bool {
        match self {
            DatabaseValue::Null => true,
            DatabaseValue::BoolOpt(None) => true,
            DatabaseValue::RealOpt(None) => true,
            DatabaseValue::StringOpt(None) => true,
            DatabaseValue::NumberOpt(None) => true,
            DatabaseValue::UNumberOpt(None) => true,
            _ => false,
        }
    }
}

impl Into<Box<dyn Expression>> for DatabaseValue {
    fn into(self) -> Box<dyn Expression> {
        Box::new(self)
    }
}

impl Into<Box<dyn Expression>> for String {
    fn into(self) -> Box<dyn Expression> {
        Box::new(Identifier { value: self })
    }
}

impl Into<Box<dyn Expression>> for &str {
    fn into(self) -> Box<dyn Expression> {
        Box::new(Identifier {
            value: self.to_string(),
        })
    }
}

pub trait BooleanExpression: Expression {}

struct And {
    conditions: Vec<Box<dyn BooleanExpression>>,
}

impl BooleanExpression for And {}
impl Expression for And {
    fn to_sql(&self) -> String {
        format!(
            "({})",
            self.conditions
                .iter()
                .map(|x| x.to_sql())
                .collect::<Vec<_>>()
                .join(" AND ")
        )
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

pub fn where_and(conditions: Vec<Box<dyn BooleanExpression>>) -> Box<dyn BooleanExpression> {
    Box::new(And { conditions })
}

struct Or {
    conditions: Vec<Box<dyn BooleanExpression>>,
}

impl BooleanExpression for Or {}
impl Expression for Or {
    fn to_sql(&self) -> String {
        format!(
            "({})",
            self.conditions
                .iter()
                .map(|x| x.to_sql())
                .collect::<Vec<_>>()
                .join(" OR ")
        )
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

pub fn where_or(conditions: Vec<Box<dyn BooleanExpression>>) -> Box<dyn BooleanExpression> {
    Box::new(Or { conditions })
}

struct NotEq {
    left: Box<dyn Expression>,
    right: Box<dyn Expression>,
}

impl BooleanExpression for NotEq {}
impl Expression for NotEq {
    fn to_sql(&self) -> String {
        if self.left.is_null() || self.right.is_null() {
            format!("({} is not {})", self.left.to_sql(), self.right.to_sql())
        } else {
            format!("({} != {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = [
            self.left.values().unwrap_or(vec![]),
            self.right.values().unwrap_or(vec![]),
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

pub fn where_not_eq<L, R>(left: L, right: R) -> Box<dyn BooleanExpression>
where
    L: Into<Box<dyn Expression>>,
    R: Into<Box<dyn Expression>>,
{
    Box::new(NotEq {
        left: left.into(),
        right: right.into(),
    })
}

struct Eq {
    left: Box<dyn Expression>,
    right: Box<dyn Expression>,
}

impl BooleanExpression for Eq {}
impl Expression for Eq {
    fn to_sql(&self) -> String {
        if self.left.is_null() || self.right.is_null() {
            format!("({} is {})", self.left.to_sql(), self.right.to_sql())
        } else {
            format!("({} = {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = [
            self.left.values().unwrap_or(vec![]),
            self.right.values().unwrap_or(vec![]),
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

pub fn where_eq<L, R>(left: L, right: R) -> Box<dyn BooleanExpression>
where
    L: Into<Box<dyn Expression>>,
    R: Into<Box<dyn Expression>>,
{
    Box::new(Eq {
        left: left.into(),
        right: right.into(),
    })
}

struct In {
    left: Box<dyn Expression>,
    values: Vec<DatabaseValue>,
}

impl BooleanExpression for In {}
impl Expression for In {
    fn to_sql(&self) -> String {
        if self.values.is_empty() {
            "false".to_string()
        } else {
            format!(
                "({} IN ({}))",
                self.left.to_sql(),
                self.values
                    .iter()
                    .map(|value| value.to_sql())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = [
            self.left.values().unwrap_or(vec![]),
            self.values.iter().collect(),
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

pub fn where_in<'a, L>(left: L, values: &[DatabaseValue]) -> Box<dyn BooleanExpression>
where
    L: Into<Box<dyn Expression>>,
{
    Box::new(In {
        left: left.into(),
        values: values.to_vec(),
    })
}

#[derive(Debug)]
pub struct Row {
    pub columns: Vec<(String, DatabaseValue)>,
}

impl Row {
    pub fn get(&self, column_name: &str) -> Option<DatabaseValue> {
        self.columns
            .iter()
            .find(|c| c.0 == column_name)
            .map(|c| c.1.clone())
    }

    pub fn id(&self) -> Option<DatabaseValue> {
        self.get("id")
    }
}

#[async_trait]
pub trait Database: Send + Sync {
    async fn select(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn select_distinct(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn select_first(
        &self,
        table_name: &str,
        columns: &[&str],
        filters: Option<&[Box<dyn BooleanExpression>]>,
        joins: Option<&[Join]>,
        sort: Option<&[Sort]>,
    ) -> Result<Option<Row>, DatabaseError>;

    async fn delete(
        &self,
        table_name: &str,
        filters: Option<&[Box<dyn BooleanExpression>]>,
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn upsert(
        &self,
        table_name: &str,
        values: &[(&str, DatabaseValue)],
        filters: Option<&[Box<dyn BooleanExpression>]>,
    ) -> Result<Row, DatabaseError>;

    async fn upsert_multi(
        &self,
        table_name: &str,
        unique: &[&str],
        values: &[Vec<(&str, DatabaseValue)>],
    ) -> Result<Vec<Row>, DatabaseError>;

    async fn update_and_get_row(
        &self,
        table_name: &str,
        id: DatabaseValue,
        values: &[(&str, DatabaseValue)],
    ) -> Result<Option<Row>, DatabaseError>;
}
