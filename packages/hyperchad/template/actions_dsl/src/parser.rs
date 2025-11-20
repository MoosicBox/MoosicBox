//! Parser for `HyperChad` Actions DSL
//!
//! This module implements a parser that can handle Rust-like syntax for defining actions.

use proc_macro2::TokenStream;
use syn::{
    Ident, Lit, braced, bracketed, parenthesized,
    parse::{ParseStream, Parser, Result},
    token,
};

use hyperchad_actions::dsl::{
    BinaryOp, Block, Dsl, Expression, Literal, MatchArm, Pattern, Statement, UnaryOp,
};

/// Parses the entire DSL input into a `Dsl` structure
///
/// This is the main entry point for parsing DSL syntax. It processes all statements
/// in the input stream sequentially.
///
/// # Errors
///
/// * Returns parse error if the input contains invalid syntax
/// * Returns error on unexpected end of input
/// * Returns error if a statement cannot be parsed
pub fn parse_dsl(input: ParseStream) -> Result<Dsl> {
    let mut statements = Vec::new();

    while !input.is_empty() {
        let stmt = parse_statement(input)?;
        statements.push(stmt);
    }

    Ok(Dsl::new(statements))
}

/// Parses a single statement from the input stream
///
/// Attempts to parse the statement as DSL syntax first, falling back to raw Rust
/// code if DSL parsing fails.
///
/// # Errors
///
/// * Returns error if input is unexpectedly empty
/// * Returns error if both DSL and raw Rust parsing fail
fn parse_statement(input: ParseStream) -> Result<Statement> {
    // Check if input is empty first
    if input.is_empty() {
        return Err(input.error("Unexpected end of input"));
    }

    // Try to parse the statement normally first
    try_parse_statement_normal(input).map_or_else(|_| try_parse_statement_as_raw_rust(input), Ok)
}

/// Attempts to parse a statement using normal DSL parsing rules
///
/// Handles let bindings, if statements, match expressions, for loops, while loops,
/// blocks, and expression statements.
///
/// # Errors
///
/// Returns error if the statement doesn't match any known DSL pattern
fn try_parse_statement_normal(input: ParseStream) -> Result<Statement> {
    let lookahead = input.lookahead1();

    if lookahead.peek(token::Let) {
        parse_let_statement(input)
    } else if lookahead.peek(token::If) {
        parse_if_statement(input)
    } else if lookahead.peek(token::Match) {
        parse_match_statement(input)
    } else if lookahead.peek(token::For) {
        parse_for_statement(input)
    } else if lookahead.peek(token::While) {
        parse_while_statement(input)
    } else if lookahead.peek(token::Brace) {
        let block = parse_block(input)?;
        Ok(Statement::Block(block))
    } else {
        // Expression statement
        let expr = parse_expression(input)?;
        if input.peek(token::Semi) {
            input.parse::<token::Semi>()?;
        }
        Ok(Statement::Expression(expr))
    }
}

/// Attempts to parse a statement as raw Rust code
///
/// This fallback parser collects tokens until it hits a semicolon or end of input,
/// then wraps them as raw Rust code.
///
/// # Errors
///
/// * Returns error if the statement is empty
/// * Returns error if token parsing fails
fn try_parse_statement_as_raw_rust(input: ParseStream) -> Result<Statement> {
    // Collect tokens until we hit a semicolon or end of input
    let mut tokens = Vec::new();

    while !input.is_empty() && !input.peek(token::Semi) {
        let token: proc_macro2::TokenTree = input.parse()?;
        tokens.push(token);
    }

    // Consume the semicolon if present
    if input.peek(token::Semi) {
        input.parse::<token::Semi>()?;
        tokens.push(proc_macro2::TokenTree::Punct(proc_macro2::Punct::new(
            ';',
            proc_macro2::Spacing::Alone,
        )));
    }

    if tokens.is_empty() {
        return Err(input.error("Empty statement"));
    }

    // Combine tokens into raw Rust code
    let raw_code: TokenStream = tokens.into_iter().collect();
    let raw_code_str = raw_code.to_string();

    Ok(Statement::Expression(Expression::RawRust(raw_code_str)))
}

/// Parse a let statement
fn parse_let_statement(input: ParseStream) -> Result<Statement> {
    input.parse::<token::Let>()?;
    let name: Ident = input.parse()?;
    input.parse::<token::Eq>()?;
    let value = parse_expression(input)?;
    input.parse::<token::Semi>()?;

    Ok(Statement::Let {
        name: name.to_string(),
        value,
    })
}

