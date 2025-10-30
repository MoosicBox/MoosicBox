//! Tokenizer for filter expressions.
//!
//! This module tokenizes filter expression strings into tokens that can be parsed
//! into a filter expression AST. It handles:
//! * Quoted strings with escape sequences
//! * Logical operators (AND, OR, NOT)
//! * Grouping with parentheses
//! * Filter conditions
//! * Full Unicode support (multibyte characters, emoji, RTL text)

use super::parser::parse_filter;
use super::types::{FilterError, Token};
use std::iter::Peekable;
use std::str::CharIndices;

/// Check if string starts with "AND" (case-insensitive), consuming exactly 3 chars.
/// Returns true only if it's exactly 3 chars matching A-N-D.
///
/// This uses character-aware iteration to avoid byte boundary panics with multibyte UTF-8.
fn starts_with_and(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(a), Some(n), Some(d))
        if a.eq_ignore_ascii_case(&'A')
           && n.eq_ignore_ascii_case(&'N')
           && d.eq_ignore_ascii_case(&'D')
    )
}

/// Check if string starts with "OR" (case-insensitive), consuming exactly 2 chars.
///
/// This uses character-aware iteration to avoid byte boundary panics with multibyte UTF-8.
fn starts_with_or(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(o), Some(r))
        if o.eq_ignore_ascii_case(&'O')
           && r.eq_ignore_ascii_case(&'R')
    )
}

/// Check if string starts with "NOT" (case-insensitive), consuming exactly 3 chars.
///
/// This uses character-aware iteration to avoid byte boundary panics with multibyte UTF-8.
fn starts_with_not(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(n), Some(o), Some(t))
        if n.eq_ignore_ascii_case(&'N')
           && o.eq_ignore_ascii_case(&'O')
           && t.eq_ignore_ascii_case(&'T')
    )
}

/// Tokenize a filter expression string.
///
/// # Examples
///
/// * `"publish=false"` → `[Filter("publish=false")]`
/// * `"publish=false AND version^=0.1"` → `[Filter("publish=false"), And, Filter("version^=0.1")]`
/// * `"(name=\"test pkg\" OR version^=\"0.1\")"` → `[LeftParen, Filter(...), Or, Filter(...), RightParen]`
///
/// # Errors
///
/// * Returns error if quotes are unclosed
/// * Returns error if filter syntax is invalid
pub fn tokenize(input: &str) -> Result<Vec<Token>, FilterError> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some((start_idx, ch)) = chars.next() {
        match ch {
            // Skip whitespace
            ' ' | '\t' | '\n' | '\r' => {}

            // Grouping
            '(' => tokens.push(Token::LeftParen),
            ')' => tokens.push(Token::RightParen),

            // Check for keywords (AND, OR, NOT)
            'A' | 'a' | 'O' | 'o' | 'N' | 'n' => {
                if let Some((keyword_token, _)) = try_parse_keyword(input, start_idx, &mut chars) {
                    tokens.push(keyword_token);
                } else {
                    // Not a keyword - parse as filter
                    let filter = parse_filter_token(input, start_idx, &mut chars)?;
                    tokens.push(Token::Filter(filter));
                }
            }

            // Everything else starts a filter condition
            _ => {
                let filter = parse_filter_token(input, start_idx, &mut chars)?;
                tokens.push(Token::Filter(filter));
            }
        }
    }

    Ok(tokens)
}

/// Try to parse a keyword (AND, OR, NOT) at the current position.
///
/// Returns the token and how many characters were consumed, or None if not a keyword.
/// Does not consume from the chars iterator - caller must consume if successful.
fn try_parse_keyword(
    input: &str,
    start_idx: usize,
    chars: &mut Peekable<CharIndices>,
) -> Option<(Token, usize)> {
    // Look ahead without consuming
    let remaining = &input[start_idx..];

    // Try to match keywords using char-aware comparison
    let (keyword, keyword_len) = if starts_with_and(remaining) {
        (Some(Token::And), 3)
    } else if starts_with_not(remaining) {
        (Some(Token::Not), 3)
    } else if starts_with_or(remaining) {
        (Some(Token::Or), 2)
    } else {
        return None;
    };

    if let Some(token) = keyword {
        // Verify word boundaries
        let before_ok = start_idx == 0
            || input[..start_idx]
                .chars()
                .last()
                .is_none_or(|c| c.is_whitespace() || c == '(');

        // Check what comes after the keyword
        let after_ok = if remaining.len() == keyword_len {
            true // End of string
        } else {
            remaining
                .chars()
                .nth(keyword_len)
                .is_none_or(|c| c.is_whitespace() || c == '(' || c == ')')
        };

        if before_ok && after_ok {
            // Consume the keyword characters
            for _ in 0..keyword_len {
                chars.next();
            }
            return Some((token, keyword_len));
        }
    }

    None
}

