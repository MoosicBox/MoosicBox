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

/// Parses a let statement: `let name = expression;`
///
/// # Errors
///
/// * Returns error if `let` keyword is missing
/// * Returns error if variable name is invalid
/// * Returns error if `=` is missing
/// * Returns error if expression parsing fails
/// * Returns error if semicolon is missing
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

/// Parses an if statement with optional else/else-if branches
///
/// Supports:
/// * Simple if: `if condition { ... }`
/// * If-else: `if condition { ... } else { ... }`
/// * If-else-if chains: `if condition { ... } else if condition { ... } else { ... }`
///
/// # Errors
///
/// * Returns error if `if` keyword is missing
/// * Returns error if condition expression parsing fails
/// * Returns error if then block parsing fails
/// * Returns error if else block parsing fails (when present)
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

/// Parses a match statement with pattern arms
///
/// Format: `match expr { pattern => body, ... }`
///
/// # Errors
///
/// * Returns error if `match` keyword is missing
/// * Returns error if expression parsing fails
/// * Returns error if braces are missing
/// * Returns error if pattern parsing fails
/// * Returns error if `=>` is missing
/// * Returns error if arm body parsing fails
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

/// Parses a for loop statement
///
/// Format: `for pattern in iter { ... }`
///
/// # Errors
///
/// * Returns error if `for` keyword is missing
/// * Returns error if pattern identifier is invalid
/// * Returns error if `in` keyword is missing
/// * Returns error if iterator expression parsing fails
/// * Returns error if body block parsing fails
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

/// Parses a while loop statement
///
/// Format: `while condition { ... }`
///
/// # Errors
///
/// * Returns error if `while` keyword is missing
/// * Returns error if condition expression parsing fails
/// * Returns error if body block parsing fails
fn parse_while_statement(input: ParseStream) -> Result<Statement> {
    input.parse::<token::While>()?;
    let condition = parse_expression(input)?;
    let body = parse_block(input)?;

    Ok(Statement::While { condition, body })
}

/// Parses a block of statements enclosed in braces
///
/// Format: `{ statement1; statement2; ... }`
///
/// # Errors
///
/// * Returns error if braces are missing
/// * Returns error if any statement parsing fails
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

/// Parses an expression, starting with the lowest-precedence operator (OR)
///
/// This is the entry point for expression parsing, which then delegates to
/// higher-precedence expression parsers.
///
/// # Errors
///
/// * Returns error if the input cannot be parsed as a valid expression
fn parse_expression(input: ParseStream) -> Result<Expression> {
    parse_or_expression(input)
}

/// Parses logical OR expressions (`||`)
///
/// Handles left-associative chaining of OR operators.
///
/// # Errors
///
/// * Returns error if the left or right operand parsing fails
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

/// Parses logical AND expressions (`&&`)
///
/// Handles left-associative chaining of AND operators.
///
/// # Errors
///
/// * Returns error if the left or right operand parsing fails
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

/// Parses equality expressions (`==`, `!=`)
///
/// Handles left-associative chaining of equality operators.
///
/// # Errors
///
/// * Returns error if the left or right operand parsing fails
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

/// Parses comparison expressions (`<`, `<=`, `>`, `>=`)
///
/// Handles left-associative chaining of comparison operators.
///
/// # Errors
///
/// * Returns error if the left or right operand parsing fails
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

/// Parses additive expressions (`+`, `-`)
///
/// Handles left-associative chaining of addition and subtraction operators.
///
/// # Errors
///
/// * Returns error if the left or right operand parsing fails
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

/// Parses multiplicative expressions (`*`, `/`, `%`)
///
/// Handles left-associative chaining of multiplication, division, and modulo operators.
///
/// # Errors
///
/// * Returns error if the left or right operand parsing fails
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

/// Parses unary expressions (`!`, `-`, `+`, `&`)
///
/// Handles prefix unary operators including negation, reference, and logical not.
///
/// # Errors
///
/// * Returns error if the operand expression parsing fails
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