/// Parse an if statement
fn parse_if_statement(input: ParseStream) -> Result<Statement> {
    input.parse::<token::If>()?;
    let condition = parse_expression(input)?;
    let then_block = parse_block(input)?;

    let else_block = if input.peek(token::Else) {
        input.parse::<token::Else>()?;
        if input.peek(token::If) {
            // else if - parse as nested if
            let nested_if = parse_if_statement(input)?;
            Some(Block {
                statements: vec![nested_if],
            })
        } else {
            Some(parse_block(input)?)
        }
    } else {
        None
    };

    Ok(Statement::If {
        condition,
        then_block,
        else_block,
    })
}

/// Parse a match statement
fn parse_match_statement(input: ParseStream) -> Result<Statement> {
    input.parse::<token::Match>()?;
    let expr = parse_expression(input)?;

    let content;
    braced!(content in input);

    let mut arms = Vec::new();
    while !content.is_empty() {
        let pattern = parse_pattern(&content)?;
        content.parse::<token::FatArrow>()?;
        let body = parse_expression(&content)?;

        arms.push(MatchArm { pattern, body });

        if content.peek(token::Comma) {
            content.parse::<token::Comma>()?;
        }
    }

    Ok(Statement::Match { expr, arms })
}

/// Parse a for loop
fn parse_for_statement(input: ParseStream) -> Result<Statement> {
    input.parse::<token::For>()?;
    let pattern: Ident = input.parse()?;
    input.parse::<token::In>()?;
    let iter = parse_expression(input)?;
    let body = parse_block(input)?;

    Ok(Statement::For {
        pattern: pattern.to_string(),
        iter,
        body,
    })
}

/// Parse a while loop
fn parse_while_statement(input: ParseStream) -> Result<Statement> {
    input.parse::<token::While>()?;
    let condition = parse_expression(input)?;
    let body = parse_block(input)?;

    Ok(Statement::While { condition, body })
}

/// Parse a block
fn parse_block(input: ParseStream) -> Result<Block> {
    let content;
    braced!(content in input);

    let mut statements = Vec::new();
    while !content.is_empty() {
        let stmt = parse_statement(&content)?;
        statements.push(stmt);
    }

    Ok(Block { statements })
}

/// Parse an expression
fn parse_expression(input: ParseStream) -> Result<Expression> {
    parse_or_expression(input)
}

/// Parse logical OR expression
fn parse_or_expression(input: ParseStream) -> Result<Expression> {
    let mut left = parse_and_expression(input)?;

    while input.peek(token::OrOr) {
        input.parse::<token::OrOr>()?;
        let right = parse_and_expression(input)?;
        left = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Or,
            right: Box::new(right),
        };
    }

    Ok(left)
}

/// Parse logical AND expression
fn parse_and_expression(input: ParseStream) -> Result<Expression> {
    let mut left = parse_equality_expression(input)?;

    while input.peek(token::AndAnd) {
        input.parse::<token::AndAnd>()?;
        let right = parse_equality_expression(input)?;
        left = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::And,
            right: Box::new(right),
        };
    }

    Ok(left)
}

/// Parse equality expression
fn parse_equality_expression(input: ParseStream) -> Result<Expression> {
    let mut left = parse_comparison_expression(input)?;

    while input.peek(token::EqEq) || input.peek(token::Ne) {
        let op = if input.peek(token::EqEq) {
            input.parse::<token::EqEq>()?;
            BinaryOp::Equal
        } else {
            input.parse::<token::Ne>()?;
            BinaryOp::NotEqual
        };

        let right = parse_comparison_expression(input)?;
        left = Expression::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        };
    }

    Ok(left)
}

/// Parse comparison expression
fn parse_comparison_expression(input: ParseStream) -> Result<Expression> {
    let mut left = parse_additive_expression(input)?;

    while input.peek(token::Lt)
        || input.peek(token::Le)
        || input.peek(token::Gt)
        || input.peek(token::Ge)
    {
        let op = if input.peek(token::Lt) {
            input.parse::<token::Lt>()?;
            BinaryOp::Less
        } else if input.peek(token::Le) {
            input.parse::<token::Le>()?;
            BinaryOp::LessEqual
        } else if input.peek(token::Gt) {
            input.parse::<token::Gt>()?;
            BinaryOp::Greater
        } else {
            input.parse::<token::Ge>()?;
            BinaryOp::GreaterEqual
        };

        let right = parse_additive_expression(input)?;
        left = Expression::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        };
    }

    Ok(left)
}

/// Parse additive expression
fn parse_additive_expression(input: ParseStream) -> Result<Expression> {
    let mut left = parse_multiplicative_expression(input)?;

    while input.peek(token::Plus) || input.peek(token::Minus) {
        let op = if input.peek(token::Plus) {
            input.parse::<token::Plus>()?;
            BinaryOp::Add
        } else {
            input.parse::<token::Minus>()?;
            BinaryOp::Subtract
        };

        let right = parse_multiplicative_expression(input)?;
        left = Expression::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        };
    }

    Ok(left)
}

