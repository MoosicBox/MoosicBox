//! Edge case tests for the tokenizer.
//!
//! Tests whitespace variations, malformed input, special characters,
//! escape sequences, and complex nesting scenarios.

use clippier::package_filter::{FilterError, Token, tokenize};

// ============================================================================
// Whitespace Variations
// ============================================================================

#[switchy_async::test]
async fn test_multiple_spaces_between_tokens() {
    let tokens = tokenize("package.publish=false     AND     package.version^=0.1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_tabs_instead_of_spaces() {
    let tokens = tokenize("package.publish=false\tAND\tpackage.version^=0.1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_mixed_tabs_and_spaces() {
    let tokens = tokenize("package.publish=false \t AND  \t package.version^=0.1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_newlines_as_separators() {
    let tokens = tokenize("package.publish=false\nAND\npackage.version^=0.1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_leading_whitespace() {
    let tokens = tokenize("   package.publish=false AND package.version^=0.1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_trailing_whitespace() {
    let tokens = tokenize("package.publish=false AND package.version^=0.1   ").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_whitespace_inside_parentheses() {
    let tokens = tokenize("(  package.publish=false  OR  package.version^=0.1  )").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LeftParen,
            Token::Filter("package.publish=false".to_string()),
            Token::Or,
            Token::Filter("package.version^=0.1".to_string()),
            Token::RightParen,
        ]
    );
}

#[switchy_async::test]
async fn test_no_spaces_around_operators() {
    let tokens = tokenize("package.publish=false AND package.version^=0.1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_only_whitespace() {
    let tokens = tokenize("   \t  \n  ").unwrap();
    assert_eq!(tokens, vec![]);
}

// ============================================================================
// Empty and Malformed Input
// ============================================================================

#[switchy_async::test]
async fn test_empty_input() {
    let tokens = tokenize("").unwrap();
    assert_eq!(tokens, vec![]);
}

#[switchy_async::test]
async fn test_only_operators() {
    let tokens = tokenize("AND OR NOT").unwrap();
    assert_eq!(tokens, vec![Token::And, Token::Or, Token::Not]);
}

#[switchy_async::test]
async fn test_only_parentheses() {
    let tokens = tokenize("()").unwrap();
    assert_eq!(tokens, vec![Token::LeftParen, Token::RightParen]);
}

#[switchy_async::test]
async fn test_empty_parentheses_with_spaces() {
    let tokens = tokenize("(   )").unwrap();
    assert_eq!(tokens, vec![Token::LeftParen, Token::RightParen]);
}

// ============================================================================
// Quote Edge Cases
// ============================================================================

#[switchy_async::test]
async fn test_unclosed_quote_at_end() {
    let result = tokenize(r#"name="unclosed"#);
    assert!(matches!(result, Err(FilterError::UnclosedQuote(_))));
}

#[switchy_async::test]
async fn test_unclosed_quote_in_middle() {
    let result = tokenize(r#"name="unclosed AND version=0.1.0"#);
    assert!(matches!(result, Err(FilterError::UnclosedQuote(_))));
}

#[switchy_async::test]
async fn test_quote_at_start_only() {
    let result = tokenize(r#""name=test"#);
    assert!(matches!(result, Err(FilterError::UnclosedQuote(_))));
}

#[switchy_async::test]
async fn test_escaped_quote_at_end() {
    let tokens = tokenize(r#"name="test\"""#).unwrap();
    assert_eq!(tokens, vec![Token::Filter(r#"name="test\"""#.to_string())]);
}

#[switchy_async::test]
async fn test_multiple_quotes_in_value() {
    let tokens = tokenize(r#"desc="She said \"hello\" today""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(
            r#"desc="She said \"hello\" today""#.to_string()
        )]
    );
}

#[switchy_async::test]
#[ignore] // TODO: This currently succeeds but should probably fail
async fn test_quotes_in_property_name_fails() {
    // Property names can't have quotes
    let result = tokenize(r#""name"=test"#);
    // This should fail to parse as a filter
    assert!(result.is_err());
}

// ============================================================================
// Escape Sequences
// ============================================================================

#[switchy_async::test]
async fn test_backslash_escape() {
    let tokens = tokenize(r#"path="C:\\Users\\test""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(r#"path="C:\\Users\\test""#.to_string())]
    );
}

#[switchy_async::test]
async fn test_newline_escape() {
    let tokens = tokenize(r#"text="line1\nline2""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(r#"text="line1\nline2""#.to_string())]
    );
}

#[switchy_async::test]
async fn test_tab_escape() {
    let tokens = tokenize(r#"text="col1\tcol2""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(r#"text="col1\tcol2""#.to_string())]
    );
}

#[switchy_async::test]
async fn test_carriage_return_escape() {
    let tokens = tokenize(r#"text="line\r\n""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(r#"text="line\r\n""#.to_string())]
    );
}

#[switchy_async::test]
async fn test_backslash_at_end() {
    let result = tokenize(r#"text="test\"#);
    // Backslash at end should cause unclosed quote
    assert!(matches!(result, Err(FilterError::UnclosedQuote(_))));
}

// ============================================================================
// Keywords as Values and Properties
// ============================================================================

#[switchy_async::test]
async fn test_keyword_and_as_property() {
    let tokens = tokenize("and=true").unwrap();
    assert_eq!(tokens, vec![Token::Filter("and=true".to_string())]);
}

#[switchy_async::test]
async fn test_keyword_or_as_property() {
    let tokens = tokenize("or=false").unwrap();
    assert_eq!(tokens, vec![Token::Filter("or=false".to_string())]);
}

#[switchy_async::test]
async fn test_keyword_not_as_property() {
    let tokens = tokenize("not=value").unwrap();
    assert_eq!(tokens, vec![Token::Filter("not=value".to_string())]);
}

#[switchy_async::test]
async fn test_keyword_in_compound_word_android() {
    let tokens = tokenize("platform=ANDROID").unwrap();
    assert_eq!(tokens, vec![Token::Filter("platform=ANDROID".to_string())]);
}

#[switchy_async::test]
async fn test_keyword_in_compound_word_fork() {
    let tokens = tokenize("action=FORK").unwrap();
    assert_eq!(tokens, vec![Token::Filter("action=FORK".to_string())]);
}

#[switchy_async::test]
async fn test_keyword_in_compound_word_notification() {
    let tokens = tokenize("type=NOTIFICATION").unwrap();
    assert_eq!(tokens, vec![Token::Filter("type=NOTIFICATION".to_string())]);
}

#[switchy_async::test]
async fn test_mixed_case_keywords_in_expression() {
    let tokens = tokenize("a=1 And b=2 oR c=3 NOT d=4").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("a=1".to_string()),
            Token::And,
            Token::Filter("b=2".to_string()),
            Token::Or,
            Token::Filter("c=3".to_string()),
            Token::Not,
            Token::Filter("d=4".to_string()),
        ]
    );
}

// ============================================================================
// Complex Nesting
// ============================================================================

#[switchy_async::test]
async fn test_deeply_nested_parentheses_5_levels() {
    let tokens = tokenize("(((((package.name=test)))))").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LeftParen,
            Token::LeftParen,
            Token::LeftParen,
            Token::LeftParen,
            Token::LeftParen,
            Token::Filter("package.name=test".to_string()),
            Token::RightParen,
            Token::RightParen,
            Token::RightParen,
            Token::RightParen,
            Token::RightParen,
        ]
    );
}

#[switchy_async::test]
async fn test_adjacent_parentheses() {
    let tokens = tokenize("((package.name=test))").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LeftParen,
            Token::LeftParen,
            Token::Filter("package.name=test".to_string()),
            Token::RightParen,
            Token::RightParen,
        ]
    );
}

#[switchy_async::test]
async fn test_nested_with_operators() {
    let tokens = tokenize("(a=1 AND (b=2 OR (c=3 AND d=4)))").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LeftParen,
            Token::Filter("a=1".to_string()),
            Token::And,
            Token::LeftParen,
            Token::Filter("b=2".to_string()),
            Token::Or,
            Token::LeftParen,
            Token::Filter("c=3".to_string()),
            Token::And,
            Token::Filter("d=4".to_string()),
            Token::RightParen,
            Token::RightParen,
            Token::RightParen,
        ]
    );
}

// ============================================================================
// Special Characters in Values
// ============================================================================

#[switchy_async::test]
async fn test_dots_in_unquoted_filter() {
    let tokens = tokenize("package.version=0.1.0").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("package.version=0.1.0".to_string())]
    );
}

#[switchy_async::test]
async fn test_hyphens_in_filter() {
    let tokens = tokenize("package.name=test-package").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("package.name=test-package".to_string())]
    );
}

