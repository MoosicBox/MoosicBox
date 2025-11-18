//! Type definitions for package filtering.

use thiserror::Error;

/// Errors that can occur during package filtering.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FilterError {
    /// Filter syntax is invalid
    #[error("Invalid filter syntax: {0}")]
    InvalidSyntax(String),

    /// Operator not recognized
    #[error("Unknown operator: {0}")]
    UnknownOperator(String),

    /// Property not found in package
    #[error("Property not found: {0}")]
    PropertyNotFound(String),

    /// Type mismatch (e.g., using array operator on string property)
    #[error("Type mismatch: {0}")]
    TypeMismatch(String),

    /// Value cannot be parsed
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    /// Regex pattern is invalid
    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),

    /// IO error reading Cargo.toml
    #[error("IO error: {0}")]
    IoError(String),

    /// TOML parsing error
    #[error("TOML parse error: {0}")]
    TomlError(String),

    /// Unclosed quote in filter expression
    #[error("Unclosed quote in filter expression: {0}")]
    UnclosedQuote(String),

    /// Unexpected token in expression
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),

    /// Expected token not found
    #[error("Expected {0}")]
    ExpectedToken(String),
}

/// A parsed package filter with support for nested properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFilter {
    /// The property path (e.g., `["publish"]`, `["metadata", "workspaces", "independent"]`)
    pub property_path: Vec<String>,
    /// The comparison operator
    pub operator: FilterOperator,
    /// The value to compare against
    pub value: String,
}

impl PackageFilter {
    /// Get the property name for display purposes.
    ///
    /// Returns the full property path joined with dots (e.g., "package.metadata.workspaces.independent").
    #[must_use]
    pub fn property_display(&self) -> String {
        self.property_path.join(".")
    }
}

/// Comparison operators for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    // Scalar operators
    /// Exact equality (=)
    Equals,
    /// Not equal (!=)
    NotEquals,
    /// Starts with (^=)
    StartsWith,
    /// Ends with ($=)
    EndsWith,
    /// Contains (*=)
    Contains,
    /// Regex match (~=)
    RegexMatch,

    // Array operators
    /// Array contains element (@=)
    ArrayContains,
    /// Array contains element with substring (@*=)
    ArrayContainsSubstring,
    /// Array contains element starting with (@^=)
    ArrayContainsStartsWith,
    /// Array contains element matching regex (@~=)
    ArrayContainsRegex,
    /// Array is empty (@!)
    ArrayEmpty,
    /// Array length equals (@#=)
    ArrayLengthEquals,
    /// Array length greater than (@#>)
    ArrayLengthGreater,
    /// Array length less than (@#<)
    ArrayLengthLess,
    /// Array does NOT contain element (!@=)
    ArrayNotContains,

    // Existence operators
    /// Property exists (?)
    Exists,
    /// Property does NOT exist (!?)
    NotExists,
}

impl FilterOperator {
    /// Get the string representation of this operator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Equals => "=",
            Self::NotEquals => "!=",
            Self::StartsWith => "^=",
            Self::EndsWith => "$=",
            Self::Contains => "*=",
            Self::RegexMatch => "~=",
            Self::ArrayContains => "@=",
            Self::ArrayContainsSubstring => "@*=",
            Self::ArrayContainsStartsWith => "@^=",
            Self::ArrayContainsRegex => "@~=",
            Self::ArrayEmpty => "@!",
            Self::ArrayLengthEquals => "@#=",
            Self::ArrayLengthGreater => "@#>",
            Self::ArrayLengthLess => "@#<",
            Self::ArrayNotContains => "!@=",
            Self::Exists => "?",
            Self::NotExists => "!?",
        }
    }

    /// Check if this is an array operator.
    #[must_use]
    pub const fn is_array_operator(self) -> bool {
        matches!(
            self,
            Self::ArrayContains
                | Self::ArrayContainsSubstring
                | Self::ArrayContainsStartsWith
                | Self::ArrayContainsRegex
                | Self::ArrayEmpty
                | Self::ArrayLengthEquals
                | Self::ArrayLengthGreater
                | Self::ArrayLengthLess
                | Self::ArrayNotContains
        )
    }

    /// Check if this operator requires no value.
    #[must_use]
    pub const fn is_value_optional(self) -> bool {
        matches!(self, Self::ArrayEmpty | Self::Exists | Self::NotExists)
    }
}

/// A token in a filter expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A complete filter condition string (e.g., "package.publish=false", "package.name=\"my package\"")
    Filter(String),
    /// Logical AND operator
    And,
    /// Logical OR operator
    Or,
    /// Logical NOT operator
    Not,
    /// Left parenthesis
    LeftParen,
    /// Right parenthesis
    RightParen,
}

/// A filter expression supporting logical operators and grouping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterExpression {
    /// A single filter condition
    Condition(PackageFilter),
    /// Logical AND - all children must be true
    And(Vec<FilterExpression>),
    /// Logical OR - at least one child must be true
    Or(Vec<FilterExpression>),
    /// Logical NOT - inverts child result
    Not(Box<FilterExpression>),
}