/// Parse multiplicative expression
fn parse_multiplicative_expression(input: ParseStream) -> Result<Expression> {
    let mut left = parse_unary_expression(input)?;

    while input.peek(token::Star) || input.peek(token::Slash) || input.peek(token::Percent) {
        let op = if input.peek(token::Star) {
            input.parse::<token::Star>()?;
            BinaryOp::Multiply
        } else if input.peek(token::Slash) {
            input.parse::<token::Slash>()?;
            BinaryOp::Divide
        } else {
            input.parse::<token::Percent>()?;
            BinaryOp::Modulo
        };

        let right = parse_unary_expression(input)?;
        left = Expression::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        };
    }

    Ok(left)
}

/// Parse unary expression
fn parse_unary_expression(input: ParseStream) -> Result<Expression> {
    if input.peek(token::Not) {
        input.parse::<token::Not>()?;
        let expr = parse_unary_expression(input)?;
        Ok(Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(expr),
        })
    } else if input.peek(token::Minus) {
        input.parse::<token::Minus>()?;
        let expr = parse_unary_expression(input)?;
        Ok(Expression::Unary {
            op: UnaryOp::Minus,
            expr: Box::new(expr),
        })
    } else if input.peek(token::Plus) {
        input.parse::<token::Plus>()?;
        let expr = parse_unary_expression(input)?;
        Ok(Expression::Unary {
            op: UnaryOp::Plus,
            expr: Box::new(expr),
        })
    } else if input.peek(token::And) {
        input.parse::<token::And>()?;
        let expr = parse_unary_expression(input)?;
        Ok(Expression::Unary {
            op: UnaryOp::Ref,
            expr: Box::new(expr),
        })
    } else {
        parse_postfix_expression(input)
    }
}

/// Parse postfix expressions (method calls, field access, etc.)
fn parse_postfix_expression(input: ParseStream) -> Result<Expression> {
    let mut expr = parse_primary_expression(input)?;

    loop {
        if input.peek(token::Dot) {
            input.parse::<token::Dot>()?;
            let method: Ident = input.parse()?;

            if input.peek(token::Paren) {
                // Method call
                let content;
                parenthesized!(content in input);
                let args = if content.is_empty() {
                    Vec::new()
                } else {
                    parse_expression_list(&content)?
                };

                expr = Expression::MethodCall {
                    receiver: Box::new(expr),
                    method: method.to_string(),
                    args,
                };
            } else {
                // Field access
                expr = Expression::Field {
                    object: Box::new(expr),
                    field: method.to_string(),
                };
            }
        } else if input.peek(token::Paren) {
            // Function call
            let content;
            parenthesized!(content in input);
            let args = if content.is_empty() {
                Vec::new()
            } else {
                parse_expression_list(&content)?
            };

            // Convert function call to method call if needed
            if let Expression::Variable(func_name) = expr {
                expr = if func_name == "element" && args.len() == 1 {
                    Expression::ElementRef(Box::new(args[0].clone()))
                } else {
                    Expression::Call {
                        function: func_name,
                        args,
                    }
                };
            } else {
                return Err(input.error("Invalid function call"));
            }
        } else if input.peek(token::Bracket) {
            // Array indexing
            let content;
            bracketed!(content in input);
            let index = parse_expression(&content)?;

            expr = Expression::MethodCall {
                receiver: Box::new(expr),
                method: "index".to_string(),
                args: vec![index],
            };
        } else {
            break;
        }
    }

    Ok(expr)
}