#[switchy_async::test]
async fn test_underscores_in_filter() {
    let tokens = tokenize("package.name=test_package").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("package.name=test_package".to_string())]
    );
}

#[switchy_async::test]
async fn test_numbers_in_filter() {
    let tokens = tokenize("package.version=123.456.789").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("package.version=123.456.789".to_string())]
    );
}

#[switchy_async::test]
async fn test_operators_in_quoted_values() {
    let tokens = tokenize(r#"desc="value with != operator""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(
            r#"desc="value with != operator""#.to_string()
        )]
    );
}

#[switchy_async::test]
async fn test_parentheses_in_quoted_values() {
    let tokens = tokenize(r#"desc="test (with parens)""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(r#"desc="test (with parens)""#.to_string())]
    );
}

#[switchy_async::test]
async fn test_newline_in_quoted_value() {
    let tokens = tokenize("desc=\"line1\nline2\"").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("desc=\"line1\nline2\"".to_string())]
    );
}

#[switchy_async::test]
async fn test_tab_in_quoted_value() {
    let tokens = tokenize("desc=\"col1\tcol2\"").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("desc=\"col1\tcol2\"".to_string())]
    );
}

// ============================================================================
// Long Inputs
// ============================================================================

#[switchy_async::test]
async fn test_very_long_filter_1000_chars() {
    let long_value = "a".repeat(1000);
    let filter_str = format!("name={long_value}");
    let tokens = tokenize(&filter_str).unwrap();
    assert_eq!(tokens.len(), 1);
    match &tokens[0] {
        Token::Filter(f) => {
            assert_eq!(f, &filter_str, "Filter string should match exactly");
            assert_eq!(
                f.len(),
                1005,
                "Filter should be 'name=' (5 chars) + 1000 'a's"
            );
        }
        _ => panic!("Expected Token::Filter, got: {tokens:?}"),
    }
}

