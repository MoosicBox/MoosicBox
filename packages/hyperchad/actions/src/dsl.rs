//! DSL AST for `HyperChad` Actions
//!
//! This module provides a flexible AST that can represent Rust-like syntax
//! for defining actions in `HyperChad` templates.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{ActionEffect, ActionType, ElementTarget};
use hyperchad_transformer_models::Visibility;

/// Top-level DSL statement
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Statement {
    /// Expression statement (e.g., `hide("test");`)
    Expression(Expression),
    /// Variable assignment (e.g., `let x = get_visibility("test");`)
    Let { name: String, value: Expression },
    /// If statement with optional else
    If {
        condition: Expression,
        then_block: Block,
        else_block: Option<Block>,
    },
    /// Match statement
    Match {
        expr: Expression,
        arms: Vec<MatchArm>,
    },
    /// For loop (for iteration over collections)
    For {
        pattern: String,
        iter: Expression,
        body: Block,
    },
    /// While loop
    While { condition: Expression, body: Block },
    /// Block statement
    Block(Block),
}

/// A block of statements
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Block {
    pub statements: Vec<Statement>,
}

/// Match arm
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expression,
}

/// Pattern for match arms
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Pattern {
    /// Literal pattern (e.g., `true`, `"hello"`, `42`)
    Literal(Literal),
    /// Variable pattern (e.g., `x`)
    Variable(String),
    /// Wildcard pattern (`_`)
    Wildcard,
    /// Enum variant pattern (e.g., `Visibility::Hidden`)
    Variant {
        enum_name: String,
        variant: String,
        fields: Vec<Pattern>,
    },
}

/// DSL Expression
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Expression {
    /// Literal value
    Literal(Literal),
    /// Variable reference
    Variable(String),
    /// Element reference for object-oriented API
    ElementRef(ElementReference),
    /// Function call
    Call {
        function: String,
        args: Vec<Expression>,
    },
    /// Method call (e.g., `expr.method(args)`)
    MethodCall {
        receiver: Box<Expression>,
        method: String,
        args: Vec<Expression>,
    },
    /// Field access (e.g., `obj.field`)
    Field {
        object: Box<Expression>,
        field: String,
    },
    /// Binary operation
    Binary {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
    },
    /// Unary operation
    Unary { op: UnaryOp, expr: Box<Expression> },
    /// Conditional expression (e.g., `if condition { a } else { b }`)
    If {
        condition: Box<Expression>,
        then_branch: Box<Expression>,
        else_branch: Option<Box<Expression>>,
    },
    /// Match expression
    Match {
        expr: Box<Expression>,
        arms: Vec<MatchArm>,
    },
    /// Block expression
    Block(Block),
    /// Array/collection literal
    Array(Vec<Expression>),
    /// Tuple literal
    Tuple(Vec<Expression>),
    /// Range expression (e.g., `1..10`)
    Range {
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
        inclusive: bool,
    },
    /// Closure expression (e.g., `|param| { ... }`)
    Closure {
        params: Vec<String>,
        body: Box<Expression>,
    },
    /// Raw Rust code that couldn't be parsed by the DSL
    /// This is a fallback for complex expressions
    RawRust(String),
}

/// Binary operators
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,

    // Comparison
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,

    // Logical
    And,
    Or,

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,

    // Miscellaneous
    Min,
    Max,
}

/// Unary operators
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum UnaryOp {
    Not,
    Minus,
    Plus,
    Ref,
}

/// Literal values
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Literal {
    /// String literal
    String(String),
    /// Integer literal
    Integer(i64),
    /// Float literal
    Float(f64),
    /// Boolean literal
    Bool(bool),
    /// Unit literal (equivalent to `()`)
    Unit,
}

/// DSL AST root
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Dsl {
    pub statements: Vec<Statement>,
}

/// Element reference type for object-oriented element manipulation
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ElementReference {
    /// The element selector (e.g., "#my-id", ".my-class")
    pub selector: String,
}

impl ElementReference {
    /// Parse the selector and determine the type at compile time
    #[must_use]
    pub fn parse_selector(&self) -> ParsedSelector {
        if self.selector.starts_with('#') {
            ParsedSelector::Id(self.selector[1..].to_string())
        } else if self.selector.starts_with('.') {
            ParsedSelector::Class(self.selector[1..].to_string())
        } else if self.selector.is_empty() {
            ParsedSelector::Invalid
        } else {
            // If it doesn't start with # or ., treat it as a string ID for backward compatibility
            ParsedSelector::Id(self.selector.clone())
        }
    }
}

