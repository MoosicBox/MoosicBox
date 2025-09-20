use serde::{Deserialize, Serialize};

/// Expression AST for GitHub Actions compatible expressions.
///
/// Supports the complete expression language as defined in the specification:
/// * MVP functions: `toJson()`, `fromJson()`, `contains()`, `startsWith()`, `join()`, `format()`
/// * Operators: `==`, `!=`, `&&`, `||`, `!`, property access with `.`
/// * Complete Expression enum with all node types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// String literal value
    String(String),

    /// Numeric literal value
    Number(f64),

    /// Boolean literal value
    Boolean(bool),

    /// Null literal value
    Null,

    /// Variable reference (e.g., `["github", "sha"]` for github.sha)
    Variable(Vec<String>),

    /// Binary operation (comparison, logical)
    BinaryOp {
        /// Left operand
        left: Box<Expression>,
        /// Operator type
        op: BinaryOperator,
        /// Right operand
        right: Box<Expression>,
    },

    /// Unary operation (negation)
    UnaryOp {
        /// Operator type
        op: UnaryOperator,
        /// Expression to operate on
        expr: Box<Expression>,
    },

    /// Function call with arguments
    FunctionCall {
        /// Function name
        name: String,
        /// Function arguments
        args: Vec<Expression>,
    },

    /// Array/object indexing
    Index {
        /// Expression to index into
        expr: Box<Expression>,
        /// Index expression
        index: Box<Expression>,
    },
}

/// Binary operators supported in expressions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    /// Equality comparison (==)
    Equal,
    /// Inequality comparison (!=)
    NotEqual,
    /// Logical AND (&&)
    And,
    /// Logical OR (||)
    Or,
}

/// Unary operators supported in expressions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    /// Logical negation (!)
    Not,
}

impl Expression {
    /// Create a string literal expression
    #[must_use]
    pub fn string<S: Into<String>>(s: S) -> Self {
        Self::String(s.into())
    }

    /// Create a number literal expression
    #[must_use]
    pub const fn number(n: f64) -> Self {
        Self::Number(n)
    }

    /// Create a boolean literal expression
    #[must_use]
    pub const fn boolean(b: bool) -> Self {
        Self::Boolean(b)
    }

    /// Create a null literal expression
    #[must_use]
    pub const fn null() -> Self {
        Self::Null
    }

    /// Create a variable reference expression
    #[must_use]
    pub fn variable<I, S>(path: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Variable(path.into_iter().map(Into::into).collect())
    }

    /// Create a binary operation expression
    #[must_use]
    pub fn binary_op(left: Self, op: BinaryOperator, right: Self) -> Self {
        Self::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }

    /// Create a unary operation expression
    #[must_use]
    pub fn unary_op(op: UnaryOperator, expr: Self) -> Self {
        Self::UnaryOp {
            op,
            expr: Box::new(expr),
        }
    }

    /// Create a function call expression
    #[must_use]
    pub fn function_call<S, I>(name: S, args: I) -> Self
    where
        S: Into<String>,
        I: IntoIterator<Item = Self>,
    {
        Self::FunctionCall {
            name: name.into(),
            args: args.into_iter().collect(),
        }
    }

    /// Create an index expression
    #[must_use]
    pub fn index(expr: Self, index: Self) -> Self {
        Self::Index {
            expr: Box::new(expr),
            index: Box::new(index),
        }
    }
}

impl BinaryOperator {
    /// Get the string representation of this operator
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

impl UnaryOperator {
    /// Get the string representation of this operator
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Not => "!",
        }
    }
}