/// Parse primary expressions (literals, variables, function calls, etc.)
fn parse_primary_expression(input: ParseStream) -> Result<Expression> {
    let lookahead = input.lookahead1();

    if lookahead.peek(syn::LitInt)
        || lookahead.peek(syn::LitFloat)
        || lookahead.peek(syn::LitStr)
        || lookahead.peek(syn::LitBool)
        || lookahead.peek(syn::LitChar)
    {
        let lit: Lit = input.parse()?;
        Ok(Expression::Literal(parse_literal(lit)?))
    } else if lookahead.peek(Ident) {
        let ident: Ident = input.parse()?;
        let ident_str = ident.to_string();

        // Check for enum variants (Type::Variant)
        if input.peek(token::PathSep) {
            input.parse::<token::PathSep>()?;

            if input.peek(Ident) {
                let variant: Ident = input.parse()?;

                // Check for struct-like variant with fields { field: value }
                // Only parse as struct variant if the brace contains field assignments
                // (identifiers followed by colons), not arbitrary statements
                if input.peek(token::Brace) {
                    // Look ahead to see if this looks like a struct variant or just a block
                    let fork = input.fork();
                    let content;
                    braced!(content in fork);

                    // Check if the first token in the brace looks like a field assignment
                    if content.peek(Ident) {
                        let field_fork = content.fork();
                        if field_fork.parse::<Ident>().is_ok()
                            && (field_fork.peek(token::Colon)
                                || field_fork.peek(token::Comma)
                                || field_fork.is_empty())
                        {
                            // This looks like a struct variant - parse it
                            let content;
                            braced!(content in input);

                            let mut fields = Vec::new();
                            while !content.is_empty() {
                                let field_name: Ident = content.parse()?;

                                let field_value = if content.peek(token::Colon) {
                                    // Full syntax: field: value
                                    content.parse::<token::Colon>()?;
                                    parse_expression(&content)?
                                } else {
                                    // Shorthand syntax: field (equivalent to field: field)
                                    Expression::Variable(field_name.to_string())
                                };

                                fields.push((field_name.to_string(), field_value));

                                if content.peek(token::Comma) {
                                    content.parse::<token::Comma>()?;
                                }
                            }

                            // Represent struct variants as function calls with named arguments
                            return Ok(Expression::Call {
                                function: format!("{ident_str}::{variant}"),
                                args: fields
                                    .into_iter()
                                    .map(|(name, value)| {
                                        // Create a tuple expression for named fields
                                        Expression::Tuple(vec![
                                            Expression::Literal(Literal::String(name)),
                                            value,
                                        ])
                                    })
                                    .collect(),
                            });
                        }
                    }
                }

                // Simple enum variant - treat any Type::Variant as a variable
                // This allows Key::Escape, ActionType::SomeAction, MyEnum::Variant, etc.
                return Ok(Expression::Variable(format!("{ident_str}::{variant}")));
            }

            return Err(input.error("Expected enum variant name"));
        }

        Ok(Expression::Variable(ident_str))
    } else if lookahead.peek(token::Or) {
        // Parse closure: |param| { ... }
        parse_closure(input)
    } else if lookahead.peek(token::Paren) {
        let content;
        parenthesized!(content in input);
        if content.is_empty() {
            Ok(Expression::Literal(Literal::Unit))
        } else {
            // Preserve grouping information by wrapping in Grouping variant
            let inner_expr = parse_expression(&content)?;
            Ok(Expression::Grouping(Box::new(inner_expr)))
        }
    } else if lookahead.peek(token::Bracket) {
        let content;
        bracketed!(content in input);
        let exprs = if content.is_empty() {
            Vec::new()
        } else {
            parse_expression_list(&content)?
        };
        Ok(Expression::Array(exprs))
    } else if lookahead.peek(token::If) {
        parse_if_expression(input)
    } else if lookahead.peek(token::Match) {
        parse_match_expression(input)
    } else if lookahead.peek(token::Brace) {
        let block = parse_block(input)?;
        Ok(Expression::Block(block))
    } else {
        // Try to parse as raw Rust expression if it's a complex pattern we don't recognize
        // This handles cases like generics, complex method chains, etc.
        try_parse_as_raw_rust(input)
    }
}

/// Parse a list of expressions separated by commas
fn parse_expression_list(input: ParseStream) -> Result<Vec<Expression>> {
    let mut exprs = Vec::new();

    while !input.is_empty() {
        // For function arguments, be more aggressive about using raw Rust fallback
        // Check if the argument looks complex (contains certain patterns)
        let lookahead = input.lookahead1();
        let use_raw_rust = lookahead.peek(token::And) || // &if, &[...]
                          input.peek(token::If); // if expressions

        let expr = if use_raw_rust {
            // Use raw Rust parsing for complex expressions
            try_parse_as_raw_rust(input)?
        } else {
            // Try to parse as normal DSL expression first
            match parse_expression(input) {
                Ok(expr) => expr,
                Err(_) => {
                    // If normal parsing fails, try to parse as raw Rust code
                    try_parse_as_raw_rust(input)?
                }
            }
        };
        exprs.push(expr);

        if input.peek(token::Comma) {
            input.parse::<token::Comma>()?;
        } else {
            break;
        }
    }

    Ok(exprs)
}

