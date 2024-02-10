use std::fmt::Debug;

use crate::{Database, DatabaseError, DatabaseValue, Row};

#[derive(Debug, Clone, Copy)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub struct Sort {
    pub expression: Box<dyn Expression>,
    pub direction: SortDirection,
}

#[derive(Debug, Clone)]
pub struct Join<'a> {
    pub table_name: &'a str,
    pub on: &'a str,
    pub left: bool,
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

    fn values(&self) -> Option<Vec<&DatabaseValue>> {
        None
    }

    fn is_null(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct Identifier {
    value: String,
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

/*impl<T: Into<DatabaseValue>> Into<Box<dyn Expression>> for T {
    fn into(self) -> Box<dyn Expression> {
        Box::new(self.into())
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
}*/

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
pub struct In {
    left: Identifier,
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

pub fn where_and(conditions: Vec<Box<dyn BooleanExpression>>) -> Box<dyn BooleanExpression> {
    Box::new(And { conditions })
}

pub fn where_or(conditions: Vec<Box<dyn BooleanExpression>>) -> Box<dyn BooleanExpression> {
    Box::new(Or { conditions })
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

pub fn where_in<'a, L, V: Into<DatabaseValue>>(left: L, values: Vec<V>) -> In
where
    L: Into<Identifier>,
{
    In {
        left: left.into(),
        values: values.into_iter().map(|x| x.into()).collect(),
    }
}

pub struct SelectQuery<'a> {
    pub table_name: &'a str,
    pub distinct: bool,
    pub columns: &'a [&'a str],
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub joins: Option<Vec<Join<'a>>>,
    pub sorts: Option<Vec<Sort>>,
}

pub fn select<'a>(table_name: &'a str) -> SelectQuery<'a> {
    SelectQuery {
        table_name,
        distinct: false,
        columns: &["*"],
        filters: None,
        joins: None,
        sorts: None,
    }
}

impl<'a> SelectQuery<'a> {
    pub fn distinct(&mut self) -> &mut Self {
        self.distinct = true;
        self
    }

    pub fn columns(&mut self, columns: &'a [&'a str]) -> &mut Self {
        self.columns = columns;
        self
    }

    pub fn filters(&mut self, filters: Vec<Box<dyn BooleanExpression>>) -> &mut Self {
        for filter in filters.into_iter() {
            if let Some(filters) = &mut self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(&mut self, filter: T) -> &mut Self {
        if let Some(filters) = &mut self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(&mut self, filter: Option<T>) -> &mut Self {
        if let Some(filter) = filter {
            if let Some(filters) = &mut self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn joins(&mut self, joins: Vec<Join<'a>>) -> &mut Self {
        for join in joins.into_iter() {
            if let Some(joins) = &mut self.joins {
                joins.push(join);
            } else {
                self.joins.replace(vec![join]);
            }
        }
        self
    }

    pub fn join(&mut self, table_name: &'a str, on: &'a str) -> &mut Self {
        if let Some(joins) = &mut self.joins {
            joins.push(join(table_name, on));
        } else {
            self.joins.replace(vec![join(table_name, on)]);
        }
        self
    }

    pub fn left_joins(&mut self, left_joins: Vec<Join<'a>>) -> &mut Self {
        for left_join in left_joins.into_iter() {
            if let Some(left_joins) = &mut self.joins {
                left_joins.push(left_join);
            } else {
                self.joins.replace(vec![left_join]);
            }
        }
        self
    }

    pub fn left_join(&mut self, table_name: &'a str, on: &'a str) -> &mut Self {
        if let Some(left_joins) = &mut self.joins {
            left_joins.push(left_join(table_name, on));
        } else {
            self.joins.replace(vec![left_join(table_name, on)]);
        }
        self
    }

    pub fn sorts(&mut self, sorts: Vec<Sort>) -> &mut Self {
        for sort in sorts.into_iter() {
            if let Some(sorts) = &mut self.sorts {
                sorts.push(sort);
            } else {
                self.sorts.replace(vec![sort]);
            }
        }
        self
    }

    pub fn sort<T>(&mut self, expression: T, direction: SortDirection) -> &mut Self
    where
        T: Into<Box<dyn Expression>>,
    {
        if let Some(sorts) = &mut self.sorts {
            sorts.push(sort(expression, direction));
        } else {
            self.sorts.replace(vec![sort(expression, direction)]);
        }
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.query(self).await
    }

    pub async fn execute_first(
        &self,
        db: &Box<dyn Database>,
    ) -> Result<Option<Row>, DatabaseError> {
        db.query_first(self).await
    }
}

pub struct UpdateMultiStatement<'a> {
    pub table_name: &'a str,
    pub values: Vec<Vec<(&'a str, Box<dyn Expression>)>>,
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub unique: Option<&'a [&'a str]>,
}

pub fn update_multi<'a>(table_name: &'a str) -> UpdateMultiStatement<'a> {
    UpdateMultiStatement {
        table_name,
        values: vec![],
        filters: None,
        unique: None,
    }
}

impl<'a> UpdateMultiStatement<'a> {
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

    pub fn filters(&mut self, filters: Vec<Box<dyn BooleanExpression>>) -> &mut Self {
        for filter in filters.into_iter() {
            if let Some(filters) = &mut self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(&mut self, filter: T) -> &mut Self {
        if let Some(filters) = &mut self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn unique(&mut self, unique: &'a [&'a str]) -> &mut Self {
        self.unique.replace(unique);
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_update_multi(self).await
    }
}

pub struct UpdateStatement<'a> {
    pub table_name: &'a str,
    pub values: Vec<(&'a str, Box<dyn Expression>)>,
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
    pub unique: Option<&'a [&'a str]>,
}

pub fn update<'a>(table_name: &'a str) -> UpdateStatement<'a> {
    UpdateStatement {
        table_name,
        values: vec![],
        filters: None,
        unique: None,
    }
}

