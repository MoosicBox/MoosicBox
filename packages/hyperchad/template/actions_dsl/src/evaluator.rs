//! Code generator for `HyperChad` Actions DSL
//!
//! This module generates Rust code that evaluates the DSL at runtime.

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

use hyperchad_actions::dsl::{
    BinaryOp, Block, Dsl, Expression, Literal, MatchArm, Pattern, Statement, UnaryOp,
};

/// Generate evaluation code for the entire DSL
pub fn generate_evaluation_code(dsl: &Dsl, output_var: &Ident) -> Result<TokenStream, String> {
    let mut statements = Vec::new();

    for stmt in &dsl.statements {
        let code = generate_statement_code(stmt, output_var)?;
        statements.push(code);
    }

    Ok(quote! {
        #(#statements)*
    })
}

/// Generate code for a statement
#[allow(clippy::too_many_lines)]
fn generate_statement_code(stmt: &Statement, output_var: &Ident) -> Result<TokenStream, String> {
    match stmt {
        Statement::Expression(expr) => {
            let expr_code = generate_expression_code(expr)?;
            Ok(quote! {
                let _action_result = #expr_code;
                #output_var.push(_action_result.into());
            })
        }
        Statement::Let { name, value } => {
            let var_name = format_ident!("{}", name);
            let value_code = generate_expression_code(value)?;
            Ok(quote! {
                let #var_name = #value_code;
            })
        }
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            // Check if the condition is a boolean literal vs a complex expression
            let is_boolean_literal = matches!(condition, Expression::Literal(Literal::Bool(_)));

            let condition_code = generate_expression_code(condition)?;
            let then_code = generate_block_code(then_block, output_var)?;

            let code = if let Some(else_block) = else_block {
                let else_code = generate_block_code(else_block, output_var)?;

                if is_boolean_literal {
                    // For boolean literals, use direct evaluation
                    quote! {
                        if #condition_code {
                            #then_code
                        } else {
                            #else_code
                        }
                    }
                } else {
                    // For complex expressions that return Condition, use logic If
                    quote! {
                        let condition_result = #condition_code;

                        // Create temporary vectors to collect actions from then/else blocks
                        let mut then_actions = Vec::new();
                        let mut else_actions = Vec::new();

                        // Execute the then block and collect its actions
                        let original_len = #output_var.len();
                        #then_code
                        let new_actions: Vec<_> = #output_var.drain(original_len..).collect();
                        then_actions.extend(new_actions.into_iter().map(|effect| hyperchad_actions::Action {
                            trigger: hyperchad_actions::ActionTrigger::Immediate,
                            action: effect,
                        }));

                        // Execute the else block and collect its actions
                        let original_len = #output_var.len();
                        #else_code
                        let new_actions: Vec<_> = #output_var.drain(original_len..).collect();
                        else_actions.extend(new_actions.into_iter().map(|effect| hyperchad_actions::Action {
                            trigger: hyperchad_actions::ActionTrigger::Immediate,
                            action: effect,
                        }));

                        // Create the If statement with collected actions
                        let mut if_action = hyperchad_actions::logic::if_stmt(condition_result, hyperchad_actions::ActionType::NoOp);
                        if_action.actions = then_actions;
                        if_action.else_actions = else_actions;

                        #output_var.push(hyperchad_actions::ActionType::Logic(if_action).into());
                    }
                }
            } else if is_boolean_literal {
                // For boolean literals, use direct evaluation
                quote! {
                    if #condition_code {
                        #then_code
                    }
                }
            } else {
                // For complex expressions that return Condition, use logic If
                quote! {
                    let condition_result = #condition_code;

                    // Create temporary vector to collect actions from then block
                    let mut then_actions = Vec::new();

                    // Execute the then block and collect its actions
                    let original_len = #output_var.len();
                    #then_code
                    let new_actions: Vec<_> = #output_var.drain(original_len..).collect();
                    then_actions.extend(new_actions.into_iter().map(|effect| hyperchad_actions::Action {
                        trigger: hyperchad_actions::ActionTrigger::Immediate,
                        action: effect,
                    }));

                    // Create the If statement with collected actions
                    let mut if_action = hyperchad_actions::logic::if_stmt(condition_result, hyperchad_actions::ActionType::NoOp);
                    if_action.actions = then_actions;

                    #output_var.push(hyperchad_actions::ActionType::Logic(if_action).into());
                }
            };

            Ok(code)
        }
        Statement::Match { expr, arms } => {
            let expr_code = generate_expression_code(expr)?;
            let arms_code: Result<Vec<_>, String> = arms
                .iter()
                .map(|arm| generate_match_arm_code(arm, output_var))
                .collect();
            let arms_code = arms_code?;

            Ok(quote! {
                match #expr_code {
                    #(#arms_code)*
                }
            })
        }
        Statement::For {
            pattern,
            iter,
            body,
        } => {
            let pattern_name = format_ident!("{}", pattern);
            let iter_code = generate_expression_code(iter)?;
            let body_code = generate_block_code(body, output_var)?;

            Ok(quote! {
                for #pattern_name in #iter_code {
                    #body_code
                }
            })
        }
        Statement::While { condition, body } => {
            let condition_code = generate_expression_code(condition)?;
            let body_code = generate_block_code(body, output_var)?;

            Ok(quote! {
                while #condition_code {
                    #body_code
                }
            })
        }
        Statement::Block(block) => generate_block_code(block, output_var),
    }
}