/// Parse a pattern
fn parse_pattern(input: ParseStream) -> Result<Pattern> {
    let lookahead = input.lookahead1();

    if lookahead.peek(Lit) {
        let lit: Lit = input.parse()?;
        Ok(Pattern::Literal(parse_literal(lit)?))
    } else if lookahead.peek(Ident) {
        let ident: Ident = input.parse()?;

        if input.peek(token::PathSep) {
            // Enum variant pattern
            input.parse::<token::PathSep>()?;
            let variant: Ident = input.parse()?;

            // TODO: Handle variant fields
            Ok(Pattern::Variant {
                enum_name: ident.to_string(),
                variant: variant.to_string(),
                fields: Vec::new(),
            })
        } else {
            // Variable pattern
            Ok(Pattern::Variable(ident.to_string()))
        }
    } else if lookahead.peek(token::Underscore) {
        input.parse::<token::Underscore>()?;
        Ok(Pattern::Wildcard)
    } else {
        Err(lookahead.error())
    }
}

/// Parse a literal
fn parse_literal(lit: Lit) -> Result<Literal> {
    match lit {
        Lit::Str(s) => Ok(Literal::String(s.value())),
        Lit::Int(i) => Ok(Literal::Integer(i.base10_parse()?)),
        Lit::Float(f) => Ok(Literal::Float(f.base10_parse()?)),
        Lit::Bool(b) => Ok(Literal::Bool(b.value())),
        _ => Err(syn::Error::new_spanned(lit, "Unsupported literal type")),
    }
}

/// Parse an if expression
fn parse_if_expression(input: ParseStream) -> Result<Expression> {
    input.parse::<token::If>()?;

    let condition = Box::new(match parse_expression(input) {
        Ok(expr) => expr,
        Err(_) => try_parse_as_raw_rust(input)?,
    });

    let then_branch = Box::new(match parse_expression(input) {
        Ok(expr) => expr,
        Err(_) => try_parse_as_raw_rust(input)?,
    });

    let else_branch = if input.peek(token::Else) {
        input.parse::<token::Else>()?;
        Some(Box::new(match parse_expression(input) {
            Ok(expr) => expr,
            Err(_) => try_parse_as_raw_rust(input)?,
        }))
    } else {
        None
    };

    Ok(Expression::If {
        condition,
        then_branch,
        else_branch,
    })
}

/// Parse a match expression
fn parse_match_expression(input: ParseStream) -> Result<Expression> {
    input.parse::<token::Match>()?;
    let expr = Box::new(parse_expression(input)?);

    let content;
    braced!(content in input);

    let mut arms = Vec::new();
    while !content.is_empty() {
        let pattern = parse_pattern(&content)?;
        content.parse::<token::FatArrow>()?;
        let body = parse_expression(&content)?;

        arms.push(MatchArm { pattern, body });

        if content.peek(token::Comma) {
            content.parse::<token::Comma>()?;
        }
    }

    Ok(Expression::Match { expr, arms })
}

/// Parse a closure expression: |param| { ... }
fn parse_closure(input: ParseStream) -> Result<Expression> {
    // Parse opening |
    input.parse::<token::Or>()?;

    // Parse parameters
    let mut params = Vec::new();
    while !input.peek(token::Or) {
        let param: Ident = input.parse()?;
        params.push(param.to_string());

        if input.peek(token::Comma) {
            input.parse::<token::Comma>()?;
        }
    }

    // Parse closing |
    input.parse::<token::Or>()?;

    // Parse body (can be a block or single expression)
    let body = if input.peek(token::Brace) {
        // Block body: |param| { ... }
        Box::new(Expression::Block(parse_block(input)?))
    } else {
        // Expression body: |param| expr
        Box::new(parse_expression(input)?)
    };

    Ok(Expression::Closure { params, body })
}

/// Try to parse an unrecognized pattern as raw Rust code
fn try_parse_as_raw_rust(input: ParseStream) -> Result<Expression> {
    // Collect tokens until we hit a delimiter that indicates end of expression
    let mut tokens = Vec::new();
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;

    while !input.is_empty() {
        // Check for expression terminators at top level only
        if paren_depth == 0
            && bracket_depth == 0
            && brace_depth == 0
            && (input.peek(token::Comma) || input.peek(token::Semi) || input.peek(token::RArrow))
        {
            break;
        }

        // Parse any token and track delimiters
        let token: proc_macro2::TokenTree = input.parse()?;

        // Track opening/closing delimiters and add tokens
        match &token {
            proc_macro2::TokenTree::Group(group) => {
                match group.delimiter() {
                    proc_macro2::Delimiter::Parenthesis => {
                        // This is a complete parenthesized group - no need to track depth
                        tokens.push(format!("({})", group.stream()));
                    }
                    proc_macro2::Delimiter::Bracket => {
                        // This is a complete bracketed group - no need to track depth
                        tokens.push(format!("[{}]", group.stream()));
                    }
                    proc_macro2::Delimiter::Brace => {
                        // This is a complete braced group - no need to track depth
                        tokens.push(format!("{{{}}}", group.stream()));
                    }
                    proc_macro2::Delimiter::None => {
                        tokens.push(group.stream().to_string());
                    }
                }
            }
            proc_macro2::TokenTree::Punct(punct) => {
                let punct_str = punct.to_string();
                tokens.push(punct_str.clone());

                // Track individual delimiter characters (though Groups should handle most cases)
                match punct_str.as_str() {
                    "(" => paren_depth += 1,
                    ")" => paren_depth = paren_depth.saturating_sub(1),
                    "[" => bracket_depth += 1,
                    "]" => bracket_depth = bracket_depth.saturating_sub(1),
                    "{" => brace_depth += 1,
                    "}" => brace_depth = brace_depth.saturating_sub(1),
                    _ => {}
                }
            }
            _ => {
                tokens.push(token.to_string());
            }
        }
    }

    if tokens.is_empty() {
        return Err(input.error("Unexpected end of input"));
    }

    // Combine tokens into raw Rust code
    let raw_code = tokens.join(" ");
    Ok(Expression::RawRust(raw_code))
}

