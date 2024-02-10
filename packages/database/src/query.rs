use crate::DatabaseValue;

#[derive(Debug, Clone, Copy)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub struct Sort {
    pub expression: Box<dyn Expression>,
    pub direction: SortDirection,
}

#[derive(Debug)]
pub struct Join<'a> {
    pub table_name: &'a str,
    pub on: &'a str,
    pub left: bool,
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

pub fn sort<T>(expression: T, direction: SortDirection) -> Sort
where
    T: Into<Box<dyn Expression>>,
{
    Sort {
        expression: expression.into(),
        direction,
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

pub fn where_in<'a, L>(left: L, values: &[DatabaseValue]) -> Box<dyn BooleanExpression>
where
    L: Into<Box<dyn Expression>>,
{
    Box::new(In {
        left: left.into(),
        values: values.to_vec(),
    })
}

pub struct SelectQuery {}