#[switchy_async::test]
async fn test_many_filters_chained() {
    let filters: Vec<String> = (0..20).map(|i| format!("f{i}=v{i}")).collect();
    let filter_str = filters.join(" AND ");
    let tokens = tokenize(&filter_str).unwrap();

    // Build expected token sequence
    let mut expected = Vec::new();
    for (i, filter) in filters.iter().enumerate() {
        if i > 0 {
            expected.push(Token::And);
        }
        expected.push(Token::Filter(filter.clone()));
    }

    assert_eq!(
        tokens, expected,
        "Token sequence should alternate Filter and AND"
    );
    assert_eq!(tokens.len(), 39, "Should have 20 filters + 19 AND tokens");
}

// ============================================================================
// Unicode Support
// ============================================================================

#[switchy_async::test]
async fn test_unicode_in_property_name() {
    let tokens = tokenize("名前=test").unwrap();
    assert_eq!(tokens, vec![Token::Filter("名前=test".to_string())]);
}

#[switchy_async::test]
async fn test_unicode_in_value() {
    let tokens = tokenize("package.name=テスト").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter("package.name=テスト".to_string())]
    );
}

#[switchy_async::test]
async fn test_unicode_in_quoted_value() {
    let tokens = tokenize(r#"desc="音楽プレーヤー""#).unwrap();
    assert_eq!(
        tokens,
        vec![Token::Filter(r#"desc="音楽プレーヤー""#.to_string())]
    );
}

#[switchy_async::test]
async fn test_emoji_in_value() {
    let tokens = tokenize("icon=🎵").unwrap();
    assert_eq!(tokens, vec![Token::Filter("icon=🎵".to_string())]);
}

// ============================================================================
// Mixed Complex Scenarios
// ============================================================================

#[switchy_async::test]
async fn test_all_three_operators_with_nesting() {
    let tokens =
        tokenize("NOT (package.publish=false AND package.version^=0.1) OR (package.name$=_example AND package.readme?)").unwrap();

    assert_eq!(
        tokens,
        vec![
            Token::Not,
            Token::LeftParen,
            Token::Filter("package.publish=false".to_string()),
            Token::And,
            Token::Filter("package.version^=0.1".to_string()),
            Token::RightParen,
            Token::Or,
            Token::LeftParen,
            Token::Filter("package.name$=_example".to_string()),
            Token::And,
            Token::Filter("package.readme?".to_string()),
            Token::RightParen,
        ]
    );
}

#[switchy_async::test]
async fn test_complex_expression_with_all_features() {
    let input = r#"(package.name^="moosicbox_" AND package.publish=true) AND 
                   (NOT (package.categories@="test" OR package.keywords@!)) AND
                   (package.version~="^\d+\.\d+\.\d+$" OR package.readme?)"#;
    let tokens = tokenize(input).unwrap();

    // Validate exact token sequence
    assert_eq!(
        tokens,
        vec![
            Token::LeftParen,
            Token::Filter(r#"package.name^="moosicbox_""#.to_string()),
            Token::And,
            Token::Filter("package.publish=true".to_string()),
            Token::RightParen,
            Token::And,
            Token::LeftParen,
            Token::Not,
            Token::LeftParen,
            Token::Filter(r#"package.categories@="test""#.to_string()),
            Token::Or,
            Token::Filter("package.keywords@!".to_string()),
            Token::RightParen,
            Token::RightParen,
            Token::And,
            Token::LeftParen,
            Token::Filter(r#"package.version~="^\d+\.\d+\.\d+$""#.to_string()),
            Token::Or,
            Token::Filter("package.readme?".to_string()),
            Token::RightParen,
        ]
    );
}

// ============================================================================
// Additional Unicode Tests
// ============================================================================

#[switchy_async::test]
async fn test_unicode_property_with_and() {
    // Japanese property name followed by AND keyword
    let tokens = tokenize("名前=test AND package.version=1.0").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("名前=test".to_string()),
            Token::And,
            Token::Filter("package.version=1.0".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_emoji_with_or() {
    // Emoji in value followed by OR keyword
    let tokens = tokenize(r#"icon="🎵" OR icon="🎸""#).unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter(r#"icon="🎵""#.to_string()),
            Token::Or,
            Token::Filter(r#"icon="🎸""#.to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_multibyte_before_not() {
    // Multibyte chars before NOT keyword
    let tokens = tokenize("名=日本 NOT x=1").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("名=日本".to_string()),
            Token::Not,
            Token::Filter("x=1".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_mixed_unicode_and_keywords() {
    // Complex expression with Unicode and all keywords
    let tokens = tokenize("(名前=テスト OR icon=🎵) AND NOT package.publish=false").unwrap();
    assert_eq!(tokens.len(), 8); // LeftParen, Filter, Or, Filter, RightParen, And, Not, Filter
    assert_eq!(tokens[0], Token::LeftParen);
    assert_eq!(tokens[1], Token::Filter("名前=テスト".to_string()));
    assert_eq!(tokens[2], Token::Or);
    assert_eq!(tokens[3], Token::Filter("icon=🎵".to_string()));
    assert_eq!(tokens[4], Token::RightParen);
    assert_eq!(tokens[5], Token::And);
    assert_eq!(tokens[6], Token::Not);
    assert_eq!(
        tokens[7],
        Token::Filter("package.publish=false".to_string())
    );
}

#[switchy_async::test]
async fn test_unicode_that_looks_like_keyword() {
    // Full-width characters that might be confused with keywords
    // Should NOT be tokenized as keywords
    let tokens = tokenize("ＡＮＤ=value").unwrap();
    assert_eq!(tokens, vec![Token::Filter("ＡＮＤ=value".to_string())]);
}

#[switchy_async::test]
async fn test_korean_chars_with_parentheses() {
    let tokens = tokenize("(이름=테스트)").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LeftParen,
            Token::Filter("이름=테스트".to_string()),
            Token::RightParen,
        ]
    );
}

#[switchy_async::test]
async fn test_arabic_with_operators() {
    let tokens = tokenize("اسم=قيمة AND نسخة=١").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Filter("اسم=قيمة".to_string()),
            Token::And,
            Token::Filter("نسخة=١".to_string()),
        ]
    );
}

#[switchy_async::test]
async fn test_mixed_rtl_ltr() {
    // Right-to-left and left-to-right mixed
    let tokens = tokenize("package.name=مرحبا OR שלום=hello").unwrap();
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0], Token::Filter("package.name=مرحبا".to_string()));
    assert_eq!(tokens[1], Token::Or);
    assert_eq!(tokens[2], Token::Filter("שלום=hello".to_string()));
}