/// Fallback parsing function that wraps complex expressions as raw Rust code
///
/// This function checks if the input starts with a known DSL function or keyword.
/// If so, it attempts to parse with argument-level fallback. Otherwise, it wraps
/// the entire input as raw Rust code.
///
/// # Must use
///
/// The returned `Dsl` structure should be used for code generation
#[must_use]
pub fn parse_dsl_with_fallback(input: &TokenStream) -> Dsl {
    // Check if the input starts with a known DSL function or control flow construct
    let input_str = input.to_string();
    let known_dsl_functions = [
        "navigate",
        "hide",
        "show",
        "log",
        "invoke",
        "throttle",
        "clamp",
        "set_background",
        "set_visibility",
        "on_event",
        "remove_background",
    ];

    let known_dsl_keywords = ["if", "match", "for", "while", "let"];

    let starts_with_dsl_function = known_dsl_functions
        .iter()
        .any(|func| input_str.trim_start().starts_with(func));

    let starts_with_dsl_keyword = known_dsl_keywords
        .iter()
        .any(|keyword| input_str.trim_start().starts_with(keyword));

    if starts_with_dsl_function || starts_with_dsl_keyword {
        // Try to parse individual arguments with fallback
        if let Ok(dsl) = parse_dsl_with_argument_fallback(input.clone()) {
            return dsl;
        }

        // If that fails, fall back to full raw Rust
        let raw_code = input.to_string();
        let fallback_expr = Expression::RawRust(raw_code);
        let statement = Statement::Expression(fallback_expr);
        return Dsl::new(vec![statement]);
    }

    // For non-DSL functions, use raw Rust fallback
    let raw_code = input.to_string();
    let fallback_expr = Expression::RawRust(raw_code);
    let statement = Statement::Expression(fallback_expr);
    Dsl::new(vec![statement])
}

