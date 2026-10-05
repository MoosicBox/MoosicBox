//! Finite cross-table rejection constraints. Identifier strings are never SQL fragments.

/// A persistent, version-one rejection constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteGuard {
    pub name: String,
    pub table: String,
    pub event: GuardEvent,
    pub reject_when: GuardPredicate,
    pub violation: String,
}
/// Row transitions supported by guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardEvent {
    BeforeInsert,
    BeforeUpdate,
}
/// Scalar constants supported independently of raw query features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardScalar {
    Null,
    Integer(i64),
    Text(String),
    Bool(bool),
}
/// A column reference or scalar constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardValue {
    NewColumn(String),
    OldColumn(String),
    Column { relation: String, column: String },
    Constant(GuardScalar),
}
/// Closed rejection predicate grammar, with SQL three-valued comparison semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardPredicate {
    Equal(GuardValue, GuardValue),
    NotEqual(GuardValue, GuardValue),
    IsNull(GuardValue),
    IsNotNull(GuardValue),
    InValues(GuardValue, Vec<GuardScalar>),
    All(Vec<Self>),
    Any(Vec<Self>),
    Exists(GuardRelation),
}
/// An existence probe; relation aliases are scoped to this probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardRelation {
    pub table: String,
    pub alias: String,
    pub joins: Vec<GuardJoin>,
    pub predicate: Box<GuardPredicate>,
}
/// An inner equality join; both operands must be bound relation columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardJoin {
    pub table: String,
    pub alias: String,
    pub left: GuardValue,
    pub right: GuardValue,
}