/// Parse a filter condition token, handling quotes and escape sequences.
fn parse_filter_token(
    input: &str,
    start_idx: usize,
    chars: &mut Peekable<CharIndices>,
) -> Result<String, FilterError> {
    let mut filter_chars = Vec::new();
    let mut in_quotes = false;
    let mut escape_next = false;

    // First, collect the character at start_idx
    if let Some(ch) = input[start_idx..].chars().next() {
        filter_chars.push(ch);
        if ch == '"' {
            in_quotes = true;
        }
    }

    // Continue collecting characters
    while let Some(&(idx, ch)) = chars.peek() {
        if escape_next {
            // After backslash, take character literally
            filter_chars.push(ch);
            escape_next = false;
            chars.next();
            continue;
        }

        match ch {
            '\\' if in_quotes => {
                // Escape sequence in quoted string
                filter_chars.push(ch);
                escape_next = true;
                chars.next();
            }
            '"' => {
                // Toggle quote state
                filter_chars.push(ch);
                in_quotes = !in_quotes;
                chars.next();
            }
            '(' | ')' if !in_quotes => {
                // Grouping operators end the filter (unless quoted)
                break;
            }
            _ if !in_quotes && ch.is_whitespace() => {
                // Check if we're at a keyword boundary
                if looks_like_keyword_at(input, idx) {
                    break;
                }
                // Otherwise, whitespace might be part of the filter (before operator)
                filter_chars.push(ch);
                chars.next();
            }
            _ if !in_quotes && ch.is_alphabetic() => {
                // Check if we're at a keyword
                if looks_like_keyword_at(input, idx) {
                    break;
                }
                filter_chars.push(ch);
                chars.next();
            }
            _ => {
                // Inside quotes or regular characters, continue collecting
                filter_chars.push(ch);
                chars.next();
            }
        }
    }

    // Check for unclosed quotes
    if in_quotes {
        return Err(FilterError::UnclosedQuote(format!(
            "Unclosed quote in filter starting at position {start_idx}"
        )));
    }

    let filter_str: String = filter_chars.iter().collect();
    let filter_str = filter_str.trim();

    if filter_str.is_empty() {
        return Err(FilterError::InvalidSyntax(
            "Empty filter condition".to_string(),
        ));
    }

    // Validate it's a real filter by trying to parse it
    parse_filter(filter_str)?;

    Ok(filter_str.to_string())
}