/// Parsed selector type to determine the correct function to call
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ParsedSelector {
    /// ID selector (#my-id -> my-id)
    Id(String),
    /// Class selector (.my-class -> my-class)
    Class(String),
    /// Complex selector (for future implementation)
    Complex(String),
    /// Invalid selector
    Invalid,
}

impl Dsl {
    /// Create a new DSL AST
    #[must_use]
    pub const fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }

    /// Evaluate the DSL and return the resulting actions
    /// This is a placeholder for now - actual evaluation logic will be implemented
    #[must_use]
    pub const fn evaluate(&self) -> Vec<ActionEffect> {
        // TODO: Implement DSL evaluation
        // For now, return empty vector
        Vec::new()
    }
}

/// Built-in functions available in the DSL
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuiltinFunction {
    // Element targeting
    Hide,
    Show,
    Toggle,
    SetVisibility,
    SetDisplay,
    SetBackground,

    // Element references
    Element,

    // Getters
    GetVisibility,
    GetDisplay,
    GetWidth,
    GetHeight,
    GetPositionX,
    GetPositionY,
    GetMouseX,
    GetMouseY,
    GetDataAttr,
    GetEventValue,

    // Utilities
    NoOp,
    Log,
    Navigate,
    Custom,

    // Control flow helpers
    If,
    Match,

    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Clamp,

    // Logic
    Eq,
    And,
    Or,
    Not,
}

/// Element target types for DSL
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DslElementTarget {
    /// String ID
    Id(String),
    /// CSS Class
    Class(String),
    /// Child class
    ChildClass(String),
    /// Numeric ID
    NumericId(usize),
    /// Self reference
    SelfTarget,
    /// Last child
    LastChild,
}

impl From<DslElementTarget> for ElementTarget {
    fn from(target: DslElementTarget) -> Self {
        match target {
            DslElementTarget::Id(id) => Self::StrId(id),
            DslElementTarget::Class(class) => Self::Class(class),
            DslElementTarget::ChildClass(class) => Self::ChildClass(class),
            DslElementTarget::NumericId(id) => Self::Id(id),
            DslElementTarget::SelfTarget => Self::SelfTarget,
            DslElementTarget::LastChild => Self::LastChild,
        }
    }
}

/// Evaluation context for DSL expressions
#[derive(Clone, Debug, Default)]
pub struct EvalContext {
    /// Variables in scope
    pub variables: std::collections::HashMap<String, DslValue>,
}

/// DSL values that can be stored in variables or used in expressions
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DslValue {
    /// String value
    String(String),
    /// Numeric value
    Number(f64),
    /// Boolean value
    Bool(bool),
    /// Visibility value
    Visibility(Visibility),
    /// Element target
    Target(DslElementTarget),
    /// Element reference for object-oriented API
    ElementRef(ElementReference),
    /// Action effect
    Action(ActionEffect),
    /// List of values
    List(Vec<DslValue>),
    /// Unit value
    Unit,
}

impl From<Literal> for DslValue {
    fn from(lit: Literal) -> Self {
        #[allow(clippy::cast_precision_loss)]
        match lit {
            Literal::String(s) => Self::String(s),
            Literal::Integer(i) => Self::Number(i as f64),
            Literal::Float(f) => Self::Number(f),
            Literal::Bool(b) => Self::Bool(b),
            Literal::Unit => Self::Unit,
        }
    }
}

impl From<DslValue> for ActionEffect {
    fn from(value: DslValue) -> Self {
        match value {
            DslValue::Action(action) => action,
            DslValue::ElementRef(element_ref) => {
                // For element references, create a custom action that stores the element reference
                Self {
                    action: ActionType::Custom {
                        action: format!("element_ref:{}", element_ref.selector),
                    },
                    delay_off: None,
                    throttle: None,
                    unique: None,
                }
            }
            _ => Self {
                action: ActionType::NoOp,
                delay_off: None,
                throttle: None,
                unique: None,
            },
        }
    }
}