/// Parses postfix expressions (method calls, field access, array indexing)
///
/// Handles chained operations like `obj.method().field[0]`.
///
/// # Errors
///
/// * Returns error if primary expression parsing fails
/// * Returns error if method name or field name is invalid
/// * Returns error if argument list parsing fails
/// * Returns error if array index expression parsing fails
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

/// Parses primary expressions (literals, variables, grouped expressions, etc.)
///
/// Primary expressions are the atomic building blocks of the expression grammar.
/// This includes:
/// * Literals (integers, floats, strings, bools, chars)
/// * Variables and enum variants
/// * Closures
/// * Parenthesized expressions
/// * Array literals
/// * If/match expressions
/// * Block expressions
///
/// # Errors
///
/// * Returns error if the input cannot be parsed as a valid primary expression
/// * Returns error for unrecognized syntax patterns
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

/// Parses a comma-separated list of expressions
///
/// Used for function arguments and array elements.
///
/// # Errors
///
/// * Returns error if any expression in the list fails to parse
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

/// Parses a pattern for use in match arms
///
/// Supports:
/// * Literal patterns (integers, strings, bools)
/// * Variable patterns (identifiers)
/// * Wildcard patterns (`_`)
/// * Enum variant patterns (`Type::Variant`)
///
/// # Errors
///
/// * Returns error if the pattern cannot be recognized
/// * Returns error if enum variant syntax is incomplete
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

/// Parses a literal value into the DSL literal type
///
/// Converts syn literals (strings, integers, floats, bools) into DSL literals.
///
/// # Errors
///
/// * Returns error for unsupported literal types (e.g., byte strings, chars)
fn parse_literal(lit: Lit) -> Result<Literal> {
    match lit {
        Lit::Str(s) => Ok(Literal::String(s.value())),
        Lit::Int(i) => Ok(Literal::Integer(i.base10_parse()?)),
        Lit::Float(f) => Ok(Literal::Float(f.base10_parse()?)),
        Lit::Bool(b) => Ok(Literal::Bool(b.value())),
        _ => Err(syn::Error::new_spanned(lit, "Unsupported literal type")),
    }
}

/// Parses an if expression (not statement) with optional else branch
///
/// Unlike if statements, if expressions return values and can be used
/// where expressions are expected.
///
/// # Errors
///
/// * Returns error if condition parsing fails
/// * Returns error if then branch parsing fails
/// * Returns error if else branch parsing fails (when present)
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

/// Parses a match expression with pattern arms
///
/// Match expressions allow pattern matching against values.
///
/// # Errors
///
/// * Returns error if match expression parsing fails
/// * Returns error if arm pattern or body parsing fails
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

/// Parses a closure expression: `|param| { ... }` or `|param| expr`
///
/// Supports both block-body closures and expression-body closures.
///
/// # Errors
///
/// * Returns error if pipe delimiters are missing
/// * Returns error if parameter parsing fails
/// * Returns error if body parsing fails
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

/// Attempts to parse unrecognized syntax as raw Rust code
///
/// This is a fallback parser that collects tokens until reaching a delimiter
/// (comma, semicolon, or arrow), then wraps them as a `RawRust` expression.
///
/// # Errors
///
/// * Returns error if input is unexpectedly empty
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

/// Fallback parsing function that wraps complex expressions as raw Rust code.
///
/// This function checks if the input starts with a known DSL function or keyword.
/// If so, it attempts to parse with argument-level fallback. Otherwise, it wraps
/// the entire input as raw Rust code.
///
/// The returned `Dsl` structure should be used for code generation.
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

