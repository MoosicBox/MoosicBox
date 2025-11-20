//! DSL AST for `HyperChad` Actions
//!
//! This module provides a flexible AST that can represent Rust-like syntax
//! for defining actions in `HyperChad` templates.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{ActionEffect, ActionType, ElementTarget, Target};
use hyperchad_transformer_models::Visibility;

/// Top-level DSL statement
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Statement {
    /// Expression statement (e.g., `hide("test");`)
    Expression(Expression),
    /// Variable assignment (e.g., `let x = get_visibility("test");`)
    Let {
        /// Variable name
        name: String,
        /// Value expression
        value: Expression,
    },
    /// If statement with optional else
    If {
        /// Condition expression
        condition: Expression,
        /// Then branch block
        then_block: Block,
        /// Optional else branch block
        else_block: Option<Block>,
    },
    /// Match statement
    Match {
        /// Expression to match against
        expr: Expression,
        /// Match arms
        arms: Vec<MatchArm>,
    },
    /// For loop (for iteration over collections)
    For {
        /// Pattern to bind iteration variable
        pattern: String,
        /// Iterator expression
        iter: Expression,
        /// Loop body
        body: Block,
    },
    /// While loop
    While {
        /// Loop condition
        condition: Expression,
        /// Loop body
        body: Block,
    },
    /// Block statement
    Block(Block),
}

/// A block of statements
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Block {
    /// Statements in this block
    pub statements: Vec<Statement>,
}

/// Match arm
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MatchArm {
    /// Pattern to match against
    pub pattern: Pattern,
    /// Expression to evaluate when pattern matches
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
        /// Name of the enum type
        enum_name: String,
        /// Name of the variant
        variant: String,
        /// Fields of the variant
        fields: Vec<Self>,
    },
}

/// Variable representing an element reference for DSL expressions
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ElementVariable {
    /// Element variable name
    pub name: String,
}

impl ElementVariable {
    /// Shows the element
    #[must_use]
    pub fn show(self) -> ActionType {
        ActionType::show_str_id(Target::reference(self.name))
    }

    /// Hides the element
    #[must_use]
    pub fn hide(self) -> ActionType {
        ActionType::hide_str_id(Target::reference(self.name))
    }

    /// Focuses the element
    #[must_use]
    pub fn focus(self) -> ActionType {
        ActionType::focus_str_id(Target::reference(self.name))
    }

    /// Selects the element
    #[must_use]
    pub fn select(self) -> ActionType {
        ActionType::select_str_id(Target::reference(self.name))
    }

