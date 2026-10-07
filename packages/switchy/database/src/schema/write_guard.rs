//! Finite cross-table rejection constraints. Identifier strings are never SQL fragments.

/// A finite, persistent after-transition projection into a separate table.
///
/// The target owns generated keys and constraints. Omitted columns use target defaults.
/// Installation requires an owned transaction; emission shares the source transaction.
/// Schema owners must avoid cycles between observation targets and source tables.
/// Only predicates evaluating true emit; false and SQL NULL do not emit.
/// Target insertion failure aborts the source statement. Generated target keys are
/// table-scoped, not per-source identifiers. This capability does not protect the
/// target from separate writes; append-only policy requires separate write guards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteObservation {
    /// Persistent schema identity; conflicting definitions are rejected.
    pub name: String,
    /// Source table identifier, never a SQL fragment.
    pub table: String,
    /// Transition observed after the source row changes.
    pub event: ObservationEvent,
    /// Finite opt-in predicate evaluated against the transition row.
    pub when: GuardPredicate,
    /// Separate destination table owned by the caller's schema.
    pub target_table: String,
    /// One to 128 unique destination mappings. Omit generated key columns.
    pub columns: Vec<ObservationColumn>,
}
/// Source row transition being observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationEvent {
    /// Observe insertion; only NEW row references are available.
    AfterInsert,
    /// Observe update; OLD and NEW row references are available.
    AfterUpdate,
    /// Observe deletion; only OLD row references are available.
    AfterDelete,
}
/// One destination column and its finite source value.
///
/// Relation column references are not permitted in destination mappings.
/// Bounded mappings accept only leaf inputs and preserve no partial text.
/// Pair bounded text with its status using identical input and limit to distinguish
/// an actual source NULL from unavailable oversized or unsupported content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationColumn {
    /// Destination column identifier.
    pub column: String,
    /// OLD/NEW source column or scalar constant, never an expression fragment.
    pub value: GuardValue,
}

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
    /// Reject a deletion using OLD values; NEW values are unavailable.
    BeforeDelete,
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
    Column {
        relation: String,
        column: String,
    },
    Constant(GuardScalar),
    /// Preserve complete TEXT within the byte limit; otherwise return NULL.
    /// The input must be a column or constant, not another bounded mapping.
    BoundedText {
        value: Box<Self>,
        max_bytes: u32,
    },
    /// Integer status for the same leaf input and bound: 0 = source NULL,
    /// 1 = complete TEXT retained, 2 = oversized TEXT, 3 = unsupported type.
    BoundedTextStatus {
        value: Box<Self>,
        max_bytes: u32,
    },
    /// Preserve integer storage exactly; otherwise return NULL. Input must be a leaf.
    IntegerOnly {
        value: Box<Self>,
    },
    /// Integer leaf status: 0 = source NULL, 1 = integer retained, 2 = unsupported.
    IntegerStatus {
        value: Box<Self>,
    },
}
/// Closed rejection predicate grammar, with SQL three-valued comparison semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardPredicate {
    Equal(GuardValue, GuardValue),
    NotEqual(GuardValue, GuardValue),
    IsNull(GuardValue),
    IsNotNull(GuardValue),
    /// True only for TEXT/BLOB whose byte length is within the inclusive limit.
    /// NULL and other storage types return false.
    ByteLengthAtMost(GuardValue, u32),
    InValues(GuardValue, Vec<GuardScalar>),
    All(Vec<Self>),
    Any(Vec<Self>),
    Exists(GuardRelation),
    /// True when the relation has no matching row.
    NotExists(GuardRelation),
    /// Reject unless the inner predicate is true. NULL is treated as false.
    Not(Box<Self>),
    /// True only for integer operands with `next == previous + 1`.
    /// NULL, non-integers, and overflow return false rather than SQL unknown.
    IntegerSuccessor {
        previous: GuardValue,
        next: GuardValue,
    },
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
