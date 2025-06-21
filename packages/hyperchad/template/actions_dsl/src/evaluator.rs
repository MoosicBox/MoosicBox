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

            // All expressions need to be convertible to ActionType for the macro to work
            // Non-action expressions are wrapped in NoOp to maintain type compatibility
            if expression_produces_action(expr) {
                Ok(quote! {
                    let _action_result = #expr_code;
                    #output_var.push(_action_result.into());
                })
            } else {
                // Non-action expressions are evaluated but wrapped as NoOp to maintain type compatibility
                Ok(quote! {
                    let _expr_result = #expr_code;
                    // Evaluate the expression but don't produce an action
                    #output_var.push(hyperchad_actions::ActionType::NoOp.into());
                })
            }
        }
        Statement::Let { name, value } => {
            let var_name = format_ident!("{}", name);
            let value_code = generate_expression_code(value)?;
            Ok(quote! {
                let #var_name = #value_code;
                // Note: Let statements don't produce ActionEffects, they just create variables
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
        Expression::ElementRef(element_ref) => {
            let selector = &element_ref.selector;
            Ok(quote! {
                hyperchad_actions::dsl::ElementReference {
                    selector: #selector.to_string()
                }
            })
        }
        Expression::Call { function, args } => generate_function_call_code(function, args),
        Expression::MethodCall {
            receiver,
            method,
            args,
        } => {
            let receiver_code = generate_expression_code(receiver)?;
            generate_method_call_code(&receiver_code, method, args)
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
                // Convert arithmetic operations to method calls for hyperchad actions
                BinaryOp::Add => Ok(quote! {
                    #left_code.plus(#right_code)
                }),
                BinaryOp::Subtract => Ok(quote! {
                    #left_code.minus(#right_code)
                }),
                BinaryOp::Multiply => Ok(quote! {
                    #left_code.multiply(#right_code)
                }),
                BinaryOp::Divide => Ok(quote! {
                    #left_code.divide(#right_code)
                }),
                BinaryOp::Min => Ok(quote! {
                    #left_code.min(#right_code)
                }),
                BinaryOp::Max => Ok(quote! {
                    #left_code.max(#right_code)
                }),
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
        Expression::Block(block) => {
            // Handle block expressions by generating the block code
            // and returning the result as a compound action
            let output_var = format_ident!("__block_output");
            let block_code = generate_block_code(block, &output_var)?;

            Ok(quote! {
                {
                    let mut #output_var: Vec<hyperchad_actions::ActionEffect> = Vec::new();
                    #block_code
                    hyperchad_actions::ActionType::Multi(
                        #output_var.into_iter().map(|effect| effect.action).collect()
                    )
                }
            })
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
        Expression::Closure { params, body } => {
            // Generate a closure that captures the closure parameters
            let param_idents: Vec<_> = params.iter().map(|p| format_ident!("{}", p)).collect();
            let body_code = generate_expression_code(body)?;

            Ok(quote! {
                |#(#param_idents),*| #body_code
            })
        }
        Expression::RawRust(code) => {
            // Parse the raw Rust code as tokens and insert directly
            let tokens: TokenStream = match code.parse() {
                Ok(tokens) => tokens,
                Err(e) => {
                    return Err(format!("Failed to parse raw Rust code: {e}"));
                }
            };
            Ok(tokens)
        }
    }
}

/// Generate code for function calls, mapping DSL functions to hyperchad actions
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn generate_function_call_code(function: &str, args: &[Expression]) -> Result<TokenStream, String> {
    let args_code: Result<Vec<_>, String> = args.iter().map(generate_expression_code).collect();
    let args_code = args_code?;

    match function {
        // Element reference function
        "element" => {
            if args_code.len() != 1 {
                return Err("element() expects exactly 1 argument".to_string());
            }
            let selector = &args_code[0];
            Ok(quote! {
                hyperchad_actions::dsl::ElementReference {
                    selector: #selector.to_string()
                }
            })
        }

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
        "set_background_self" => {
            if args_code.len() != 1 {
                return Err("set_background() expects exactly 1 argument".to_string());
            }
            let background = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::set_background_self(#background.to_string())
            })
        }

        // Visibility functions
        "set_visibility_child_class" => {
            if args_code.len() != 2 {
                return Err("set_visibility_child_class() expects exactly 2 arguments".to_string());
            }
            let visibility = &args_code[0];
            let target = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionType::set_visibility_child_class(#visibility, &#target.to_string())
            })
        }

        // Display functions
        "display_str_id" => {
            if args_code.len() != 1 {
                return Err("display_str_id() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::display_str_id(&#target.to_string())
            })
        }
        "no_display_str_id" => {
            if args_code.len() != 1 {
                return Err("no_display_str_id() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::no_display_str_id(&#target.to_string())
            })
        }
        "display_class" => {
            if args_code.len() != 1 {
                return Err("display_class() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::display_class(&#target.to_string())
            })
        }
        "no_display_class" => {
            if args_code.len() != 1 {
                return Err("no_display_class() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::no_display_class(&#target.to_string())
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
        "get_visibility_str_id" => {
            if args_code.len() != 1 {
                return Err("get_visibility_str_id() expects exactly 1 argument".to_string());
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
        "get_width_px_self" => {
            if !args_code.is_empty() {
                return Err("get_width_px_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_width_px_self()
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
        "get_height_px_str_id" => {
            if args_code.len() != 1 {
                return Err("get_height_px_str_id() expects exactly 1 argument".to_string());
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
        "get_mouse_x_self" => {
            if !args_code.is_empty() {
                return Err("get_mouse_x_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_mouse_x_self()
            })
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
        "get_mouse_y_str_id" => {
            if args_code.len() != 1 {
                return Err("get_mouse_y_str_id() expects exactly 1 argument".to_string());
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::logic::get_mouse_y_str_id(#target.to_string())
            })
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

        // Action invocation function
        "invoke" => {
            if args_code.len() != 2 {
                return Err("invoke() expects exactly 2 arguments (action, value)".to_string());
            }

            // Check if the first argument is a struct variant call
            let action_code = if let Expression::Call {
                function,
                args: call_args,
            } = &args[0]
            {
                // Check if this looks like a struct variant (contains :: and has tuple args)
                if function.contains("::") && !call_args.is_empty() {
                    let is_struct_variant = call_args.iter().all(|arg| {
                        matches!(arg, Expression::Tuple(tuple_args) if tuple_args.len() == 2 &&
                            matches!(tuple_args[0], Expression::Literal(Literal::String(_))))
                    });

                    if is_struct_variant {
                        // Generate struct syntax: Enum::Variant { field1: value1, field2: value2 }
                        let struct_path = function;
                        let struct_path_tokens: TokenStream = struct_path.parse().map_err(|e| {
                            format!("Failed to parse struct path '{struct_path}': {e}")
                        })?;

                        let mut field_assignments = Vec::new();
                        for arg in call_args {
                            if let Expression::Tuple(tuple_args) = arg {
                                if let (
                                    Expression::Literal(Literal::String(field_name)),
                                    field_value,
                                ) = (&tuple_args[0], &tuple_args[1])
                                {
                                    let field_ident = format_ident!("{}", field_name);
                                    let field_value_code = generate_expression_code(field_value)?;
                                    field_assignments
                                        .push(quote! { #field_ident: #field_value_code });
                                }
                            }
                        }

                        quote! { #struct_path_tokens { #(#field_assignments),* } }
                    } else {
                        args_code[0].clone()
                    }
                } else {
                    args_code[0].clone()
                }
            } else {
                args_code[0].clone()
            };

            let value = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionType::Parameterized {
                    action: Box::new(hyperchad_actions::ActionType::Custom {
                        action: serde_json::to_string(&#action_code).unwrap_or_else(|_| #action_code.to_string()),
                    }),
                    value: hyperchad_actions::logic::Value::from(#value),
                }
            })
        }

        // Throttle function: throttle(duration, action)
        "throttle" => {
            if args_code.len() != 2 {
                return Err("throttle() expects exactly 2 arguments (duration, action)".to_string());
            }
            let duration = &args_code[0];
            let action = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionEffect {
                    action: #action,
                    throttle: Some(#duration as u64),
                    delay_off: None,
                    unique: None,
                }
            })
        }

        // Delay off function: delay_off(duration, action)
        "delay_off" => {
            if args_code.len() != 2 {
                return Err(
                    "delay_off() expects exactly 2 arguments (duration, action)".to_string()
                );
            }
            let duration = &args_code[0];
            let action = &args_code[1];
            Ok(quote! {
                hyperchad_actions::ActionEffect {
                    action: #action,
                    throttle: None,
                    delay_off: Some(#duration as u64),
                    unique: None,
                }
            })
        }

        // Unique function: unique(action)
        "unique" => {
            if args_code.len() != 1 {
                return Err("unique() expects exactly 1 argument (action)".to_string());
            }
            let action = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionEffect {
                    action: #action,
                    throttle: None,
                    delay_off: None,
                    unique: Some(true),
                }
            })
        }

        // Clamp function: clamp(min, value, max)
        "clamp" => {
            if args_code.len() != 3 {
                return Err("clamp() expects exactly 3 arguments (min, value, max)".to_string());
            }
            let min = &args_code[0];
            let value = &args_code[1];
            let max = &args_code[2];
            Ok(quote! {
                #value.clamp(#min, #max)
            })
        }

        // Group function for arithmetic grouping: group(expression)
        "group" => {
            if args_code.len() != 1 {
                return Err("group() expects exactly 1 argument".to_string());
            }
            let expr = &args_code[0];
            Ok(quote! {
                hyperchad_actions::logic::Arithmetic::Grouping(Box::new(#expr))
            })
        }

        // Event handling function: on_event(event_name, closure)
        "on_event" => {
            if args_code.len() != 2 {
                return Err(
                    "on_event() expects exactly 2 arguments (event_name, closure)".to_string(),
                );
            }
            let event_name = &args_code[0];
            let closure_expr = &args[1]; // Use the original expression, not the generated code

            // Check if the second argument is a closure
            if let Expression::Closure { params, body } = closure_expr {
                // Transform the closure into hyperchad logic
                // Replace the closure parameter with get_event_value() calls
                let transformed_body = transform_closure_for_event(params, body)?;
                Ok(quote! {
                    hyperchad_actions::ActionType::Event {
                        name: #event_name.to_string(),
                        action: Box::new(#transformed_body),
                    }
                })
            } else {
                // If it's not a closure, just use the regular action
                let action = &args_code[1];
                Ok(quote! {
                    hyperchad_actions::ActionType::Event {
                        name: #event_name.to_string(),
                        action: Box::new(#action),
                    }
                })
            }
        }

        // ActionType struct constructors
        name if name.starts_with("hyperchad_actions::ActionType::") => {
            let variant = name
                .strip_prefix("hyperchad_actions::ActionType::")
                .unwrap();
            match variant {
                "Navigate" => {
                    // Handle hyperchad_actions::ActionType::Navigate { url: "..." }
                    if args_code.len() == 1 {
                        // Expecting a tuple with field name and value
                        let url_arg = &args_code[0];
                        Ok(quote! {
                            hyperchad_actions::ActionType::Navigate {
                                url: #url_arg.to_string()
                            }
                        })
                    } else {
                        Err(
                            "hyperchad_actions::ActionType::Navigate expects exactly 1 field (url)"
                                .to_string(),
                        )
                    }
                }
                "hide_str_id" => {
                    if args_code.len() != 1 {
                        return Err(
                            "hyperchad_actions::ActionType::hide_str_id() expects exactly 1 argument".to_string()
                        );
                    }
                    let target = &args_code[0];
                    Ok(quote! {
                        hyperchad_actions::ActionType::hide_str_id(&#target.to_string())
                    })
                }
                "show_str_id" => {
                    if args_code.len() != 1 {
                        return Err(
                            "hyperchad_actions::ActionType::show_str_id() expects exactly 1 argument".to_string()
                        );
                    }
                    let target = &args_code[0];
                    Ok(quote! {
                        hyperchad_actions::ActionType::show_str_id(&#target.to_string())
                    })
                }
                "show_self" => {
                    if !args_code.is_empty() {
                        return Err(
                            "hyperchad_actions::ActionType::show_self() expects no arguments"
                                .to_string(),
                        );
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::show_self()
                    })
                }
                "hide_self" => {
                    if !args_code.is_empty() {
                        return Err("hide_self() expects no arguments".to_string());
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::hide_self()
                    })
                }
                "remove_background_self" => {
                    if !args_code.is_empty() {
                        return Err("remove_background_self() expects no arguments".to_string());
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_self()
                    })
                }
                "remove_background_str_id" => {
                    if args_code.len() != 1 {
                        return Err(
                            "remove_background_str_id() expects exactly 1 argument (target)"
                                .to_string(),
                        );
                    }
                    let target = &args_code[0];
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_str_id(#target)
                    })
                }
                "remove_background_id" => {
                    if args_code.len() != 1 {
                        return Err("remove_background_id() expects exactly 1 argument (target)"
                            .to_string());
                    }
                    let target = &args_code[0];
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_id(#target)
                    })
                }
                "remove_background_class" => {
                    if args_code.len() != 1 {
                        return Err(
                            "remove_background_class() expects exactly 1 argument (class_name)"
                                .to_string(),
                        );
                    }
                    let class_name = &args_code[0];
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_class(#class_name)
                    })
                }
                "remove_background_child_class" => {
                    if args_code.len() != 1 {
                        return Err("remove_background_child_class() expects exactly 1 argument (class_name)".to_string());
                    }
                    let class_name = &args_code[0];
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_child_class(#class_name)
                    })
                }
                "remove_background_last_child" => {
                    if !args_code.is_empty() {
                        return Err(
                            "remove_background_last_child() expects no arguments".to_string()
                        );
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_last_child()
                    })
                }
                _ => {
                    let variant_ident = format_ident!("{}", variant);
                    Ok(quote! {
                        hyperchad_actions::ActionType::#variant_ident
                    })
                }
            }
        }

        // Visibility enum variants
        name if name.starts_with("Visibility::") => {
            let variant = name.strip_prefix("Visibility::").unwrap();
            let variant_ident = format_ident!("{}", variant);
            Ok(quote! {
                hyperchad_transformer_models::Visibility::#variant_ident
            })
        }

        // Action enum variants
        name if name.starts_with("Action::") => {
            let variant = name.strip_prefix("Action::").unwrap();
            match variant {
                "SetVolume" | "SeekCurrentTrackPercent" => {
                    // These expect a value to be passed via then_pass_to
                    let variant_ident = format_ident!("{}", variant);
                    Ok(quote! {
                        Action::#variant_ident
                    })
                }
                "PlayAlbum" | "AddAlbumToQueue" | "PlayAlbumStartingAtTrackId" => {
                    // These are complex variants with fields - for now, just return the variant
                    let variant_ident = format_ident!("{}", variant);
                    if args_code.is_empty() {
                        // Simple variant without arguments
                        Ok(quote! {
                            Action::#variant_ident
                        })
                    } else {
                        // For now, assume arguments are provided as a single struct-like expression
                        Ok(quote! {
                            Action::#variant_ident { #(#args_code),* }
                        })
                    }
                }
                _ => {
                    let variant_ident = format_ident!("{}", variant);
                    Ok(quote! {
                        Action::#variant_ident
                    })
                }
            }
        }

        // Self-targeting functions
        "show_self" => {
            if !args_code.is_empty() {
                return Err("show_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::show_self()
            })
        }
        "hide_self" => {
            if !args_code.is_empty() {
                return Err("hide_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::hide_self()
            })
        }
        "remove_background_self" => {
            if !args_code.is_empty() {
                return Err("remove_background_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_self()
            })
        }
        "remove_background_str_id" => {
            if args_code.len() != 1 {
                return Err(
                    "remove_background_str_id() expects exactly 1 argument (target)".to_string(),
                );
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_str_id(#target)
            })
        }
        "remove_background_id" => {
            if args_code.len() != 1 {
                return Err(
                    "remove_background_id() expects exactly 1 argument (target)".to_string()
                );
            }
            let target = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_id(#target)
            })
        }
        "remove_background_class" => {
            if args_code.len() != 1 {
                return Err(
                    "remove_background_class() expects exactly 1 argument (class_name)".to_string(),
                );
            }
            let class_name = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_class(#class_name)
            })
        }
        "remove_background_child_class" => {
            if args_code.len() != 1 {
                return Err(
                    "remove_background_child_class() expects exactly 1 argument (class_name)"
                        .to_string(),
                );
            }
            let class_name = &args_code[0];
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_child_class(#class_name)
            })
        }
        "remove_background_last_child" => {
            if !args_code.is_empty() {
                return Err("remove_background_last_child() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_last_child()
            })
        }
        "show_last_child" => {
            if !args_code.is_empty() {
                return Err("show_last_child() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::show_last_child()
            })
        }
        "get_visibility_self" => {
            if !args_code.is_empty() {
                return Err("get_visibility_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_visibility_self()
            })
        }
        "get_event_value" => {
            if !args_code.is_empty() {
                return Err("get_event_value() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_event_value()
            })
        }

        // Default case - assume it's a variable or unknown function
        _ => {
            // Check if this looks like a struct variant (contains :: and has tuple args)
            if function.contains("::") && !args.is_empty() {
                // Check if arguments are tuples representing struct fields
                let is_struct_variant = args.iter().all(|arg| {
                    matches!(arg, Expression::Tuple(tuple_args) if tuple_args.len() == 2 &&
                        matches!(tuple_args[0], Expression::Literal(Literal::String(_))))
                });

                if is_struct_variant {
                    // Generate struct syntax: Enum::Variant { field1: value1, field2: value2 }
                    let struct_path = function;
                    let struct_path_tokens: TokenStream = struct_path
                        .parse()
                        .map_err(|e| format!("Failed to parse struct path '{struct_path}': {e}"))?;

                    let mut field_assignments = Vec::new();
                    for arg in args {
                        if let Expression::Tuple(tuple_args) = arg {
                            if let (Expression::Literal(Literal::String(field_name)), field_value) =
                                (&tuple_args[0], &tuple_args[1])
                            {
                                let field_ident = format_ident!("{}", field_name);
                                let field_value_code = generate_expression_code(field_value)?;
                                field_assignments.push(quote! { #field_ident: #field_value_code });
                            }
                        }
                    }

                    return Ok(quote! {
                        #struct_path_tokens { #(#field_assignments),* }
                    });
                }
            }

            // Regular function call
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
        UnaryOp::Ref => quote! { & },
    }
}

/// Generate code for method calls
#[allow(clippy::too_many_lines)]
fn generate_method_call_code(
    receiver: &TokenStream,
    method: &str,
    args: &[Expression],
) -> Result<TokenStream, String> {
    let args_code: Result<Vec<_>, String> = args.iter().map(generate_expression_code).collect();
    let args_code = args_code?;

    match method {
        // Element reference methods
        "show" => {
            if !args_code.is_empty() {
                return Err("ElementReference.show() expects no arguments".to_string());
            }
            Ok(quote! {
                {
                    let element_ref = &#receiver;
                    let parsed = element_ref.parse_selector();
                    match parsed {
                        hyperchad_actions::dsl::ParsedSelector::Id(id) => {
                            hyperchad_actions::ActionType::show_str_id(&id)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Class(class) => {
                            hyperchad_actions::ActionType::show_class(&class)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Complex(_) => {
                            // For complex selectors, fall back to string ID for now
                            hyperchad_actions::ActionType::show_str_id(&element_ref.selector)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Invalid => {
                            hyperchad_actions::ActionType::NoOp
                        }
                    }
                }
            })
        }
        "hide" => {
            if !args_code.is_empty() {
                return Err("ElementReference.hide() expects no arguments".to_string());
            }
            Ok(quote! {
                {
                    let element_ref = &#receiver;
                    let parsed = element_ref.parse_selector();
                    match parsed {
                        hyperchad_actions::dsl::ParsedSelector::Id(id) => {
                            hyperchad_actions::ActionType::hide_str_id(&id)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Class(class) => {
                            hyperchad_actions::ActionType::hide_class(&class)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Complex(_) => {
                            // For complex selectors, fall back to string ID for now
                            hyperchad_actions::ActionType::hide_str_id(&element_ref.selector)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Invalid => {
                            hyperchad_actions::ActionType::NoOp
                        }
                    }
                }
            })
        }
        "toggle" => {
            if !args_code.is_empty() {
                return Err("ElementReference.toggle() expects no arguments".to_string());
            }
            Ok(quote! {
                {
                    let element_ref = &#receiver;
                    let parsed = element_ref.parse_selector();
                    match parsed {
                        hyperchad_actions::dsl::ParsedSelector::Id(id) => {
                            // TODO: Implement proper toggle logic based on current visibility
                            hyperchad_actions::ActionType::show_str_id(&id)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Class(class) => {
                            // TODO: Implement proper toggle logic based on current visibility
                            hyperchad_actions::ActionType::show_class(&class)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Complex(_) => {
                            // For complex selectors, fall back to string ID for now
                            hyperchad_actions::ActionType::show_str_id(&element_ref.selector)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Invalid => {
                            hyperchad_actions::ActionType::NoOp
                        }
                    }
                }
            })
        }
        "visibility" => {
            if !args_code.is_empty() {
                return Err("ElementReference.visibility() expects no arguments".to_string());
            }
            Ok(quote! {
                {
                    let element_ref = &#receiver;
                    let parsed = element_ref.parse_selector();
                    match parsed {
                        hyperchad_actions::dsl::ParsedSelector::Id(id) => {
                            hyperchad_actions::logic::get_visibility_str_id(id)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Class(class) => {
                            hyperchad_actions::logic::get_visibility_class(class)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Complex(_) => {
                            // For complex selectors, fall back to string ID for now
                            hyperchad_actions::logic::get_visibility_str_id(element_ref.selector.clone())
                        }
                        hyperchad_actions::dsl::ParsedSelector::Invalid => {
                            hyperchad_actions::logic::get_visibility_self() // Return a CalcValue instead of Value
                        }
                    }
                }
            })
        }
        "set_visibility" => {
            if args_code.len() != 1 {
                return Err(
                    "ElementReference.set_visibility() expects exactly 1 argument".to_string(),
                );
            }
            let visibility = &args_code[0];
            Ok(quote! {
                {
                    let element_ref = &#receiver;
                    let parsed = element_ref.parse_selector();
                    match parsed {
                        hyperchad_actions::dsl::ParsedSelector::Id(id) => {
                            hyperchad_actions::ActionType::set_visibility_str_id(#visibility, &id)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Class(class) => {
                            hyperchad_actions::ActionType::set_visibility_class(#visibility, &class)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Complex(_) => {
                            // For complex selectors, fall back to string ID for now
                            hyperchad_actions::ActionType::set_visibility_str_id(#visibility, &element_ref.selector)
                        }
                        hyperchad_actions::dsl::ParsedSelector::Invalid => {
                            hyperchad_actions::ActionType::NoOp
                        }
                    }
                }
            })
        }

        // Logic chaining methods
        "eq" => {
            if args_code.len() != 1 {
                return Err("eq() expects exactly 1 argument".to_string());
            }
            let value = &args_code[0];
            Ok(quote! {
                #receiver.eq(#value)
            })
        }
        "then" => {
            if args_code.len() != 1 {
                return Err("then() expects exactly 1 argument".to_string());
            }
            let action = &args_code[0];
            Ok(quote! {
                #receiver.then(#action)
            })
        }
        "or_else" => {
            if args_code.len() != 1 {
                return Err("or_else() expects exactly 1 argument".to_string());
            }
            let action = &args_code[0];
            Ok(quote! {
                #receiver.or_else(#action)
            })
        }
        "and" => {
            if args_code.len() != 1 {
                return Err("and() expects exactly 1 argument".to_string());
            }
            let action = &args_code[0];
            Ok(quote! {
                #receiver.and(#action)
            })
        }
        "then_pass_to" => {
            if args_code.len() != 1 {
                return Err("then_pass_to() expects exactly 1 argument".to_string());
            }
            let action = &args_code[0];
            Ok(quote! {
                #receiver.then_pass_to(#action)
            })
        }
        // Math operations
        "divide" => {
            if args_code.len() != 1 {
                return Err("divide() expects exactly 1 argument".to_string());
            }
            let divisor = &args_code[0];
            Ok(quote! {
                #receiver.divide(#divisor)
            })
        }
        "minus" => {
            if args_code.len() != 1 {
                return Err("minus() expects exactly 1 argument".to_string());
            }
            let value = &args_code[0];
            Ok(quote! {
                #receiver.minus(#value)
            })
        }
        "plus" => {
            if args_code.len() != 1 {
                return Err("plus() expects exactly 1 argument".to_string());
            }
            let value = &args_code[0];
            Ok(quote! {
                #receiver.plus(#value)
            })
        }
        "multiply" => {
            if args_code.len() != 1 {
                return Err("multiply() expects exactly 1 argument".to_string());
            }
            let value = &args_code[0];
            Ok(quote! {
                #receiver.multiply(#value)
            })
        }
        "clamp" => {
            if args_code.len() != 2 {
                return Err("clamp() expects exactly 2 arguments (min, max)".to_string());
            }
            let min = &args_code[0];
            let max = &args_code[1];
            Ok(quote! {
                #receiver.clamp(#min, #max)
            })
        }
        // Delay/timing methods
        "delay_off" => {
            if args_code.len() != 1 {
                return Err("delay_off() expects exactly 1 argument".to_string());
            }
            let delay = &args_code[0];
            Ok(quote! {
                #receiver.delay_off(#delay as u64)
            })
        }
        "throttle" => {
            if args_code.len() != 1 {
                return Err("throttle() expects exactly 1 argument".to_string());
            }
            let interval = &args_code[0];
            Ok(quote! {
                #receiver.throttle(#interval as u64)
            })
        }
        // Default case - pass through to the receiver object
        _ => {
            let method_ident = format_ident!("{}", method);
            Ok(quote! {
                #receiver.#method_ident(#(#args_code),*)
            })
        }
    }
}

/// Transform a closure used in `on_event` by replacing parameters with `get_event_value()` calls
fn transform_closure_for_event(
    params: &[String],
    body: &Expression,
) -> Result<TokenStream, String> {
    // For now, assume single parameter named 'value' that should be replaced with get_event_value()
    if params.len() != 1 {
        return Err("on_event closures must have exactly one parameter".to_string());
    }

    let param_name = &params[0];
    let transformed_body = replace_param_with_get_event_value(body, param_name)?;
    generate_expression_code(&transformed_body)
}

/// Replace a parameter in an expression with `get_event_value()` calls
fn replace_param_with_get_event_value(
    expr: &Expression,
    param_name: &str,
) -> Result<Expression, String> {
    match expr {
        Expression::Variable(name) if name == param_name => {
            // Replace the parameter with get_event_value() call
            Ok(Expression::Call {
                function: "get_event_value".to_string(),
                args: vec![],
            })
        }
        Expression::Binary { left, op, right } => {
            let left_transformed = replace_param_with_get_event_value(left, param_name)?;
            let right_transformed = replace_param_with_get_event_value(right, param_name)?;
            Ok(Expression::Binary {
                left: Box::new(left_transformed),
                op: op.clone(),
                right: Box::new(right_transformed),
            })
        }
        Expression::Call { function, args } => {
            let transformed_args: Result<Vec<_>, String> = args
                .iter()
                .map(|arg| replace_param_with_get_event_value(arg, param_name))
                .collect();
            Ok(Expression::Call {
                function: function.clone(),
                args: transformed_args?,
            })
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition_transformed = replace_param_with_get_event_value(condition, param_name)?;
            let then_transformed = replace_param_with_get_event_value(then_branch, param_name)?;
            let else_transformed = if let Some(else_branch) = else_branch {
                Some(Box::new(replace_param_with_get_event_value(
                    else_branch,
                    param_name,
                )?))
            } else {
                None
            };
            Ok(Expression::If {
                condition: Box::new(condition_transformed),
                then_branch: Box::new(then_transformed),
                else_branch: else_transformed,
            })
        }
        Expression::Block(block) => {
            let transformed_statements: Result<Vec<_>, String> = block
                .statements
                .iter()
                .map(|stmt| replace_param_in_statement(stmt, param_name))
                .collect();
            Ok(Expression::Block(Block {
                statements: transformed_statements?,
            }))
        }
        Expression::MethodCall {
            receiver,
            method,
            args,
        } => {
            let receiver_transformed = replace_param_with_get_event_value(receiver, param_name)?;
            let transformed_args: Result<Vec<_>, String> = args
                .iter()
                .map(|arg| replace_param_with_get_event_value(arg, param_name))
                .collect();
            Ok(Expression::MethodCall {
                receiver: Box::new(receiver_transformed),
                method: method.clone(),
                args: transformed_args?,
            })
        }
        Expression::RawRust(code) => {
            // For raw Rust code, we need to do string replacement
            // This is a simple approach - replace the parameter name with get_event_value()
            let transformed_code = code.replace(param_name, "get_event_value()");
            Ok(Expression::RawRust(transformed_code))
        }
        // For other expressions, return as-is (literals, etc.)
        _ => Ok(expr.clone()),
    }
}

/// Replace a parameter in a statement with `get_event_value()` calls
fn replace_param_in_statement(stmt: &Statement, param_name: &str) -> Result<Statement, String> {
    match stmt {
        Statement::Expression(expr) => {
            let transformed_expr = replace_param_with_get_event_value(expr, param_name)?;
            Ok(Statement::Expression(transformed_expr))
        }
        Statement::Let { name, value } => {
            let transformed_value = replace_param_with_get_event_value(value, param_name)?;
            Ok(Statement::Let {
                name: name.clone(),
                value: transformed_value,
            })
        }
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            let condition_transformed = replace_param_with_get_event_value(condition, param_name)?;
            let then_transformed = replace_param_in_block(then_block, param_name)?;
            let else_transformed = if let Some(else_block) = else_block {
                Some(replace_param_in_block(else_block, param_name)?)
            } else {
                None
            };
            Ok(Statement::If {
                condition: condition_transformed,
                then_block: then_transformed,
                else_block: else_transformed,
            })
        }
        Statement::Block(block) => {
            let transformed_block = replace_param_in_block(block, param_name)?;
            Ok(Statement::Block(transformed_block))
        }
        // For other statements, return as-is
        _ => Ok(stmt.clone()),
    }
}

/// Replace a parameter in a block with `get_event_value()` calls
fn replace_param_in_block(block: &Block, param_name: &str) -> Result<Block, String> {
    let transformed_statements: Result<Vec<_>, String> = block
        .statements
        .iter()
        .map(|stmt| replace_param_in_statement(stmt, param_name))
        .collect();
    Ok(Block {
        statements: transformed_statements?,
    })
}

/// Check if an expression produces an `ActionType` (should be pushed to output)
/// vs a value type (should just be evaluated)
fn expression_produces_action(expr: &Expression) -> bool {
    match expr {
        // Method calls on ElementReference that produce actions
        Expression::MethodCall { method, .. } => {
            matches!(
                method.as_str(),
                "show" | "hide" | "toggle" | "set_visibility"
            )
        }
        // Function calls that produce actions
        Expression::Call { function, .. } => {
            matches!(
                function.as_str(),
                "show"
                    | "hide"
                    | "log"
                    | "navigate"
                    | "custom"
                    | "noop"
                    | "show_str_id"
                    | "hide_str_id"
                    | "show_class"
                    | "hide_class"
                    | "set_visibility_str_id"
                    | "set_visibility_class"
                    | "add_class"
                    | "remove_class"
                    | "toggle_class"
                    | "set_style"
                    | "remove_style"
                    | "set_attribute"
                    | "remove_attribute"
            )
        }
        // Binary operations, literals, variables, etc. don't produce actions
        Expression::Binary { .. }
        | Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::ElementRef(_)
        | Expression::Field { .. }
        | Expression::Unary { .. } => false,

        // Conditional expressions might produce actions based on their branches
        Expression::If {
            then_branch,
            else_branch,
            ..
        } => {
            expression_produces_action(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(|e| expression_produces_action(e))
        }

        // Other complex expressions - be conservative and assume they might produce actions
        Expression::Match { .. }
        | Expression::Block(_)
        | Expression::Array(_)
        | Expression::Tuple(_)
        | Expression::Range { .. }
        | Expression::Closure { .. }
        | Expression::RawRust(_) => true,
    }
}