    /// Toggles visibility of the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_visibility(self) -> ActionType {
        ActionType::toggle_visibility_str_id(Target::reference(self.name))
    }

    /// Gets the visibility value of the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn visibility(self) -> crate::logic::CalcValue {
        crate::logic::get_visibility_str_id(Target::reference(self.name))
    }

    /// Enables display on the element
    #[must_use]
    pub fn display(self) -> ActionType {
        ActionType::display_str_id(Target::reference(self.name))
    }

    /// Disables display on the element
    #[must_use]
    pub fn no_display(self) -> ActionType {
        ActionType::no_display_str_id(Target::reference(self.name))
    }

    /// Sets display property on the element
    #[must_use]
    pub fn set_display(self, display: bool) -> ActionType {
        ActionType::set_display_str_id(display, Target::reference(self.name))
    }

    /// Toggles display property on the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display(self) -> ActionType {
        ActionType::toggle_display_str_id(Target::reference(self.name))
    }

    /// Gets the display value of the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn get_display(self) -> crate::logic::CalcValue {
        crate::logic::get_display_str_id(Target::reference(self.name))
    }

    /// Gets the width in pixels of the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn get_width_px(self) -> crate::logic::CalcValue {
        crate::logic::get_width_px_str_id(Target::reference(self.name))
    }

    /// Gets the height in pixels of the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn get_height_px(self) -> crate::logic::CalcValue {
        crate::logic::get_height_px_str_id(Target::reference(self.name))
    }

    /// Gets the mouse X coordinate relative to the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn get_mouse_x(self) -> crate::logic::CalcValue {
        crate::logic::get_mouse_x_str_id(Target::reference(self.name))
    }

    /// Gets the mouse Y coordinate relative to the element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn get_mouse_y(self) -> crate::logic::CalcValue {
        crate::logic::get_mouse_y_str_id(Target::reference(self.name))
    }

    /// Sets visibility on the element
    #[must_use]
    pub fn set_visibility(self, visibility: Visibility) -> ActionType {
        ActionType::set_visibility_str_id(visibility, Target::reference(self.name))
    }
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
    ElementRef(Box<Self>),
    /// Function call
    Call {
        /// Function name
        function: String,
        /// Function arguments
        args: Vec<Self>,
    },
    /// Method call (e.g., `expr.method(args)`)
    MethodCall {
        /// Receiver expression
        receiver: Box<Self>,
        /// Method name
        method: String,
        /// Method arguments
        args: Vec<Self>,
    },
    /// Field access (e.g., `obj.field`)
    Field {
        /// Object expression
        object: Box<Self>,
        /// Field name
        field: String,
    },
    /// Binary operation
    Binary {
        /// Left operand
        left: Box<Self>,
        /// Binary operator
        op: BinaryOp,
        /// Right operand
        right: Box<Self>,
    },
    /// Unary operation
    Unary {
        /// Unary operator
        op: UnaryOp,
        /// Operand expression
        expr: Box<Self>,
    },
    /// Conditional expression (e.g., `if condition { a } else { b }`)
    If {
        /// Condition expression
        condition: Box<Self>,
        /// Then branch expression
        then_branch: Box<Self>,
        /// Optional else branch expression
        else_branch: Option<Box<Self>>,
    },
    /// Match expression
    Match {
        /// Expression to match against
        expr: Box<Self>,
        /// Match arms
        arms: Vec<MatchArm>,
    },
    /// Block expression
    Block(Block),
    /// Array/collection literal
    Array(Vec<Self>),
    /// Tuple literal
    Tuple(Vec<Self>),
    /// Range expression (e.g., `1..10`)
    Range {
        /// Optional start expression
        start: Option<Box<Self>>,
        /// Optional end expression
        end: Option<Box<Self>>,
        /// Whether the range is inclusive
        inclusive: bool,
    },
    /// Closure expression (e.g., `|param| { ... }`)
    Closure {
        /// Parameter names
        params: Vec<String>,
        /// Closure body
        body: Box<Self>,
    },
    /// Parenthesized expression for explicit grouping
    Grouping(Box<Self>),
    /// Raw Rust code that couldn't be parsed by the DSL
    /// This is a fallback for complex expressions
    RawRust(String),
}

impl std::fmt::Display for Expression {
    /// Formats the expression (only supports literals and variables currently)
    ///
    /// # Panics
    ///
    /// * If the expression type is not yet implemented for display
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Literal(literal) => std::fmt::Display::fmt(literal, f),
            Self::Variable(variable) => std::fmt::Display::fmt(variable, f),
            Self::ElementRef(..) => unimplemented!("element_ref"),
            Self::Call { .. } => unimplemented!("call"),
            Self::MethodCall { .. } => unimplemented!("method_call"),
            Self::Field { .. } => unimplemented!("field"),
            Self::Binary { .. } => unimplemented!("binary"),
            Self::Unary { .. } => unimplemented!("unary"),
            Self::If { .. } => unimplemented!("if"),
            Self::Match { .. } => unimplemented!("match"),
            Self::Block(..) => unimplemented!("block"),
            Self::Array(..) => unimplemented!("array"),
            Self::Tuple(..) => unimplemented!("tuple"),
            Self::Range { .. } => unimplemented!("range"),
            Self::Closure { .. } => unimplemented!("closure"),
            Self::RawRust(_) => unimplemented!("raw_rust"),
            Self::Grouping(_) => unimplemented!("grouping"),
        }
    }
}

/// Binary operators
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BinaryOp {
    /// Addition operator (+)
    Add,
    /// Subtraction operator (-)
    Subtract,
    /// Multiplication operator (*)
    Multiply,
    /// Division operator (/)
    Divide,
    /// Modulo operator (%)
    Modulo,

    /// Equality operator (==)
    Equal,
    /// Inequality operator (!=)
    NotEqual,
    /// Less than operator (<)
    Less,
    /// Less than or equal operator (<=)
    LessEqual,
    /// Greater than operator (>)
    Greater,
    /// Greater than or equal operator (>=)
    GreaterEqual,

    /// Logical AND operator (&&)
    And,
    /// Logical OR operator (||)
    Or,

    /// Bitwise AND operator (&)
    BitAnd,
    /// Bitwise OR operator (|)
    BitOr,
    /// Bitwise XOR operator (^)
    BitXor,
}