/// Try to parse DSL with argument-level fallback
fn parse_dsl_with_argument_fallback(input: TokenStream) -> Result<Dsl> {
    // This is a simplified approach - we'll enhance the error handling
    // to be more specific about what failed
    Parser::parse2(parse_dsl, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test_log::test]
    fn test_parse_pattern_literal_integer() {
        let input = quote! { 42 };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        match result {
            Pattern::Literal(Literal::Integer(n)) => assert_eq!(n, 42),
            _ => panic!("Expected integer literal pattern"),
        }
    }

    #[test_log::test]
    fn test_parse_pattern_literal_string() {
        let input = quote! { "test" };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        match result {
            Pattern::Literal(Literal::String(s)) => assert_eq!(s, "test"),
            _ => panic!("Expected string literal pattern"),
        }
    }

    #[test_log::test]
    fn test_parse_pattern_variable() {
        let input = quote! { x };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        match result {
            Pattern::Variable(name) => assert_eq!(name, "x"),
            _ => panic!("Expected variable pattern"),
        }
    }

    #[test_log::test]
    fn test_parse_pattern_wildcard() {
        let input = quote! { _ };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        assert!(matches!(result, Pattern::Wildcard));
    }

    #[test_log::test]
    fn test_parse_pattern_enum_variant() {
        let input = quote! { Option::Some };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        match result {
            Pattern::Variant {
                enum_name,
                variant,
                fields,
            } => {
                assert_eq!(enum_name, "Option");
                assert_eq!(variant, "Some");
                assert!(fields.is_empty());
            }
            _ => panic!("Expected enum variant pattern"),
        }
    }

    #[test_log::test]
    fn test_parse_literal_bool_true() {
        let lit = syn::parse_quote! { true };
        let result = parse_literal(lit).unwrap();
        assert!(matches!(result, Literal::Bool(true)));
    }

    #[test_log::test]
    fn test_parse_literal_bool_false() {
        let lit = syn::parse_quote! { false };
        let result = parse_literal(lit).unwrap();
        assert!(matches!(result, Literal::Bool(false)));
    }

    #[test_log::test]
    fn test_parse_literal_float() {
        let lit = syn::parse_quote! { 3.14 };
        let result = parse_literal(lit).unwrap();
        match result {
            Literal::Float(f) => assert!((f - 3.14).abs() < f64::EPSILON),
            _ => panic!("Expected float literal"),
        }
    }

    #[test_log::test]
    fn test_parse_closure_single_param() {
        let input = quote! { |x| x };
        let result = Parser::parse2(parse_closure, input).unwrap();
        match result {
            Expression::Closure { params, body: _ } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], "x");
            }
            _ => panic!("Expected closure expression"),
        }
    }

    #[test_log::test]
    fn test_parse_closure_multiple_params() {
        let input = quote! { |x, y| x };
        let result = Parser::parse2(parse_closure, input).unwrap();
        match result {
            Expression::Closure { params, body: _ } => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0], "x");
                assert_eq!(params[1], "y");
            }
            _ => panic!("Expected closure expression"),
        }
    }

    #[test_log::test]
    fn test_parse_closure_no_params() {
        let input = quote! { || 42 };
        let result = Parser::parse2(parse_closure, input).unwrap();
        match result {
            Expression::Closure { params, body: _ } => {
                assert!(params.is_empty());
            }
            _ => panic!("Expected closure expression"),
        }
    }

    #[test_log::test]
    fn test_parse_unary_not() {
        let input = quote! { !true };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Unary { op, expr: _ } => {
                assert!(matches!(op, UnaryOp::Not));
            }
            _ => panic!("Expected unary not expression"),
        }
    }

    #[test_log::test]
    fn test_parse_unary_minus() {
        let input = quote! { -42 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Unary { op, expr: _ } => {
                assert!(matches!(op, UnaryOp::Minus));
            }
            _ => panic!("Expected unary minus expression"),
        }
    }

    #[test_log::test]
    fn test_parse_unary_reference() {
        let input = quote! { &value };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Unary { op, expr: _ } => {
                assert!(matches!(op, UnaryOp::Ref));
            }
            _ => panic!("Expected unary reference expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_add() {
        let input = quote! { 1 + 2 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Add));
            }
            _ => panic!("Expected binary add expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_multiply() {
        let input = quote! { 3 * 4 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Multiply));
            }
            _ => panic!("Expected binary multiply expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_equal() {
        let input = quote! { x == y };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Equal));
            }
            _ => panic!("Expected binary equal expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_and() {
        let input = quote! { true && false };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::And));
            }
            _ => panic!("Expected binary and expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_or() {
        let input = quote! { true || false };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Or));
            }
            _ => panic!("Expected binary or expression"),
        }
    }

    #[test_log::test]
    fn test_parse_grouping_preserves_expression() {
        let input = quote! { (a + b) };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Grouping(inner) => match *inner {
                Expression::Binary {
                    left: _,
                    op,
                    right: _,
                } => {
                    assert!(matches!(op, BinaryOp::Add));
                }
                _ => panic!("Expected binary expression inside grouping"),
            },
            _ => panic!("Expected grouping expression"),
        }
    }

    #[test_log::test]
    fn test_parse_empty_array() {
        let input = quote! { [] };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Array(exprs) => {
                assert!(exprs.is_empty());
            }
            _ => panic!("Expected array expression"),
        }
    }

    #[test_log::test]
    fn test_parse_array_with_elements() {
        let input = quote! { [1, 2, 3] };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Array(exprs) => {
                assert_eq!(exprs.len(), 3);
            }
            _ => panic!("Expected array expression"),
        }
    }

    #[test_log::test]
    fn test_parse_enum_variant_simple() {
        let input = quote! { Key::Escape };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Variable(name) => {
                assert_eq!(name, "Key::Escape");
            }
            _ => panic!("Expected enum variant as variable"),
        }
    }

    #[test_log::test]
    fn test_parse_struct_variant_with_fields() {
        let input = quote! { Action::Update { id: 123, name: "test" } };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Call { function, args } => {
                assert_eq!(function, "Action::Update");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected struct variant as call"),
        }
    }

    #[test_log::test]
    fn test_parse_let_statement() {
        let input = quote! { let x = 42; };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::Let { name, value: _ } => {
                assert_eq!(name, "x");
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test_log::test]
    fn test_parse_if_statement_with_else() {
        let input = quote! {
            if true {
                show("modal");
            } else {
                hide("modal");
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::If {
                condition: _,
                then_block,
                else_block,
            } => {
                assert!(!then_block.statements.is_empty());
                assert!(else_block.is_some());
            }
            _ => panic!("Expected if statement"),
        }
    }

    #[test_log::test]
    fn test_parse_if_statement_without_else() {
        let input = quote! {
            if true {
                show("modal");
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::If {
                condition: _,
                then_block,
                else_block,
            } => {
                assert!(!then_block.statements.is_empty());
                assert!(else_block.is_none());
            }
            _ => panic!("Expected if statement"),
        }
    }

    #[test_log::test]
    fn test_parse_for_statement() {
        let input = quote! {
            for item in items {
                log(item);
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::For {
                pattern,
                iter: _,
                body,
            } => {
                assert_eq!(pattern, "item");
                assert!(!body.statements.is_empty());
            }
            _ => panic!("Expected for statement"),
        }
    }

    #[test_log::test]
    fn test_parse_while_statement() {
        let input = quote! {
            while true {
                log("test");
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::While { condition: _, body } => {
                assert!(!body.statements.is_empty());
            }
            _ => panic!("Expected while statement"),
        }
    }

    #[test_log::test]
    fn test_parse_empty_dsl() {
        let input = quote! {};
        let result = parse_dsl.parse2(input).unwrap();
        assert!(result.statements.is_empty());
    }

    #[test_log::test]
    fn test_parse_multiple_statements() {
        let input = quote! {
            let x = 1;
            let y = 2;
            show("test");
        };
        let result = parse_dsl.parse2(input).unwrap();
        assert_eq!(result.statements.len(), 3);
    }

    #[test_log::test]
    fn test_parse_nested_blocks() {
        let input = quote! {
            {
                {
                    show("test");
                }
            }
        };
        let result = parse_dsl.parse2(input).unwrap();
        assert_eq!(result.statements.len(), 1);
        match &result.statements[0] {
            Statement::Block(block) => {
                assert_eq!(block.statements.len(), 1);
            }
            _ => panic!("Expected block statement"),
        }
    }

    #[test_log::test]
    fn test_parse_operator_precedence_add_multiply() {
        let input = quote! { 1 + 2 * 3 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        // Should parse as 1 + (2 * 3) due to operator precedence
        match result {
            Expression::Binary {
                left: _,
                op: BinaryOp::Add,
                right,
            } => match *right {
                Expression::Binary {
                    left: _,
                    op: BinaryOp::Multiply,
                    right: _,
                } => {} // Correct precedence
                _ => panic!("Incorrect operator precedence"),
            },
            _ => panic!("Expected binary expression"),
        }
    }

    #[test_log::test]
    fn test_parse_comparison_less_than() {
        let input = quote! { a < b };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Less));
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test_log::test]
    fn test_parse_comparison_greater_than() {
        let input = quote! { a > b };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Greater));
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test_log::test]
    fn test_parse_comparison_not_equal() {
        let input = quote! { a != b };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::NotEqual));
            }
            _ => panic!("Expected binary expression"),
        }
    }

    #[test_log::test]
    fn test_parse_method_call() {
        let input = quote! { value.to_string() };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::MethodCall {
                receiver: _,
                method,
                args,
            } => {
                assert_eq!(method, "to_string");
                assert!(args.is_empty());
            }
            _ => panic!("Expected method call expression"),
        }
    }

    #[test_log::test]
    fn test_parse_method_call_with_args() {
        let input = quote! { value.clamp(0, 100) };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::MethodCall {
                receiver: _,
                method,
                args,
            } => {
                assert_eq!(method, "clamp");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected method call expression"),
        }
    }

    #[test_log::test]
    fn test_parse_field_access() {
        let input = quote! { obj.field };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Field { object: _, field } => {
                assert_eq!(field, "field");
            }
            _ => panic!("Expected field access expression"),
        }
    }

    #[test_log::test]
    fn test_parse_dsl_with_fallback_for_unknown_function() {
        let input = quote! { unknown_function(1, 2, 3) };
        let result = parse_dsl_with_fallback(&input);
        assert_eq!(result.statements.len(), 1);
        match &result.statements[0] {
            Statement::Expression(Expression::RawRust(_)) => {} // Should fall back to raw Rust
            _ => panic!("Expected fallback to raw Rust"),
        }
    }

    #[test_log::test]
    fn test_parse_element_function() {
        let input = quote! { element(".selector") };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::ElementRef(selector) => match *selector {
                Expression::Literal(Literal::String(s)) => {
                    assert_eq!(s, ".selector");
                }
                _ => panic!("Expected string literal in element ref"),
            },
            _ => panic!("Expected element ref expression"),
        }
    }
}