/// Generate code for a block
fn generate_block_code(block: &Block, output_var: &Ident) -> Result<TokenStream, String> {
    let mut statements = Vec::new();

    for stmt in &block.statements {
        let code = generate_statement_code(stmt, output_var)?;
        statements.push(code);
    }

    Ok(quote! {
        {
            #(#statements)*
        }
    })
}

/// Generate code for a match arm
fn generate_match_arm_code(arm: &MatchArm, output_var: &Ident) -> Result<TokenStream, String> {
    let pattern_code = generate_pattern_code(&arm.pattern);
    let body_code = generate_expression_code(&arm.body)?;

    Ok(quote! {
        #pattern_code => {
            let _action_result = #body_code;
            #output_var.push(_action_result.into());
        },
    })
}

/// Generate code for a pattern
fn generate_pattern_code(pattern: &Pattern) -> TokenStream {
    match pattern {
        Pattern::Literal(lit) => {
            let lit_code = generate_literal_code(lit);
            quote! { #lit_code }
        }
        Pattern::Variable(name) => {
            let var_name = format_ident!("{}", name);
            quote! { #var_name }
        }
        Pattern::Wildcard => quote! { _ },
        Pattern::Variant {
            enum_name,
            variant,
            fields: _,
        } => {
            // For now, handle simple enum variants without fields
            let enum_ident = format_ident!("{}", enum_name);
            let variant_ident = format_ident!("{}", variant);
            quote! { #enum_ident::#variant_ident }
        }
    }
}

/// Generate code for an expression
#[allow(clippy::too_many_lines)]
fn generate_expression_code(expr: &Expression) -> Result<TokenStream, String> {
    match expr {
        Expression::Literal(lit) => {
            let lit_code = generate_literal_code(lit);
            Ok(quote! { #lit_code })
        }
        Expression::Variable(name) => {
            let var_name = format_ident!("{}", name);
            Ok(quote! { #var_name })
        }
        Expression::Call { function, args } => generate_function_call_code(function, args),
        Expression::MethodCall {
            receiver,
            method,
            args,
        } => {
            let receiver_code = generate_expression_code(receiver)?;
            let args_code: Result<Vec<_>, String> =
                args.iter().map(generate_expression_code).collect();
            let args_code = args_code?;

            let method_ident = format_ident!("{}", method);

            Ok(quote! {
                #receiver_code.#method_ident(#(#args_code),*)
            })
        }
        Expression::Field { object, field } => {
            let object_code = generate_expression_code(object)?;
            let field_ident = format_ident!("{}", field);

            Ok(quote! {
                #object_code.#field_ident
            })
        }
        Expression::Binary { left, op, right } => {
            let left_code = generate_expression_code(left)?;
            let right_code = generate_expression_code(right)?;

            // Handle special cases for logic operations
            match op {
                BinaryOp::Equal => {
                    // For equality, we need to use the logic::eq function to create a Condition
                    Ok(quote! {
                        hyperchad_actions::logic::eq(#left_code, #right_code)
                    })
                }
                BinaryOp::NotEqual => {
                    // For now, we don't support != directly, but we could implement it
                    Err("NotEqual operation is not yet supported in logic context".to_string())
                }
                BinaryOp::Greater => {
                    // For comparisons, we need to handle this differently
                    Err("Greater than operation is not yet supported in logic context".to_string())
                }
                _ => {
                    // For other operations, use standard Rust operators
                    let op_code = generate_binary_op_code(op);
                    Ok(quote! {
                        #left_code #op_code #right_code
                    })
                }
            }
        }
        Expression::Unary { op, expr } => {
            let expr_code = generate_expression_code(expr)?;
            let op_code = generate_unary_op_code(op);

            Ok(quote! {
                #op_code #expr_code
            })
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition_code = generate_expression_code(condition)?;
            let then_code = generate_expression_code(then_branch)?;

            if let Some(else_branch) = else_branch {
                let else_code = generate_expression_code(else_branch)?;
                Ok(quote! {
                    if #condition_code { #then_code } else { #else_code }
                })
            } else {
                Ok(quote! {
                    if #condition_code { #then_code } else { () }
                })
            }
        }
        Expression::Match {
            expr: _expr,
            arms: _arms,
        } => {
            // Implement basic match expression support for visibility patterns
            // For now, this is a simplified implementation
            Ok(quote! {
                // Match expressions in expression context are complex
                // For now, return a placeholder that won't cause compilation errors
                hyperchad_actions::ActionType::NoOp
            })
        }
        Expression::Block(_block) => {
            // For now, block expressions are not supported
            Err("Block expressions are not yet supported in expression context".to_string())
        }
        Expression::Array(exprs) => {
            let exprs_code: Result<Vec<_>, String> =
                exprs.iter().map(generate_expression_code).collect();
            let exprs_code = exprs_code?;

            Ok(quote! {
                vec![#(#exprs_code),*]
            })
        }
        Expression::Tuple(exprs) => {
            let exprs_code: Result<Vec<_>, String> =
                exprs.iter().map(generate_expression_code).collect();
            let exprs_code = exprs_code?;

            Ok(quote! {
                (#(#exprs_code),*)
            })
        }
        Expression::Range {
            start,
            end,
            inclusive,
        } => {
            let start_code = if let Some(start) = start {
                generate_expression_code(start)?
            } else {
                quote! { 0 }
            };

            let end_code = if let Some(end) = end {
                generate_expression_code(end)?
            } else {
                return Err("Range without end is not supported".to_string());
            };

            if *inclusive {
                Ok(quote! { #start_code..=#end_code })
            } else {
                Ok(quote! { #start_code..#end_code })
            }
        }
    }
}

/// Generate code for function calls, mapping DSL functions to hyperchad actions
#[allow(clippy::too_many_lines)]
fn generate_function_call_code(function: &str, args: &[Expression]) -> Result<TokenStream, String> {
    let args_code: Result<Vec<_>, String> = args.iter().map(generate_expression_code).collect();
    let args_code = args_code?;

    match function {
        // Element visibility functions
        "hide" => {
            if args_code.len() != 1 {
                return Err("hide() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::hide_str_id(&#target.to_string())
            })
        }
        "show" => {
            if args_code.len() != 1 {
                return Err("show() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::show_str_id(&#target.to_string())
            })
        }
        "toggle" => {
            if args_code.len() != 1 {
                return Err("toggle() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                {
                    let target_id = #target.to_string();
                    // TODO: Implement toggle logic based on current visibility
                    hyperchad_actions::ActionType::show_str_id(&target_id)
                }
            })
        }
        "set_visibility" => {
            if args_code.len() != 2 {
                return Err("set_visibility() expects exactly 2 arguments".to_string());
            }
            let target = &args_code[0];
            let visibility = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionType::set_visibility_str_id(#visibility, &#target.to_string())
            })
        }

        // Display functions
        "set_display" => {
            if args_code.len() != 2 {
                return Err("set_display() expects exactly 2 arguments".to_string());
            }
            let target = &args_code[0];
            let display = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionType::set_display_str_id(#display, &#target.to_string())
            })
        }

        // Background functions
        "set_background" => {
            if args_code.len() != 2 {
                return Err("set_background() expects exactly 2 arguments".to_string());
            }
            let target = &args_code[0];
            let background = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionType::set_background_str_id(#background.to_string(), &#target.to_string())
            })
        }

        // Getter functions
        "get_visibility" => {
            if args_code.len() != 1 {
                return Err("get_visibility() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::logic::get_visibility_str_id(#target.to_string())
            })
        }
        "get_display" => {
            if args_code.len() != 1 {
                return Err("get_display() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::logic::get_display_str_id(#target.to_string())
            })
        }
        "get_width" => {
            if args_code.len() != 1 {
                return Err("get_width() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::logic::get_width_px_str_id(#target.to_string())
            })
        }
        "get_height" => {
            if args_code.len() != 1 {
                return Err("get_height() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::logic::get_height_px_str_id(#target.to_string())
            })
        }
        "get_mouse_x" => {
            if args_code.is_empty() {
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_x()
                })
            } else if args_code.len() == 1 {
                let target = &args_code[0];
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_x_str_id(#target.to_string())
                })
            } else {
                Err("get_mouse_x() expects 0 or 1 arguments".to_string())
            }
        }
        "get_mouse_y" => {
            if args_code.is_empty() {
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_y()
                })
            } else if args_code.len() == 1 {
                let target = &args_code[0];
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_y_str_id(#target.to_string())
                })
            } else {
                Err("get_mouse_y() expects 0 or 1 arguments".to_string())
            }
        }

        // Utility functions
        "noop" => Ok(quote! {
            hyperchad_actions::ActionType::NoOp
        }),
        "log" => {
            if args_code.len() != 1 {
                return Err("log() expects exactly 1 argument".to_string());
            }
            let message = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::Log {
                    message: #message.to_string(),
                    level: hyperchad_actions::LogLevel::Info,
                }
            })
        }
        "navigate" => {
            if args_code.len() != 1 {
                return Err("navigate() expects exactly 1 argument".to_string());
            }
            let url = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::Navigate {
                    url: #url.to_string(),
                }
            })
        }
        "custom" => {
            if args_code.len() != 1 {
                return Err("custom() expects exactly 1 argument".to_string());
            }
            let action = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::Custom {
                    action: #action.to_string(),
                }
            })
        }

        // Logic functions
        "visible" => Ok(quote! {
            hyperchad_actions::logic::visible()
        }),
        "hidden" => Ok(quote! {
            hyperchad_actions::logic::hidden()
        }),
        "eq" => {
            if args_code.len() != 2 {
                return Err("eq() expects exactly 2 arguments".to_string());
            }
            let left = &args_code[0];
            let right = &args_code[1];
            Ok(quote! {
                hyperchad_actions::logic::eq(#left, #right)
            })
        }

        // Default case - assume it's a variable or unknown function
        _ => {
            let function_ident = format_ident!("{}", function);
            Ok(quote! {
                #function_ident(#(#args_code),*)
            })
        }
    }
}