/// Unary operators
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum UnaryOp {
    /// Logical NOT operator (!)
    Not,
    /// Unary minus operator (-)
    Minus,
    /// Unary plus operator (+)
    Plus,
    /// Reference operator (&)
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

impl Literal {
    /// Creates a string literal
    #[must_use]
    pub fn string(x: impl Into<String>) -> Self {
        Self::String(x.into())
    }

    /// Creates an integer literal
    #[must_use]
    pub fn integer(x: impl Into<i64>) -> Self {
        Self::Integer(x.into())
    }

    /// Creates a float literal
    #[must_use]
    pub fn float(x: impl Into<f64>) -> Self {
        Self::Float(x.into())
    }

    /// Creates a boolean literal
    #[must_use]
    pub fn bool(x: impl Into<bool>) -> Self {
        Self::Bool(x.into())
    }
}

impl std::fmt::Display for Literal {
    /// Formats the literal value as a string
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(x) => f.write_str(x),
            Self::Integer(x) => write!(f, "{x}"),
            Self::Float(x) => write!(f, "{x}"),
            Self::Bool(x) => write!(f, "{x}"),
            Self::Unit => write!(f, ""),
        }
    }
}

/// DSL AST root
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Dsl {
    /// Top-level statements in the DSL program
    pub statements: Vec<Statement>,
}

/// Element reference type for object-oriented element manipulation
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ElementReference {
    /// The element selector (e.g., "#my-id", ".my-class")
    /// FIXME: This should just be a `ParseSelector`
    pub selector: String,
}

impl ElementReference {
    /// Parses the selector and determines its type
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
    /// Creates a new DSL AST
    #[must_use]
    pub const fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }

    /// Evaluates the DSL and returns the resulting actions
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
    /// Hide an element
    Hide,
    /// Show an element
    Show,
    /// Toggle element visibility
    Toggle,
    /// Set element visibility
    SetVisibility,
    /// Set element display property
    SetDisplay,
    /// Set element background
    SetBackground,

    // Element references
    /// Get element reference
    Element,

    // Getters
    /// Get element visibility
    GetVisibility,
    /// Get element display property
    GetDisplay,
    /// Get element width
    GetWidth,
    /// Get element height
    GetHeight,
    /// Get element X position
    GetPositionX,
    /// Get element Y position
    GetPositionY,
    /// Get mouse X coordinate
    GetMouseX,
    /// Get mouse Y coordinate
    GetMouseY,
    /// Get element data attribute
    GetDataAttr,
    /// Get event value
    GetEventValue,

    // Utilities
    /// No operation
    NoOp,
    /// Log a message
    Log,
    /// Navigate to URL
    Navigate,
    /// Custom action
    Custom,

    // Control flow helpers
    /// If conditional
    If,
    /// Match expression
    Match,

    // Arithmetic
    /// Addition operation
    Add,
    /// Subtraction operation
    Subtract,
    /// Multiplication operation
    Multiply,
    /// Division operation
    Divide,
    /// Minimum of two values
    Min,
    /// Maximum of two values
    Max,
    /// Clamp value between min and max
    Clamp,

    // Logic
    /// Equality comparison
    Eq,
    /// Logical AND operation
    And,
    /// Logical OR operation
    Or,
    /// Logical NOT operation
    Not,
}

/// Evaluation context for DSL expressions
#[derive(Clone, Debug, Default)]
pub struct EvalContext {
    /// Variables in scope
    pub variables: std::collections::BTreeMap<String, DslValue>,
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
    Target(ElementTarget),
    /// Element reference for object-oriented API
    ElementRef(ElementReference),
    /// Action effect
    Action(ActionEffect),
    /// List of values
    List(Vec<Self>),
    /// Unit value
    Unit,
}

impl From<Literal> for DslValue {
    /// Converts a `Literal` into a `DslValue`
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
    /// Converts a `DslValue` into an `ActionEffect`
    ///
    /// For element references, creates a custom action. For non-action values,
    /// creates a no-op action.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_reference_parse_id_with_hash() {
        let reference = ElementReference {
            selector: "#my-element".to_string(),
        };
        let parsed = reference.parse_selector();

