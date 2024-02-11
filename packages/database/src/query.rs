use std::fmt::Debug;

use crate::{Database, DatabaseError, DatabaseValue, Row};

#[derive(Debug, Clone, Copy)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug)]
pub struct Sort {
    pub expression: Box<dyn Expression>,
    pub direction: SortDirection,
}

impl Expression for Sort {
    fn to_sql(&self) -> String {
        format!(
            "({}) {}",
            self.expression.to_sql(),
            match self.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct Join<'a> {
    pub table_name: &'a str,
    pub on: &'a str,
    pub left: bool,
}

impl Expression for Join<'_> {
    fn to_sql(&self) -> String {
        format!(
            "{} JOIN {} ON {}",
            if self.left { "LEFT" } else { "" },
            self.table_name,
            self.on
        )
    }
}

pub trait Expression: Send + Sync + Debug {
    fn to_sql(&self) -> String;
    fn to_param(&self) -> String {
        self.to_sql()
    }
    fn to_param_offset(&self, offset: u16) -> String {
        let param = self.to_param();

        if param == "?" {
            format!("{param}{offset}")
        } else {
            param
        }
    }

    fn params(&self) -> Option<Vec<&DatabaseValue>> {
        self.values().map(|x| {
            x.into_iter()
                .filter(|value| !value.is_null())
                .collect::<Vec<_>>()
        })
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        None
    }