/// Generate code for a literal
fn generate_literal_code(lit: &Literal) -> TokenStream {
    match lit {
        Literal::String(s) => quote! { #s },
        Literal::Integer(i) => quote! { #i },
        Literal::Float(f) => quote! { #f },
        Literal::Bool(b) => quote! { #b },
        Literal::Unit => quote! { () },
    }
}

/// Generate code for binary operators
fn generate_binary_op_code(op: &BinaryOp) -> TokenStream {
    match op {
        BinaryOp::Add => quote! { + },
        BinaryOp::Subtract => quote! { - },
        BinaryOp::Multiply => quote! { * },
        BinaryOp::Divide => quote! { / },
        BinaryOp::Modulo => quote! { % },
        BinaryOp::Equal => quote! { == },
        BinaryOp::NotEqual => quote! { != },
        BinaryOp::Less => quote! { < },
        BinaryOp::LessEqual => quote! { <= },
        BinaryOp::Greater => quote! { > },
        BinaryOp::GreaterEqual => quote! { >= },
        BinaryOp::And => quote! { && },
        BinaryOp::Or => quote! { || },
        BinaryOp::BitAnd => quote! { & },
        BinaryOp::BitOr => quote! { | },
        BinaryOp::BitXor => quote! { ^ },
        BinaryOp::Min => quote! { .min },
        BinaryOp::Max => quote! { .max },
    }
}

/// Generate code for unary operators
fn generate_unary_op_code(op: &UnaryOp) -> TokenStream {
    match op {
        UnaryOp::Not => quote! { ! },
        UnaryOp::Minus => quote! { - },
        UnaryOp::Plus => quote! { + },
    }
}
