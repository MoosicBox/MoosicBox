//! Parser for `HyperChad` Actions DSL
//!
//! This module implements a parser that can handle Rust-like syntax for defining actions.

use syn::{
    Ident, Lit, braced, bracketed, parenthesized,
    parse::{ParseStream, Result},
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
    } else {
        parse_postfix_expression(input)
    }
}

/// Parse postfix expression (method calls, field access, etc.)
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
                let args = parse_expression_list(&content)?;

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
        } else {
            break;
        }
    }

    Ok(expr)
}

/// Parse primary expression
fn parse_primary_expression(input: ParseStream) -> Result<Expression> {
    let lookahead = input.lookahead1();

    if lookahead.peek(Lit) {
        let lit: Lit = input.parse()?;
        Ok(Expression::Literal(parse_literal(lit)?))
    } else if lookahead.peek(Ident) {
        let ident: Ident = input.parse()?;

        if input.peek(token::Paren) {
            // Function call
            let content;
            parenthesized!(content in input);
            let args = parse_expression_list(&content)?;

            Ok(Expression::Call {
                function: ident.to_string(),
                args,
            })
        } else {
            // Variable reference
            Ok(Expression::Variable(ident.to_string()))
        }
    } else if lookahead.peek(token::Paren) {
        // Parenthesized expression or tuple
        let content;
        parenthesized!(content in input);

        if content.is_empty() {
            // Unit literal
            Ok(Expression::Literal(Literal::Unit))
        } else {
            let exprs = parse_expression_list(&content)?;
            if exprs.len() == 1 {
                // Parenthesized expression
                Ok(exprs.into_iter().next().unwrap())
            } else {
                // Tuple
                Ok(Expression::Tuple(exprs))
            }
        }
    } else if lookahead.peek(token::Bracket) {
        // Array literal
        let content;
        bracketed!(content in input);
        let exprs = parse_expression_list(&content)?;
        Ok(Expression::Array(exprs))
    } else if lookahead.peek(token::Brace) {
        // Block expression
        let block = parse_block(input)?;
        Ok(Expression::Block(block))
    } else if lookahead.peek(token::If) {
        // If expression
        input.parse::<token::If>()?;
        let condition = Box::new(parse_expression(input)?);
        let then_branch = Box::new(parse_expression(input)?);
        let else_branch = if input.peek(token::Else) {
            input.parse::<token::Else>()?;
            Some(Box::new(parse_expression(input)?))
        } else {
            None
        };

        Ok(Expression::If {
            condition,
            then_branch,
            else_branch,
        })
    } else if lookahead.peek(token::Match) {
        // Match expression
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
    } else {
        Err(lookahead.error())
    }
}

/// Parse a list of expressions separated by commas
fn parse_expression_list(input: ParseStream) -> Result<Vec<Expression>> {
    let mut exprs = Vec::new();

    while !input.is_empty() {
        exprs.push(parse_expression(input)?);

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