impl<'a> UpdateStatement<'a> {
    pub fn values<T: Into<Box<dyn Expression>>>(&mut self, values: Vec<(&'a str, T)>) -> &mut Self {
        for value in values.into_iter() {
            self.values.push((value.0, value.1.into()));
        }
        self
    }

    pub fn value<T: Into<Box<dyn Expression>>>(&mut self, name: &'a str, value: T) -> &mut Self {
        self.values.push((name, value.into()));
        self
    }

    pub fn filters(&mut self, filters: Vec<Box<dyn BooleanExpression>>) -> &mut Self {
        for filter in filters.into_iter() {
            if let Some(filters) = &mut self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(&mut self, filter: T) -> &mut Self {
        if let Some(filters) = &mut self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(&mut self, filter: Option<T>) -> &mut Self {
        if let Some(filter) = filter {
            if let Some(filters) = &mut self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn unique(&mut self, unique: &'a [&'a str]) -> &mut Self {
        self.unique.replace(unique);
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

pub struct DeleteStatement<'a> {
    pub table_name: &'a str,
    pub filters: Option<Vec<Box<dyn BooleanExpression>>>,
}

pub fn delete<'a>(table_name: &'a str) -> DeleteStatement<'a> {
    DeleteStatement {
        table_name,
        filters: None,
    }
}

impl<'a> DeleteStatement<'a> {
    pub fn filters(&mut self, filters: Vec<Box<dyn BooleanExpression>>) -> &mut Self {
        for filter in filters.into_iter() {
            if let Some(filters) = &mut self.filters {
                filters.push(filter);
            } else {
                self.filters.replace(vec![filter]);
            }
        }
        self
    }

    pub fn filter_some<T: BooleanExpression + 'static>(&mut self, filter: Option<T>) -> &mut Self {
        if let Some(filter) = filter {
            if let Some(filters) = &mut self.filters {
                filters.push(Box::new(filter));
            } else {
                self.filters.replace(vec![Box::new(filter)]);
            }
        }
        self
    }

    pub fn filter<T: BooleanExpression + 'static>(&mut self, filter: T) -> &mut Self {
        if let Some(filters) = &mut self.filters {
            filters.push(Box::new(filter));
        } else {
            self.filters.replace(vec![Box::new(filter)]);
        }
        self
    }

    pub async fn execute(&self, db: &Box<dyn Database>) -> Result<Vec<Row>, DatabaseError> {
        db.exec_delete(self).await
    }
}