    fn is_null(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct Identifier {
    pub value: String,
}

impl Into<Identifier> for &str {
    fn into(self) -> Identifier {
        Identifier {
            value: self.to_string(),
        }
    }
}

impl Into<Identifier> for String {
    fn into(self) -> Identifier {
        Identifier {
            value: self.clone(),
        }
    }
}

impl Into<Box<dyn Expression>> for Identifier {
    fn into(self) -> Box<dyn Expression> {
        Box::new(self)
    }
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

    fn to_param(&self) -> String {
        match self {
            DatabaseValue::Null => "NULL".to_string(),
            DatabaseValue::BoolOpt(None) => "NULL".to_string(),
            DatabaseValue::StringOpt(None) => "NULL".to_string(),
            DatabaseValue::NumberOpt(None) => "NULL".to_string(),
            DatabaseValue::UNumberOpt(None) => "NULL".to_string(),
            DatabaseValue::RealOpt(None) => "NULL".to_string(),
            DatabaseValue::NowAdd(add) => {
                format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
            }
            _ => "?".to_string(),
        }
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

impl<T: Into<DatabaseValue>> From<T> for Box<dyn Expression> {
    fn from(value: T) -> Self {
        Box::new(value.into())
    }
}

pub trait BooleanExpression: Expression {}

#[derive(Debug)]
pub struct And {
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

#[derive(Debug)]
pub struct Or {
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

#[derive(Debug)]
pub struct NotEq {
    left: Identifier,
    right: Box<dyn Expression>,
}

impl BooleanExpression for NotEq {}
impl Expression for NotEq {
    fn to_sql(&self) -> String {
        if self.right.is_null() {
            format!("({} is {})", self.left.to_sql(), self.right.to_sql())
        } else {
            format!("({} = {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Eq {
    left: Identifier,
    right: Box<dyn Expression>,
}

impl BooleanExpression for Eq {}
impl Expression for Eq {
    fn to_sql(&self) -> String {
        if self.right.is_null() {
            format!("({} is {})", self.left.to_sql(), self.right.to_sql())
        } else {
            format!("({} = {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Gt {
    left: Identifier,
    right: Box<dyn Expression>,
}

impl BooleanExpression for Gt {}
impl Expression for Gt {
    fn to_sql(&self) -> String {
        if self.right.is_null() {
            panic!("Invalid > comparison with NULL");
        } else {
            format!("({} > {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Gte {
    left: Identifier,
    right: Box<dyn Expression>,
}

impl BooleanExpression for Gte {}
impl Expression for Gte {
    fn to_sql(&self) -> String {
        if self.right.is_null() {
            panic!("Invalid >= comparison with NULL");
        } else {
            format!("({} >= {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Lt {
    left: Identifier,
    right: Box<dyn Expression>,
}

impl BooleanExpression for Lt {}
impl Expression for Lt {
    fn to_sql(&self) -> String {
        if self.right.is_null() {
            panic!("Invalid < comparison with NULL");
        } else {
            format!("({} < {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Lte {
    left: Identifier,
    right: Box<dyn Expression>,
}

impl BooleanExpression for Lte {}
impl Expression for Lte {
    fn to_sql(&self) -> String {
        if self.right.is_null() {
            panic!("Invalid <= comparison with NULL");
        } else {
            format!("({} <= {})", self.left.to_sql(), self.right.to_sql())
        }
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct In<'a> {
    left: Identifier,
    values: Box<dyn List + 'a>,
}

impl BooleanExpression for In<'_> {}
impl Expression for In<'_> {
    fn to_sql(&self) -> String {
        format!("{} IN ({})", self.left.to_sql(), self.values.to_sql())
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = [
            self.left.values().unwrap_or(vec![]),
            self.values.values().unwrap_or(vec![]),
        ]
        .concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
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

pub fn where_and(conditions: Vec<Box<dyn BooleanExpression>>) -> And {
    And { conditions }
}

pub fn where_or(conditions: Vec<Box<dyn BooleanExpression>>) -> Or {
    Or { conditions }
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

#[derive(Debug)]
pub struct InList {
    pub values: Vec<Box<dyn Expression>>,
}

impl List for InList {}
impl Expression for InList {
    fn to_sql(&self) -> String {
        format!(
            "{}",
            self.values
                .iter()
                .map(|value| value.to_sql())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let values = self
            .values
            .iter()
            .flat_map(|x| x.values().unwrap_or(vec![]))
            .collect::<Vec<_>>();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

pub trait List: Expression {}

impl<T> Into<Box<dyn List>> for Vec<T>
where
    T: Into<Box<dyn Expression>> + Send + Sync,
{
    fn into(self) -> Box<dyn List> {
        Box::new(InList {
            values: self.into_iter().map(|x| x.into()).collect(),
        })
    }
}

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

impl<'a> Into<Box<dyn List + 'a>> for SelectQuery<'a> {
    fn into(self) -> Box<dyn List + 'a> {
        Box::new(self)
    }
}

#[derive(Debug)]
pub struct SelectQuery<'a> {
    pub table_name: &'a str,
    pub distinct: bool,
    pub columns: &'a [&'a str],
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub joins: Option<Vec<Join<'a>>>,
    pub sorts: Option<Vec<Sort>>,
    pub limit: Option<usize>,
}

impl List for SelectQuery<'_> {}
impl Expression for SelectQuery<'_> {
    fn to_sql(&self) -> String {
        let joins = if let Some(joins) = &self.joins {
            joins
                .iter()
                .map(|x| x.to_sql())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            "".to_string()
        };

        let where_clause = if let Some(filters) = &self.filters {
            if filters.is_empty() {
                "".to_string()
            } else {
                format!(
                    "WHERE {}",
                    filters
                        .iter()
                        .map(|x| format!("({})", x.to_sql()))
                        .collect::<Vec<_>>()
                        .join(" AND ")
                )
            }
        } else {
            "".to_string()
        };

        let sort_clause = if let Some(sorts) = &self.sorts {
            if sorts.is_empty() {
                "".to_string()
            } else {
                format!(
                    "ORDER BY {}",
                    sorts
                        .iter()
                        .map(|x| x.to_sql())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        } else {
            "".to_string()
        };

        let limit = if let Some(limit) = self.limit {
            format!("LIMIT {}", limit)
        } else {
            "".to_string()
        };

        format!(
            "SELECT {} {} FROM {} {} {} {} {}",
            if self.distinct { "DISTINCT" } else { "" },
            self.columns.join(", "),
            self.table_name,
            joins,
            where_clause,
            sort_clause,
            limit
        )
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        let joins_values = self
            .joins
            .as_ref()
            .map(|x| {
                x.into_iter()
                    .flat_map(|j| j.values().unwrap_or(vec![]))
                    .collect()
            })
            .unwrap_or(vec![]);
        let filters_values = self
            .filters
            .as_ref()
            .map(|x| {
                x.into_iter()
                    .flat_map(|j| j.values().unwrap_or(vec![]))
                    .collect()
            })
            .unwrap_or(vec![]);
        let sorts_values = self
            .sorts
            .as_ref()
            .map(|x| {
                x.into_iter()
                    .flat_map(|j| j.values().unwrap_or(vec![]))
                    .collect()
            })
            .unwrap_or(vec![]);

        let values = [joins_values, filters_values, sorts_values].concat();

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }
}

pub fn select<'a>(table_name: &'a str) -> SelectQuery<'a> {
    SelectQuery {
        table_name,
        distinct: false,
        columns: &["*"],
        filters: None,
        joins: None,
        sorts: None,
        limit: None,
    }
}

impl<'a> SelectQuery<'a> {
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    pub fn columns(mut self, columns: &'a [&'a str]) -> Self {
        self.columns = columns;
        self
    }

    pub fn filters(mut self, filters: Vec<Box<dyn BooleanExpression>>) -> Self {
        for filter in filters.into_iter() {
            if let Some(ref mut filters) = self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(mut self, filter: T) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(mut self, filter: Option<T>) -> Self {
        if let Some(filter) = filter {
            if let Some(ref mut filters) = self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn joins(mut self, joins: Vec<Join<'a>>) -> Self {
        for join in joins.into_iter() {
            if let Some(ref mut joins) = self.joins {
                joins.push(join);
            } else {
                self.joins.replace(vec![join]);
            }
        }
        self
    }

    pub fn join(mut self, table_name: &'a str, on: &'a str) -> Self {
        if let Some(ref mut joins) = self.joins {
            joins.push(join(table_name, on));
        } else {
            self.joins.replace(vec![join(table_name, on)]);
        }
        self
    }

    pub fn left_joins(mut self, left_joins: Vec<Join<'a>>) -> Self {
        for left_join in left_joins.into_iter() {
            if let Some(ref mut left_joins) = self.joins {
                left_joins.push(left_join);
            } else {
                self.joins.replace(vec![left_join]);
            }
        }
        self
    }

    pub fn left_join(mut self, table_name: &'a str, on: &'a str) -> Self {
        if let Some(ref mut left_joins) = self.joins {
            left_joins.push(left_join(table_name, on));
        } else {
            self.joins.replace(vec![left_join(table_name, on)]);
        }
        self
    }

    pub fn sorts(mut self, sorts: Vec<Sort>) -> Self {
        for sort in sorts.into_iter() {
            if let Some(ref mut sorts) = self.sorts {
                sorts.push(sort);
            } else {
                self.sorts.replace(vec![sort]);
            }
        }
        self
    }

    pub fn sort<T>(mut self, expression: T, direction: SortDirection) -> Self
    where
        T: Into<Identifier>,
    {
        if let Some(ref mut sorts) = self.sorts {
            sorts.push(sort(expression.into(), direction));
        } else {
            self.sorts.replace(vec![sort(expression.into(), direction)]);
        }
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    pub async fn execute(self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.query(&self).await
    }

    pub async fn execute_first(self, db: &Box<dyn Database>) -> Result<Option<Row>, DatabaseError> {
        let this = if self.limit.is_none() {
            self.limit(1)
        } else {
            self
        };

        db.query_first(&this).await
    }
}

pub struct UpsertMultiStatement<'a> {
    pub table_name: &'a str,
    pub values: Vec<Vec<(&'a str, Box<dyn Expression>)>>,
    pub unique: Option<&'a [&'a str]>,
}

pub fn upsert_multi<'a>(table_name: &'a str) -> UpsertMultiStatement<'a> {
    UpsertMultiStatement {
        table_name,
        values: vec![],
        unique: None,
    }
}

impl<'a> UpsertMultiStatement<'a> {
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

    pub fn unique(&mut self, unique: &'a [&'a str]) -> &mut Self {
        self.unique.replace(unique);
        self
    }

    pub async fn execute(&mut self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_upsert_multi(self).await
    }
}

pub struct InsertStatement<'a> {
    pub table_name: &'a str,
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
}

pub fn insert<'a>(table_name: &'a str) -> InsertStatement<'a> {
    InsertStatement {
        table_name,
        values: vec![],
    }
}

impl<'a> InsertStatement<'a> {
    pub fn values<T: Into<Box<dyn Expression>>>(mut self, values: Vec<(&'a str, T)>) -> Self {
        for value in values.into_iter() {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    pub fn value<T: Into<Box<dyn Expression>>>(mut self, name: &'a str, value: T) -> Self {
        self.values.push((name, value.into()));
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Row, DatabaseError> {
        db.exec_insert(self).await
    }
}

pub struct UpdateStatement<'a> {
    pub table_name: &'a str,
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub unique: Option<&'a [&'a str]>,
    pub limit: Option<usize>,
}

pub fn update<'a>(table_name: &'a str) -> UpdateStatement<'a> {
    UpdateStatement {
        table_name,
        values: vec![],
        filters: None,
        unique: None,
        limit: None,
    }
}

impl<'a> UpdateStatement<'a> {
    pub fn values<T: Into<Box<dyn Expression>>>(mut self, values: Vec<(&'a str, T)>) -> Self {
        for value in values.into_iter() {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    pub fn value<T: Into<Box<dyn Expression>>>(mut self, name: &'a str, value: T) -> Self {
        self.values.push((name, value.into()));
        self
    }

    pub fn filters(mut self, filters: Vec<Box<dyn BooleanExpression>>) -> Self {
        for filter in filters.into_iter() {
            if let Some(ref mut filters) = self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(mut self, filter: T) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(mut self, filter: Option<T>) -> Self {
        if let Some(filter) = filter {
            if let Some(ref mut filters) = self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn unique(mut self, unique: &'a [&'a str]) -> Self {
        self.unique.replace(unique);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_update(self).await
    }

    pub async fn execute_first(
        &self,
        db: &Box<dyn Database>,
    ) -> Result<Option<Row>, DatabaseError> {
        db.exec_update_first(self).await
    }
}

pub struct UpsertStatement<'a> {
    pub table_name: &'a str,
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub unique: Option<&'a [&'a str]>,
    pub limit: Option<usize>,
}

pub fn upsert<'a>(table_name: &'a str) -> UpsertStatement<'a> {
    UpsertStatement {
        table_name,
        values: vec![],
        filters: None,
        unique: None,
        limit: None,
    }
}

impl<'a> UpsertStatement<'a> {
    pub fn values<T: Into<Box<dyn Expression>>>(mut self, values: Vec<(&'a str, T)>) -> Self {
        for value in values.into_iter() {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    pub fn value<T: Into<Box<dyn Expression>>>(mut self, name: &'a str, value: T) -> Self {
        self.values.push((name, value.into()));
        self
    }

    pub fn filters(mut self, filters: Vec<Box<dyn BooleanExpression>>) -> Self {
        for filter in filters.into_iter() {
            if let Some(ref mut filters) = self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(mut self, filter: T) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(mut self, filter: Option<T>) -> Self {
        if let Some(filter) = filter {
            if let Some(ref mut filters) = self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn unique(mut self, unique: &'a [&'a str]) -> Self {
        self.unique.replace(unique);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_upsert(self).await
    }

    pub async fn execute_first(&self, db: &Box<dyn Database>) -> Result<Row, DatabaseError> {
        db.exec_upsert_first(self).await
    }
}

pub struct DeleteStatement<'a> {
    pub table_name: &'a str,
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub limit: Option<usize>,
}

pub fn delete<'a>(table_name: &'a str) -> DeleteStatement<'a> {
    DeleteStatement {
        table_name,
        filters: None,
        limit: None,
    }
}

impl<'a> DeleteStatement<'a> {
    pub fn filters(mut self, filters: Vec<Box<dyn BooleanExpression>>) -> Self {
        for filter in filters.into_iter() {
            if let Some(ref mut filters) = self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(mut self, filter: Option<T>) -> Self {
        if let Some(filter) = filter {
            if let Some(ref mut filters) = self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(mut self, filter: T) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_delete(self).await
    }
}
