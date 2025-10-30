//! Expression parser for filter expressions.
//!
//! This module implements a recursive descent parser for filter expressions,
//! supporting logical operators (AND, OR, NOT) and grouping with parentheses.
//!
//! Operator precedence (highest to lowest):
//! 1. NOT
//! 2. AND
//! 3. OR

use super::parser::parse_filter;
use super::tokenizer::tokenize;
use super::types::{FilterError, FilterExpression, Token};

/// Parse a filter expression string into a `FilterExpression` AST.
///
/// # Examples
///
/// * `"package.publish=false"` → Single condition
/// * `"package.publish=false AND package.version^=0.1"` → AND expression
/// * `"(package.publish=false OR package.name$=_example) AND package.categories@=audio"` → Complex nested expression
/// * `"NOT package.version^=0.1"` → NOT expression
///
/// # Errors
///
/// * Returns error if syntax is invalid
/// * Returns error if parentheses are unmatched
/// * Returns error if filter conditions are invalid
pub fn parse_expression(input: &str) -> Result<FilterExpression, FilterError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    parser.parse_or_expression()
}

/// Internal parser state.
struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    const fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    /// Get the current token without consuming it.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    /// Consume and return the current token.
    fn next(&mut self) -> Option<Token> {
        if self.position < self.tokens.len() {
            let token = self.tokens[self.position].clone();
            self.position += 1;
            Some(token)
        } else {
            None
        }
    }

    /// Parse an OR expression (lowest precedence).
    ///
    /// Grammar: `and_expr (OR and_expr)*`
    fn parse_or_expression(&mut self) -> Result<FilterExpression, FilterError> {
        let mut left = self.parse_and_expression()?;

        while matches!(self.peek(), Some(Token::Or)) {
            self.next(); // consume OR
            let right = self.parse_and_expression()?;

            // Flatten OR expressions
            left = match left {
                FilterExpression::Or(mut children) => {
                    children.push(right);
                    FilterExpression::Or(children)
                }
                _ => FilterExpression::Or(vec![left, right]),
            };
        }

        Ok(left)
    }

    /// Parse an AND expression (medium precedence).
    ///
    /// Grammar: `not_expr (AND not_expr)*`
    fn parse_and_expression(&mut self) -> Result<FilterExpression, FilterError> {
        let mut left = self.parse_not_expression()?;

        while matches!(self.peek(), Some(Token::And)) {
            self.next(); // consume AND
            let right = self.parse_not_expression()?;

            // Flatten AND expressions
            left = match left {
                FilterExpression::And(mut children) => {
                    children.push(right);
                    FilterExpression::And(children)
                }
                _ => FilterExpression::And(vec![left, right]),
            };
        }

        Ok(left)
    }

    /// Parse a NOT expression (highest precedence).
    ///
    /// Grammar: `NOT not_expr | primary`
    fn parse_not_expression(&mut self) -> Result<FilterExpression, FilterError> {
        if matches!(self.peek(), Some(Token::Not)) {
            self.next(); // consume NOT
            let expr = self.parse_not_expression()?;
            Ok(FilterExpression::Not(Box::new(expr)))
        } else {
            self.parse_primary()
        }
    }

    /// Parse a primary expression (parentheses or filter condition).
    ///
    /// Grammar: `( or_expr ) | filter`
    fn parse_primary(&mut self) -> Result<FilterExpression, FilterError> {
        match self.next() {
            Some(Token::LeftParen) => {
                let expr = self.parse_or_expression()?;
                match self.next() {
                    Some(Token::RightParen) => Ok(expr),
                    Some(other) => Err(FilterError::UnexpectedToken(format!(
                        "Expected ')' but found {other:?}"
                    ))),
                    None => Err(FilterError::ExpectedToken("')' to close group".to_string())),
                }
            }
            Some(Token::Filter(filter_str)) => {
                let filter = parse_filter(&filter_str)?;
                Ok(FilterExpression::Condition(filter))
            }
            Some(other) => Err(FilterError::UnexpectedToken(format!(
                "Expected filter condition or '(' but found {other:?}"
            ))),
            None => Err(FilterError::ExpectedToken(
                "filter condition or '('".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_filter() {
        let expr = parse_expression("publish=false").unwrap();
        match expr {
            FilterExpression::Condition(filter) => {
                assert_eq!(filter.property_path, vec!["publish"]);
                assert_eq!(filter.value, "false");
            }
            _ => panic!("Expected Condition"),
        }
    }

    #[test]
    fn test_parse_simple_filter_with_nested_path() {
        let expr = parse_expression("package.publish=false").unwrap();
        match expr {
            FilterExpression::Condition(filter) => {
                assert_eq!(filter.property_path, vec!["package", "publish"]);
                assert_eq!(filter.value, "false");
            }
            _ => panic!("Expected Condition"),
        }
    }

    #[test]
    fn test_parse_and_expression() {
        let expr = parse_expression("package.publish=false AND package.version^=0.1").unwrap();
        match expr {
            FilterExpression::And(children) => {
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected And"),
        }
    }

    #[test]
    fn test_parse_or_expression() {
        let expr = parse_expression("package.publish=false OR package.version^=0.1").unwrap();
        match expr {
            FilterExpression::Or(children) => {
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_parse_not_expression() {
        let expr = parse_expression("NOT package.publish=false").unwrap();
        match expr {
            FilterExpression::Not(inner) => match *inner {
                FilterExpression::Condition(_) => {}
                _ => panic!("Expected Condition inside Not"),
            },
            _ => panic!("Expected Not"),
        }
    }

    #[test]
    fn test_parse_parentheses() {
        let expr = parse_expression("(package.publish=false OR package.version^=0.1)").unwrap();
        match expr {
            FilterExpression::Or(children) => {
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_parse_complex_expression() {
        let expr = parse_expression(
            "(package.publish=false OR package.name$=_example) AND package.categories@=audio",
        )
        .unwrap();
        match expr {
            FilterExpression::And(children) => {
                assert_eq!(children.len(), 2);
                match &children[0] {
                    FilterExpression::Or(or_children) => {
                        assert_eq!(or_children.len(), 2);
                    }
                    _ => panic!("Expected Or as first child of And"),
                }
            }
            _ => panic!("Expected And"),
        }
    }

    #[test]
    fn test_parse_precedence() {
        // NOT > AND > OR
        let expr = parse_expression(
            "NOT package.publish=false AND package.version^=0.1 OR package.name=test",
        )
        .unwrap();
        match expr {
            FilterExpression::Or(children) => {
                assert_eq!(children.len(), 2);
                // First child should be AND
                match &children[0] {
                    FilterExpression::And(and_children) => {
                        assert_eq!(and_children.len(), 2);
                        // First child of AND should be NOT
                        assert!(matches!(&and_children[0], FilterExpression::Not(_)));
                    }
                    _ => panic!("Expected And as first child of Or"),
                }
            }
            _ => panic!("Expected Or"),
        }
    }

    #[test]
    fn test_parse_quoted_value() {
        let expr = parse_expression(r#"name="test package""#).unwrap();
        match expr {
            FilterExpression::Condition(filter) => {
                assert_eq!(filter.value, "test package");
            }
            _ => panic!("Expected Condition"),
        }
    }

    #[test]
    fn test_parse_quoted_with_keyword() {
        let expr = parse_expression(r#"description="This AND that""#).unwrap();
        match expr {
            FilterExpression::Condition(filter) => {
                assert_eq!(filter.value, "This AND that");
            }
            _ => panic!("Expected Condition"),
        }
    }

    #[test]
    fn test_parse_unmatched_paren() {
        let result = parse_expression("(package.publish=false");
        assert!(matches!(result, Err(FilterError::ExpectedToken(_))));
    }

    #[test]
    fn test_parse_unexpected_token() {
        let result = parse_expression(")");
        assert!(matches!(result, Err(FilterError::UnexpectedToken(_))));
    }
}
