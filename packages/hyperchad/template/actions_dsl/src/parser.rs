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

/// Parse the entire DSL input
pub fn parse_dsl(input: ParseStream) -> Result<Dsl> {
    let mut statements = Vec::new();

    while !input.is_empty() {
        let stmt = parse_statement(input)?;
        statements.push(stmt);
    }

    Ok(Dsl::new(statements))
}

/// Parse a single statement
fn parse_statement(input: ParseStream) -> Result<Statement> {
    // Try to parse the statement normally first
    try_parse_statement_normal(input).map_or_else(|_| try_parse_statement_as_raw_rust(input), Ok)
}

/// Try to parse a statement using normal DSL parsing
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

/// Try to parse a statement as raw Rust code
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
                    Expression::ElementRef(hyperchad_actions::dsl::ElementReference {
                        selector: args[0].to_string(),
                    })
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

    if lookahead.peek(syn::LitStr)
        || lookahead.peek(syn::LitInt)
        || lookahead.peek(syn::LitFloat)
        || lookahead.peek(syn::LitBool)
    {
        let lit: Lit = input.parse()?;
        Ok(Expression::Literal(parse_literal(lit)?))
    } else if lookahead.peek(Ident) {
        let ident: Ident = input.parse()?;
        let ident_str = ident.to_string();

        // Check for enum variants (Type::Variant)
        if input.peek(token::PathSep) {
            input.parse::<token::PathSep>()?;

            if input.peek(Ident) || (input.peek(token::Brace) || input.peek(token::Paren)) {
                let variant: Ident = if input.peek(Ident) {
                    input.parse()?
                } else {
                    return Err(input.error("Expected enum variant name"));
                };

                // Check for struct-like variant with fields { field: value }
                if input.peek(token::Brace) {
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

                    // For now, represent struct variants as method calls with named arguments
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

                // Simple enum variant
                return Ok(Expression::Call {
                    function: format!("{ident_str}::{variant}"),
                    args: vec![],
                });
            }
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
/// but tries to preserve DSL function calls
pub fn parse_dsl_with_fallback(input: &TokenStream) -> Dsl {
    // Check if the input starts with a known DSL function
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

    let starts_with_dsl_function = known_dsl_functions
        .iter()
        .any(|func| input_str.trim_start().starts_with(func));

    if starts_with_dsl_function {
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