        match parsed {
            ParsedSelector::Id(id) => assert_eq!(id, "my-element"),
            _ => panic!("Expected Id selector"),
        }
    }

    #[test]
    fn test_element_reference_parse_class_with_dot() {
        let reference = ElementReference {
            selector: ".my-class".to_string(),
        };
        let parsed = reference.parse_selector();

        match parsed {
            ParsedSelector::Class(class) => assert_eq!(class, "my-class"),
            _ => panic!("Expected Class selector"),
        }
    }

    #[test]
    fn test_element_reference_parse_bare_string_as_id() {
        let reference = ElementReference {
            selector: "my-element".to_string(),
        };
        let parsed = reference.parse_selector();

        match parsed {
            ParsedSelector::Id(id) => assert_eq!(id, "my-element"),
            _ => panic!("Expected Id selector for backward compatibility"),
        }
    }

    #[test]
    fn test_element_reference_parse_empty_as_invalid() {
        let reference = ElementReference {
            selector: String::new(),
        };
        let parsed = reference.parse_selector();

        match parsed {
            ParsedSelector::Invalid => {}
            _ => panic!("Expected Invalid selector for empty string"),
        }
    }

    #[test]
    fn test_literal_constructors() {
        let string = Literal::string("test");
        assert_eq!(string, Literal::String("test".to_string()));

        let integer = Literal::integer(42);
        assert_eq!(integer, Literal::Integer(42));

        let float = Literal::float(2.5);
        assert_eq!(float, Literal::Float(2.5));

        let boolean = Literal::bool(true);
        assert_eq!(boolean, Literal::Bool(true));
    }

    #[test]
    fn test_literal_display_string() {
        let lit = Literal::String("hello".to_string());
        assert_eq!(format!("{lit}"), "hello");
    }

    #[test]
    fn test_literal_display_integer() {
        let lit = Literal::Integer(42);
        assert_eq!(format!("{lit}"), "42");
    }

    #[test]
    fn test_literal_display_float() {
        let lit = Literal::Float(2.5);
        assert_eq!(format!("{lit}"), "2.5");
    }

    #[test]
    fn test_literal_display_bool() {
        let lit = Literal::Bool(true);
        assert_eq!(format!("{lit}"), "true");
    }

    #[test]
    fn test_literal_display_unit() {
        let lit = Literal::Unit;
        assert_eq!(format!("{lit}"), "");
    }

    #[test]
    fn test_expression_display_literal() {
        let expr = Expression::Literal(Literal::String("test".to_string()));
        assert_eq!(format!("{expr}"), "test");
    }

    #[test]
    fn test_expression_display_variable() {
        let expr = Expression::Variable("my_var".to_string());
        assert_eq!(format!("{expr}"), "my_var");
    }

    #[test]
    fn test_dsl_value_from_literal_string() {
        let lit = Literal::String("test".to_string());
        let value: DslValue = lit.into();
        match value {
            DslValue::String(s) => assert_eq!(s, "test"),
            _ => panic!("Expected String value"),
        }
    }

    #[test]
    fn test_dsl_value_from_literal_integer() {
        let lit = Literal::Integer(42);
        let value: DslValue = lit.into();
        match value {
            DslValue::Number(n) => assert!((n - 42.0).abs() < f64::EPSILON),
            _ => panic!("Expected Number value"),
        }
    }

    #[test]
    fn test_dsl_value_from_literal_float() {
        let lit = Literal::Float(2.5);
        let value: DslValue = lit.into();
        match value {
            DslValue::Number(n) => assert!((n - 2.5).abs() < f64::EPSILON),
            _ => panic!("Expected Number value"),
        }
    }

    #[test]
    fn test_dsl_value_from_literal_bool() {
        let lit = Literal::Bool(true);
        let value: DslValue = lit.into();
        match value {
            DslValue::Bool(b) => assert!(b),
            _ => panic!("Expected Bool value"),
        }
    }

    #[test]
    fn test_dsl_value_from_literal_unit() {
        let lit = Literal::Unit;
        let value: DslValue = lit.into();
        match value {
            DslValue::Unit => {}
            _ => panic!("Expected Unit value"),
        }
    }

    #[test]
    fn test_dsl_value_to_action_effect_for_action() {
        let action_effect = crate::ActionEffect {
            action: crate::ActionType::NoOp,
            delay_off: None,
            throttle: None,
            unique: None,
        };
        let dsl_value = DslValue::Action(action_effect);
        let converted: crate::ActionEffect = dsl_value.into();

        assert!(matches!(converted.action, crate::ActionType::NoOp));
    }

    #[test]
    fn test_dsl_value_to_action_effect_for_element_ref() {
        let element_ref = ElementReference {
            selector: "#test".to_string(),
        };
        let dsl_value = DslValue::ElementRef(element_ref);
        let converted: crate::ActionEffect = dsl_value.into();

        match converted.action {
            crate::ActionType::Custom { action } => {
                assert_eq!(action, "element_ref:#test");
            }
            _ => panic!("Expected Custom action for element reference"),
        }
    }

    #[test]
    fn test_dsl_value_to_action_effect_for_non_action() {
        let dsl_value = DslValue::String("test".to_string());
        let converted: crate::ActionEffect = dsl_value.into();

        assert!(matches!(converted.action, crate::ActionType::NoOp));
    }
}