/// Attempts to parse DSL with argument-level fallback
///
/// This tries to parse the input using normal DSL parsing rules before
/// falling back to raw Rust parsing.
///
/// # Errors
///
/// * Returns error if DSL parsing fails
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
        let lit = syn::parse_quote! { 2.5 };
        let result = parse_literal(lit).unwrap();
        match result {
            Literal::Float(f) => assert!((f - 2.5).abs() < f64::EPSILON),
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

    #[test_log::test]
    fn test_parse_literal_char() {
        // Char literals should return an error as unsupported
        let lit: syn::Lit = syn::parse_quote! { 'a' };
        let result = parse_literal(lit);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_parse_closure_with_block_body() {
        let input = quote! { |x| { show(x); } };
        let result = Parser::parse2(parse_closure, input).unwrap();
        match result {
            Expression::Closure { params, body } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], "x");
                match *body {
                    Expression::Block(_) => {} // Expected block body
                    _ => panic!("Expected block body in closure"),
                }
            }
            _ => panic!("Expected closure expression"),
        }
    }

    #[test_log::test]
    fn test_parse_match_statement() {
        let input = quote! {
            match value {
                1 => show("one"),
                2 => show("two"),
                _ => show("other"),
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::Match { expr: _, arms } => {
                assert_eq!(arms.len(), 3);
            }
            _ => panic!("Expected match statement"),
        }
    }

    #[test_log::test]
    fn test_parse_else_if_chain() {
        let input = quote! {
            if a {
                show("a");
            } else if b {
                show("b");
            } else {
                show("c");
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
                // Else block should contain a nested if
                let else_block = else_block.expect("Expected else block");
                assert_eq!(else_block.statements.len(), 1);
                match &else_block.statements[0] {
                    Statement::If { .. } => {} // Expected nested if
                    _ => panic!("Expected nested if in else block"),
                }
            }
            _ => panic!("Expected if statement"),
        }
    }

    #[test_log::test]
    fn test_parse_unit_literal() {
        let input = quote! { () };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Literal(Literal::Unit) => {} // Expected unit literal
            _ => panic!("Expected unit literal expression"),
        }
    }

    #[test_log::test]
    fn test_parse_array_indexing() {
        let input = quote! { arr[0] };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::MethodCall {
                receiver: _,
                method,
                args,
            } => {
                assert_eq!(method, "index");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected method call (index) expression"),
        }
    }

    #[test_log::test]
    fn test_parse_struct_variant_shorthand() {
        // Test struct variant with shorthand syntax (field name equals variable name)
        let input = quote! { Action::Update { id } };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Call { function, args } => {
                assert_eq!(function, "Action::Update");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected struct variant as call"),
        }
    }

    #[test_log::test]
    fn test_parse_dsl_with_fallback_for_dsl_function() {
        // When parsing a known DSL function, it should try DSL parsing
        let input = quote! { show("modal"); };
        let result = parse_dsl_with_fallback(&input);
        assert_eq!(result.statements.len(), 1);
        // Should be parsed as a Call, not RawRust
        match &result.statements[0] {
            Statement::Expression(Expression::Call { function, .. }) => {
                assert_eq!(function, "show");
            }
            _ => panic!("Expected parsed DSL call"),
        }
    }

    #[test_log::test]
    fn test_parse_dsl_with_fallback_for_if_keyword() {
        // When parsing with 'if' keyword, should try DSL parsing
        let input = quote! { if true { show("x"); } };
        let result = parse_dsl_with_fallback(&input);
        assert_eq!(result.statements.len(), 1);
        match &result.statements[0] {
            Statement::If { .. } => {} // Expected if statement
            _ => panic!("Expected if statement"),
        }
    }

    #[test_log::test]
    fn test_parse_block_statement() {
        let input = quote! {
            {
                show("test");
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::Block(block) => {
                assert_eq!(block.statements.len(), 1);
            }
            _ => panic!("Expected block statement"),
        }
    }

    #[test_log::test]
    fn test_parse_expression_statement_without_semicolon() {
        // Expression statements can omit semicolon at end
        let input = quote! { show("test") };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::Expression(Expression::Call { function, .. }) => {
                assert_eq!(function, "show");
            }
            _ => panic!("Expected expression statement"),
        }
    }

    #[test_log::test]
    fn test_parse_match_expression_in_expression_context() {
        let input = quote! {
            match value {
                1 => "one",
                _ => "other",
            }
        };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Match { expr: _, arms } => {
                assert_eq!(arms.len(), 2);
            }
            _ => panic!("Expected match expression"),
        }
    }

    #[test_log::test]
    fn test_parse_if_expression_in_expression_context() {
        let input = quote! { if cond { 1 } else { 2 } };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::If {
                condition: _,
                then_branch: _,
                else_branch,
            } => {
                assert!(else_branch.is_some());
            }
            _ => panic!("Expected if expression"),
        }
    }

    #[test_log::test]
    fn test_parse_chained_method_calls() {
        let input = quote! { value.method1().method2() };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::MethodCall {
                receiver,
                method,
                args: _,
            } => {
                assert_eq!(method, "method2");
                // Receiver should be another method call
                match *receiver {
                    Expression::MethodCall {
                        receiver: _,
                        method: inner_method,
                        args: _,
                    } => {
                        assert_eq!(inner_method, "method1");
                    }
                    _ => panic!("Expected inner method call"),
                }
            }
            _ => panic!("Expected method call expression"),
        }
    }

    #[test_log::test]
    fn test_parse_complex_binary_expression_precedence() {
        // Test that a + b * c + d parses with correct precedence
        let input = quote! { a + b * c + d };
        let result = Parser::parse2(parse_expression, input).unwrap();
        // Should be ((a + (b * c)) + d)
        match result {
            Expression::Binary {
                left,
                op: BinaryOp::Add,
                right: _,
            } => {
                // Left should be a + (b * c)
                match *left {
                    Expression::Binary {
                        left: _,
                        op: BinaryOp::Add,
                        right,
                    } => {
                        // Right of inner add should be b * c
                        match *right {
                            Expression::Binary {
                                left: _,
                                op: BinaryOp::Multiply,
                                right: _,
                            } => {} // Correct precedence
                            _ => panic!("Expected multiply in inner right"),
                        }
                    }
                    _ => panic!("Expected inner add expression"),
                }
            }
            _ => panic!("Expected outer add expression"),
        }
    }

    #[test_log::test]
    fn test_parse_block_expression() {
        let input = quote! { { 1 } };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Block(block) => {
                assert_eq!(block.statements.len(), 1);
            }
            _ => panic!("Expected block expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_subtract() {
        let input = quote! { 10 - 3 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Subtract));
            }
            _ => panic!("Expected binary subtract expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_divide() {
        let input = quote! { 10 / 2 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Divide));
            }
            _ => panic!("Expected binary divide expression"),
        }
    }

    #[test_log::test]
    fn test_parse_binary_modulo() {
        let input = quote! { 10 % 3 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left: _,
                op,
                right: _,
            } => {
                assert!(matches!(op, BinaryOp::Modulo));
            }
            _ => panic!("Expected binary modulo expression"),
        }
    }

    #[test_log::test]
    fn test_parse_unary_plus() {
        let input = quote! { +42 };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Unary { op, expr: _ } => {
                assert!(matches!(op, UnaryOp::Plus));
            }
            _ => panic!("Expected unary plus expression"),
        }
    }

    #[test_log::test]
    fn test_parse_expression_statement_with_semicolon() {
        let input = quote! { show("test"); };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::Expression(Expression::Call { function, .. }) => {
                assert_eq!(function, "show");
            }
            _ => panic!("Expected expression statement"),
        }
    }

    #[test_log::test]
    fn test_parse_function_call_with_multiple_args() {
        let input = quote! { invoke(Action::Test, "value", 123) };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Call { function, args } => {
                assert_eq!(function, "invoke");
                assert_eq!(args.len(), 3);
            }
            _ => panic!("Expected function call expression"),
        }
    }

    #[test_log::test]
    fn test_parse_chained_field_and_method() {
        let input = quote! { obj.field.method() };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::MethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(method, "method");
                assert!(args.is_empty());
                // Receiver should be a field access
                match *receiver {
                    Expression::Field { object: _, field } => {
                        assert_eq!(field, "field");
                    }
                    _ => panic!("Expected field access as receiver"),
                }
            }
            _ => panic!("Expected method call expression"),
        }
    }

    #[test_log::test]
    fn test_parse_nested_parentheses() {
        let input = quote! { ((a + b)) };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Grouping(inner) => match *inner {
                Expression::Grouping(innermost) => match *innermost {
                    Expression::Binary {
                        left: _,
                        op: BinaryOp::Add,
                        right: _,
                    } => {} // Correct nesting
                    _ => panic!("Expected binary add in innermost grouping"),
                },
                _ => panic!("Expected nested grouping"),
            },
            _ => panic!("Expected grouping expression"),
        }
    }

    #[test_log::test]
    fn test_parse_if_expression_without_else() {
        let input = quote! { if cond { 1 } };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::If {
                condition: _,
                then_branch: _,
                else_branch,
            } => {
                assert!(else_branch.is_none());
            }
            _ => panic!("Expected if expression"),
        }
    }

    #[test_log::test]
    fn test_parse_dsl_with_fallback_for_let_keyword() {
        // When parsing with 'let' keyword, should try DSL parsing
        let input = quote! { let x = 42; };
        let result = parse_dsl_with_fallback(&input);
        assert_eq!(result.statements.len(), 1);
        match &result.statements[0] {
            Statement::Let { name, value: _ } => {
                assert_eq!(name, "x");
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test_log::test]
    fn test_parse_logical_and_or_precedence() {
        // OR has lower precedence than AND, so `a && b || c` should parse as `(a && b) || c`
        let input = quote! { a && b || c };
        let result = Parser::parse2(parse_expression, input).unwrap();
        match result {
            Expression::Binary {
                left,
                op: BinaryOp::Or,
                right: _,
            } => {
                // Left side should be the AND expression
                match *left {
                    Expression::Binary {
                        left: _,
                        op: BinaryOp::And,
                        right: _,
                    } => {} // Correct precedence
                    _ => panic!("Expected AND expression on left of OR"),
                }
            }
            _ => panic!("Expected OR expression at top level"),
        }
    }

    #[test_log::test]
    fn test_parse_empty_closure_params() {
        let input = quote! { || show("test") };
        let result = Parser::parse2(parse_closure, input).unwrap();
        match result {
            Expression::Closure { params, body: _ } => {
                assert!(params.is_empty());
            }
            _ => panic!("Expected closure expression"),
        }
    }

    #[test_log::test]
    fn test_parse_match_with_single_arm() {
        let input = quote! {
            match value {
                _ => noop(),
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::Match { expr: _, arms } => {
                assert_eq!(arms.len(), 1);
                assert!(matches!(arms[0].pattern, Pattern::Wildcard));
            }
            _ => panic!("Expected match statement"),
        }
    }

    #[test_log::test]
    fn test_parse_for_with_empty_body() {
        let input = quote! {
            for item in items {
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
                assert!(body.statements.is_empty());
            }
            _ => panic!("Expected for statement"),
        }
    }

    #[test_log::test]
    fn test_parse_while_with_empty_body() {
        let input = quote! {
            while condition {
            }
        };
        let result = Parser::parse2(parse_statement, input).unwrap();
        match result {
            Statement::While { condition: _, body } => {
                assert!(body.statements.is_empty());
            }
            _ => panic!("Expected while statement"),
        }
    }

    #[test_log::test]
    fn test_parse_pattern_literal_bool_true() {
        let input = quote! { true };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        match result {
            Pattern::Literal(Literal::Bool(value)) => assert!(value),
            _ => panic!("Expected bool literal pattern"),
        }
    }

    #[test_log::test]
    fn test_parse_pattern_literal_bool_false() {
        let input = quote! { false };
        let result = Parser::parse2(parse_pattern, input).unwrap();
        match result {
            Pattern::Literal(Literal::Bool(value)) => assert!(!value),
            _ => panic!("Expected bool literal pattern"),
        }
    }

    #[test_log::test]
    fn test_parse_literal_float_decimal() {
        let lit = syn::parse_quote! { 1.234 };
        let result = parse_literal(lit).unwrap();
        match result {
            Literal::Float(f) => assert!((f - 1.234).abs() < f64::EPSILON),
            _ => panic!("Expected float literal"),
        }
    }
}
