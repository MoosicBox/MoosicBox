//! Expression AST types for workflow conditions and expressions.
//!
//! This module provides the abstract syntax tree for GitHub Actions compatible expressions,
//! including literals, variables, operators, function calls, and indexing. These expressions
//! are used in workflow conditions (`if` statements) and can evaluate variables and perform
//! operations at runtime.

use serde::{Deserialize, Serialize};

/// Expression AST for GitHub Actions compatible expressions.
///
/// Supports the complete expression language as defined in the specification:
/// * MVP functions: `toJson()`, `fromJson()`, `contains()`, `startsWith()`, `join()`, `format()`
/// * Operators: `==`, `!=`, `&&`, `||`, `!`, property access with `.`
/// * Complete Expression enum with all node types
///
/// # Examples
///
/// ```
/// use gpipe_ast::{Expression, BinaryOperator};
///
/// // Create a simple boolean expression: github.ref == 'refs/heads/main'
/// let expr = Expression::binary_op(
///     Expression::variable(["github", "ref"]),
///     BinaryOperator::Equal,
///     Expression::string("refs/heads/main"),
/// );
///
/// // Create a function call: contains(github.event.head_commit.message, '[skip ci]')
/// let contains_expr = Expression::function_call(
///     "contains",
///     vec![
///         Expression::variable(["github", "event", "head_commit", "message"]),
///         Expression::string("[skip ci]"),
///     ],
/// );
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_string_literal() {
        let expr = Expression::string("hello");
        assert_eq!(expr, Expression::String("hello".to_string()));
    }

    #[test]
    fn test_expression_number_literal() {
        let expr = Expression::number(42.5);
        assert_eq!(expr, Expression::Number(42.5));
    }

    #[test]
    fn test_expression_boolean_literal() {
        let expr_true = Expression::boolean(true);
        assert_eq!(expr_true, Expression::Boolean(true));

        let expr_false = Expression::boolean(false);
        assert_eq!(expr_false, Expression::Boolean(false));
    }

    #[test]
    fn test_expression_null_literal() {
        let expr = Expression::null();
        assert_eq!(expr, Expression::Null);
    }

    #[test]
    fn test_expression_variable_single_path() {
        let expr = Expression::variable(["github"]);
        assert_eq!(expr, Expression::Variable(vec!["github".to_string()]));
    }

    #[test]
    fn test_expression_variable_nested_path() {
        let expr = Expression::variable(["github", "event", "head_commit", "message"]);
        assert_eq!(
            expr,
            Expression::Variable(vec![
                "github".to_string(),
                "event".to_string(),
                "head_commit".to_string(),
                "message".to_string()
            ])
        );
    }

    #[test]
    fn test_expression_binary_op_equal() {
        let left = Expression::variable(["github", "ref"]);
        let right = Expression::string("refs/heads/main");
        let expr = Expression::binary_op(left.clone(), BinaryOperator::Equal, right.clone());

        if let Expression::BinaryOp {
            left: l,
            op,
            right: r,
        } = expr
        {
            assert_eq!(*l, left);
            assert_eq!(op, BinaryOperator::Equal);
            assert_eq!(*r, right);
        } else {
            panic!("Expected BinaryOp expression");
        }
    }

    #[test]
    fn test_expression_binary_op_not_equal() {
        let expr = Expression::binary_op(
            Expression::number(1.0),
            BinaryOperator::NotEqual,
            Expression::number(2.0),
        );

        if let Expression::BinaryOp { op, .. } = expr {
            assert_eq!(op, BinaryOperator::NotEqual);
        } else {
            panic!("Expected BinaryOp expression");
        }
    }

    #[test]
    fn test_expression_binary_op_and() {
        let expr = Expression::binary_op(
            Expression::boolean(true),
            BinaryOperator::And,
            Expression::boolean(false),
        );

        if let Expression::BinaryOp { op, .. } = expr {
            assert_eq!(op, BinaryOperator::And);
        } else {
            panic!("Expected BinaryOp expression");
        }
    }

    #[test]
    fn test_expression_binary_op_or() {
        let expr = Expression::binary_op(
            Expression::boolean(true),
            BinaryOperator::Or,
            Expression::boolean(false),
        );

        if let Expression::BinaryOp { op, .. } = expr {
            assert_eq!(op, BinaryOperator::Or);
        } else {
            panic!("Expected BinaryOp expression");
        }
    }

    #[test]
    fn test_expression_unary_op_not() {
        let inner = Expression::boolean(true);
        let expr = Expression::unary_op(UnaryOperator::Not, inner.clone());

        if let Expression::UnaryOp { op, expr: e } = expr {
            assert_eq!(op, UnaryOperator::Not);
            assert_eq!(*e, inner);
        } else {
            panic!("Expected UnaryOp expression");
        }
    }

    #[test]
    fn test_expression_function_call_no_args() {
        let expr = Expression::function_call("toJson", vec![]);

        if let Expression::FunctionCall { name, args } = expr {
            assert_eq!(name, "toJson");
            assert_eq!(args.len(), 0);
        } else {
            panic!("Expected FunctionCall expression");
        }
    }

    #[test]
    fn test_expression_function_call_with_args() {
        let expr = Expression::function_call(
            "contains",
            vec![
                Expression::variable(["github", "event", "head_commit", "message"]),
                Expression::string("[skip ci]"),
            ],
        );

        if let Expression::FunctionCall { name, args } = expr {
            assert_eq!(name, "contains");
            assert_eq!(args.len(), 2);
            assert_eq!(
                args[0],
                Expression::Variable(vec![
                    "github".to_string(),
                    "event".to_string(),
                    "head_commit".to_string(),
                    "message".to_string()
                ])
            );
            assert_eq!(args[1], Expression::String("[skip ci]".to_string()));
        } else {
            panic!("Expected FunctionCall expression");
        }
    }

    #[test]
    fn test_expression_index() {
        let array = Expression::variable(["matrix", "os"]);
        let index = Expression::number(0.0);
        let expr = Expression::index(array.clone(), index.clone());

        if let Expression::Index { expr: e, index: i } = expr {
            assert_eq!(*e, array);
            assert_eq!(*i, index);
        } else {
            panic!("Expected Index expression");
        }
    }

    #[test]
    fn test_expression_nested_binary_ops() {
        // Build: (a == b) && (c != d)
        let left = Expression::binary_op(
            Expression::variable(["a"]),
            BinaryOperator::Equal,
            Expression::variable(["b"]),
        );
        let right = Expression::binary_op(
            Expression::variable(["c"]),
            BinaryOperator::NotEqual,
            Expression::variable(["d"]),
        );
        let expr = Expression::binary_op(left, BinaryOperator::And, right);

        if let Expression::BinaryOp {
            left: l,
            op,
            right: r,
        } = expr
        {
            assert_eq!(op, BinaryOperator::And);
            assert!(matches!(*l, Expression::BinaryOp { .. }));
            assert!(matches!(*r, Expression::BinaryOp { .. }));
        } else {
            panic!("Expected BinaryOp expression");
        }
    }

    #[test]
    fn test_binary_operator_as_str() {
        assert_eq!(BinaryOperator::Equal.as_str(), "==");
        assert_eq!(BinaryOperator::NotEqual.as_str(), "!=");
        assert_eq!(BinaryOperator::And.as_str(), "&&");
        assert_eq!(BinaryOperator::Or.as_str(), "||");
    }

    #[test]
    fn test_unary_operator_as_str() {
        assert_eq!(UnaryOperator::Not.as_str(), "!");
    }

    #[test]
    fn test_expression_serde_string() {
        let expr = Expression::string("test");
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_number() {
        let expr = Expression::number(42.5);
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_boolean() {
        let expr = Expression::boolean(true);
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_null() {
        let expr = Expression::null();
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_variable() {
        let expr = Expression::variable(["github", "ref"]);
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_binary_op() {
        let expr = Expression::binary_op(
            Expression::variable(["a"]),
            BinaryOperator::Equal,
            Expression::string("b"),
        );
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_unary_op() {
        let expr = Expression::unary_op(UnaryOperator::Not, Expression::boolean(true));
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_function_call() {
        let expr = Expression::function_call(
            "contains",
            vec![
                Expression::variable(["message"]),
                Expression::string("text"),
            ],
        );
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_expression_serde_index() {
        let expr = Expression::index(Expression::variable(["array"]), Expression::number(0.0));
        let json = serde_json::to_string(&expr).unwrap();
        let deserialized: Expression = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, deserialized);
    }

    #[test]
    fn test_complex_expression_building() {
        // Build: !contains(github.event.head_commit.message, '[skip ci]') && github.ref == 'refs/heads/main'
        let contains_call = Expression::function_call(
            "contains",
            vec![
                Expression::variable(["github", "event", "head_commit", "message"]),
                Expression::string("[skip ci]"),
            ],
        );

        let not_contains = Expression::unary_op(UnaryOperator::Not, contains_call);

        let ref_check = Expression::binary_op(
            Expression::variable(["github", "ref"]),
            BinaryOperator::Equal,
            Expression::string("refs/heads/main"),
        );

        let complex = Expression::binary_op(not_contains, BinaryOperator::And, ref_check);

        // Verify structure
        if let Expression::BinaryOp { left, op, right: _ } = complex {
            assert_eq!(op, BinaryOperator::And);
            assert!(matches!(*left, Expression::UnaryOp { .. }));
        } else {
            panic!("Expected complex BinaryOp expression");
        }
    }
}