/// Check if the position in the input looks like the start of a keyword.
fn looks_like_keyword_at(input: &str, pos: usize) -> bool {
    let remaining = &input[pos..];

    // Skip leading whitespace
    let remaining = remaining.trim_start();
    if remaining.is_empty() {
        return false;
    }

    // Check if it starts with a keyword using char-aware comparison
    let (keyword, keyword_len) = if starts_with_and(remaining) || starts_with_not(remaining) {
        (true, 3)
    } else if starts_with_or(remaining) {
        (true, 2)
    } else {
        return false;
    };

    if !keyword {
        return false;
    }

    // Check word boundary after keyword
    if remaining.len() == keyword_len {
        return true; // End of string
    }

    remaining
        .chars()
        .nth(keyword_len)
        .is_none_or(|c| c.is_whitespace() || c == '(' || c == ')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_filter() {
        let tokens = tokenize("publish=false").unwrap();
        assert_eq!(tokens, vec![Token::Filter("publish=false".to_string())]);
    }

    #[test]
    fn test_tokenize_and_expression() {
        let tokens = tokenize("publish=false AND version^=0.1").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Filter("publish=false".to_string()),
                Token::And,
                Token::Filter("version^=0.1".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_or_expression() {
        let tokens = tokenize("publish=false OR version^=0.1").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Filter("publish=false".to_string()),
                Token::Or,
                Token::Filter("version^=0.1".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_not_expression() {
        let tokens = tokenize("NOT publish=false").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Not, Token::Filter("publish=false".to_string()),]
        );
    }

    #[test]
    fn test_tokenize_parentheses() {
        let tokens = tokenize("(publish=false OR version^=0.1)").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LeftParen,
                Token::Filter("publish=false".to_string()),
                Token::Or,
                Token::Filter("version^=0.1".to_string()),
                Token::RightParen,
            ]
        );
    }

    #[test]
    fn test_tokenize_complex_expression() {
        let tokens = tokenize("(publish=false OR name$=_example) AND categories@=audio").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LeftParen,
                Token::Filter("publish=false".to_string()),
                Token::Or,
                Token::Filter("name$=_example".to_string()),
                Token::RightParen,
                Token::And,
                Token::Filter("categories@=audio".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_quoted_value() {
        let tokens = tokenize(r#"name="test package""#).unwrap();
        assert_eq!(
            tokens,
            vec![Token::Filter(r#"name="test package""#.to_string())]
        );
    }

    #[test]
    fn test_tokenize_quoted_with_keyword() {
        let tokens = tokenize(r#"description="This AND that""#).unwrap();
        assert_eq!(
            tokens,
            vec![Token::Filter(r#"description="This AND that""#.to_string())]
        );
    }

    #[test]
    fn test_tokenize_quoted_with_escaped_quote() {
        let tokens = tokenize(r#"title="Quote: \"test\"""#).unwrap();
        assert_eq!(
            tokens,
            vec![Token::Filter(r#"title="Quote: \"test\"""#.to_string())]
        );
    }

    #[test]
    fn test_tokenize_case_insensitive_keywords() {
        let tokens = tokenize("publish=false and version^=0.1 OR name=test").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Filter("publish=false".to_string()),
                Token::And,
                Token::Filter("version^=0.1".to_string()),
                Token::Or,
                Token::Filter("name=test".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_keyword_in_value() {
        // "ANDROID" should not be treated as AND + OID
        let tokens = tokenize("brand=ANDROID").unwrap();
        assert_eq!(tokens, vec![Token::Filter("brand=ANDROID".to_string())]);
    }

    #[test]
    fn test_tokenize_unclosed_quote() {
        let result = tokenize(r#"name="unclosed"#);
        assert!(matches!(result, Err(FilterError::UnclosedQuote(_))));
    }

    #[test]
    fn test_tokenize_standalone_keyword() {
        use super::super::expression_parser::parse_expression;

        // Tokenization should succeed, but parsing should fail
        let tokens = tokenize("AND").unwrap();
        assert_eq!(tokens, vec![Token::And]);

        // Parsing the expression should fail (UnexpectedToken, not ExpectedToken)
        let result = parse_expression("AND");
        assert!(matches!(result, Err(FilterError::UnexpectedToken(_))));
    }

    #[test]
    fn test_starts_with_and() {
        assert!(starts_with_and("AND"));
        assert!(starts_with_and("AND "));
        assert!(starts_with_and("and"));
        assert!(starts_with_and("AnD"));
        assert!(starts_with_and("AND rest of string"));

        assert!(!starts_with_and("AN")); // Too short
        assert!(starts_with_and("ANDROID")); // This DOES match AND prefix - boundaries checked by caller
        assert!(!starts_with_and("OR"));
        assert!(!starts_with_and(""));
        assert!(!starts_with_and("名前")); // Unicode chars
    }

    #[test]
    fn test_starts_with_or() {
        assert!(starts_with_or("OR"));
        assert!(starts_with_or("or"));
        assert!(starts_with_or("Or"));
        assert!(starts_with_or("OR more"));

        assert!(!starts_with_or("O")); // Too short
        assert!(!starts_with_or("AND"));
        assert!(!starts_with_or(""));
    }

    #[test]
    fn test_starts_with_not() {
        assert!(starts_with_not("NOT"));
        assert!(starts_with_not("not"));
        assert!(starts_with_not("NoT"));
        assert!(starts_with_not("NOT more"));

        assert!(!starts_with_not("NO")); // Too short
        assert!(!starts_with_not("AND"));
        assert!(!starts_with_not(""));
    }

    #[test]
    fn test_unicode_does_not_match_keywords() {
        // Full-width characters that look like AND/OR/NOT
        assert!(!starts_with_and("ＡＮＤ"));
        assert!(!starts_with_or("ＯＲ"));
        assert!(!starts_with_not("ＮＯＴ"));

        // Unicode that starts with similar letters
        assert!(!starts_with_and("名前"));
        assert!(!starts_with_or("おはよう"));
    }
}
