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
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Sort(self)
    }
}

#[derive(Debug, Clone)]
pub struct Join<'a> {
    pub table_name: &'a str,
    pub on: &'a str,
    pub left: bool,
}

impl Expression for Join<'_> {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Join(self)
    }
}

pub enum ExpressionType<'a> {
    Eq(&'a Eq),
    Gt(&'a Gt),
    In(&'a In<'a>),
    Lt(&'a Lt),
    Or(&'a Or),
    And(&'a And),
    Gte(&'a Gte),
    Lte(&'a Lte),
    Join(&'a Join<'a>),
    Sort(&'a Sort),
    NotEq(&'a NotEq),
    InList(&'a InList),
    Literal(&'a Literal),
    Coalesce(&'a Coalesce),
    Identifier(&'a Identifier),
    SelectQuery(&'a SelectQuery<'a>),
    DatabaseValue(&'a DatabaseValue),
}

pub trait Expression: Send + Sync + Debug {
    fn expression_type(&self) -> ExpressionType;

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
pub struct Literal {
    pub value: String,
}

impl Into<Literal> for &str {
    fn into(self) -> Literal {
        Literal {
            value: self.to_string(),
        }
    }
}

impl Into<Literal> for String {
    fn into(self) -> Literal {
        Literal {
            value: self.clone(),
        }
    }
}

impl Into<Box<dyn Expression>> for Literal {
    fn into(self) -> Box<dyn Expression> {
        Box::new(self)
    }
}

impl Expression for Literal {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Literal(self)
    }
}

pub fn literal(value: &str) -> Literal {
    Literal {
        value: value.to_string(),
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
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Identifier(self)
    }
}

pub fn identifier(value: &str) -> Identifier {
    Identifier {
        value: value.to_string(),
    }
}

impl Expression for DatabaseValue {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::DatabaseValue(self)
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
    pub(crate) conditions: Vec<Box<dyn BooleanExpression>>,
}

impl BooleanExpression for And {}
impl Expression for And {
    fn expression_type(&self) -> ExpressionType {
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

#[derive(Debug)]
pub struct Or {
    pub(crate) conditions: Vec<Box<dyn BooleanExpression>>,
}

impl BooleanExpression for Or {}
impl Expression for Or {
    fn expression_type(&self) -> ExpressionType {
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

#[derive(Debug)]
pub struct NotEq {
    pub(crate) left: Identifier,
    pub(crate) right: Box<dyn Expression>,
}

impl BooleanExpression for NotEq {}
impl Expression for NotEq {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::NotEq(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Eq {
    pub(crate) left: Identifier,
    pub(crate) right: Box<dyn Expression>,
}

impl BooleanExpression for Eq {}
impl Expression for Eq {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Eq(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Gt {
    pub(crate) left: Identifier,
    pub(crate) right: Box<dyn Expression>,
}

impl BooleanExpression for Gt {}
impl Expression for Gt {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Gt(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Gte {
    pub(crate) left: Identifier,
    pub(crate) right: Box<dyn Expression>,
}

impl BooleanExpression for Gte {}
impl Expression for Gte {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Gte(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Lt {
    pub(crate) left: Identifier,
    pub(crate) right: Box<dyn Expression>,
}

impl BooleanExpression for Lt {}
impl Expression for Lt {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Lt(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct Lte {
    pub(crate) left: Identifier,
    pub(crate) right: Box<dyn Expression>,
}

impl BooleanExpression for Lte {}
impl Expression for Lte {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Lte(self)
    }

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        self.right.values()
    }
}

#[derive(Debug)]
pub struct In<'a> {
    pub(crate) left: Identifier,
    pub(crate) values: Box<dyn List + 'a>,
}

impl BooleanExpression for In<'_> {}
impl Expression for In<'_> {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::In(self)
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
pub struct Coalesce {
    pub values: Vec<Box<dyn Expression>>,
}

impl List for Coalesce {}
impl Expression for Coalesce {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::Coalesce(self)
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

pub fn coalesce<'a>(values: Vec<Box<dyn Expression>>) -> Coalesce {
    Coalesce { values }
}

#[derive(Debug)]
pub struct InList {
    pub values: Vec<Box<dyn Expression>>,
}

impl List for InList {}
impl Expression for InList {
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::InList(self)
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

#[macro_export]
macro_rules! boxed {
    () => (
        Vec::new()
    );
    ($($x:expr),+ $(,)?) => (
        vec![$(Box::new($x)),+]
    );
}

pub trait FilterableQuery
where
    Self: Sized,
{
    fn filters(self, filters: Vec<Box<dyn BooleanExpression>>) -> Self {
        let mut this = self;
        for filter in filters.into_iter() {
            this = this.filter(filter)
        }
        this
    }

    fn filter(self, filter: Box<dyn BooleanExpression>) -> Self;

    fn filter_if_some<T: BooleanExpression + 'static>(self, filter: Option<T>) -> Self {
        if let Some(filter) = filter {
            self.filter(Box::new(filter))
        } else {
            self
        }
    }

    fn where_in<L, V>(self, left: L, values: V) -> Self
    where
        L: Into<Identifier>,
        V: Into<Box<dyn List>>,
    {
        self.filter(Box::new(where_in(left, values)))
    }

    fn where_and(self, conditions: Vec<Box<dyn BooleanExpression>>) -> Self {
        self.filter(Box::new(where_and(conditions)))
    }

    fn where_or(self, conditions: Vec<Box<dyn BooleanExpression>>) -> Self {
        self.filter(Box::new(where_or(conditions)))
    }

    fn where_eq<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_eq(left, right)))
    }

    fn where_not_eq<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_not_eq(left, right)))
    }

    fn where_gt<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_gt(left, right)))
    }

    fn where_gte<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_gte(left, right)))
    }

    fn where_lt<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_lt(left, right)))
    }

    fn where_lte<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<Identifier>,
        R: Into<Box<dyn Expression>>,
    {
        self.filter(Box::new(where_lte(left, right)))
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
    fn expression_type(&self) -> ExpressionType {
        ExpressionType::SelectQuery(self)
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

impl FilterableQuery for SelectQuery<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
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
    pub unique: Option<Vec<Box<dyn Expression>>>,
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

    pub fn unique(&mut self, unique: Vec<Box<dyn Expression>>) -> &mut Self {
        self.unique
            .replace(unique.into_iter().map(|x| x.into()).collect::<Vec<_>>());
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

impl FilterableQuery for UpdateStatement<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
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

impl FilterableQuery for UpsertStatement<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
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

impl FilterableQuery for DeleteStatement<'_> {
    fn filter(mut self, filter: Box<dyn BooleanExpression>) -> Self {
        if let Some(ref mut filters) = self.filters {
            filters.push(filter);
        } else {
            self.filters.replace(vec![filter]);
        }
        self
    }
}

impl<'a> DeleteStatement<'a> {
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit.replace(limit);
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_delete(self).await
    }
}
