//! Code generator for `HyperChad` Actions DSL
//!
//! This module generates Rust code that evaluates the DSL at runtime.

use std::collections::VecDeque;

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

use hyperchad_actions::dsl::{
    BinaryOp, Block, Expression, Literal, MatchArm, Pattern, Statement, UnaryOp,
};

/// Variable scope for tracking defined variables during code generation
///
/// Maintains a list of variable names that are currently in scope.
#[derive(Default, Clone, Debug)]
pub struct Scope {
    variables: VecDeque<String>,
}

/// Code generation context that manages variable scopes
///
/// The context maintains a stack of scopes, with the most recent scope at the front.
/// This allows proper handling of nested scopes in control flow structures.
#[derive(Clone, Debug)]
pub struct Context {
    scopes: VecDeque<Scope>,
}

impl Default for Context {
    fn default() -> Self {
        let mut scopes = VecDeque::new();
        scopes.push_back(Scope::default());

        Self { scopes }
    }
}

impl Context {
    /// Adds a variable to the current scope
    ///
    /// The variable is added to the front of the most recent scope.
    pub fn add_variable(&mut self, name: &str) {
        self.scopes
            .front_mut()
            .unwrap()
            .variables
            .push_front(name.to_string());
    }

    /// Checks if a variable is defined in any scope
    ///
    /// Searches through all scopes from most recent to oldest.
    #[must_use]
    pub fn is_variable_defined(&self, name: impl AsRef<str>) -> bool {
        let name = name.as_ref();
        self.scopes
            .iter()
            .any(|scope| scope.variables.iter().any(|x| x == name))
    }

    /// Resolves enum variants generically
    ///
    /// Converts a pattern like `Type::Variant` into appropriate Rust code representation.
    ///
    /// # Must use
    ///
    /// The returned token stream should be used in code generation
    #[must_use]
    fn resolve_enum_variant(enum_type: &str, variant: &str) -> TokenStream {
        let enum_ident = format_ident!("{}", enum_type);
        let variant_ident = format_ident!("{}", variant);

        // Generate the enum variant directly without conversion
        // Let the context determine if conversion to Value is needed
        quote! {
            #enum_ident::#variant_ident
        }
    }
}

macro_rules! push_scope {
    ($context:ident, $($tt:tt)*) => {
        $context.scopes.push_front(Scope::default());
        $($tt)*
        $context.scopes.pop_front();
    };
}

/// Generate code for a statement
#[allow(clippy::too_many_lines)]
fn generate_statement_push(
    context: &mut Context,
    stmt: &Statement,
    output_var: &Ident,
) -> Result<TokenStream, String> {
    match stmt {
        Statement::Expression(expr) => {
            // Special handling for function calls that should become actions
            if let Expression::Call { function, args } = expr {
                match function.as_str() {
                    "show" | "hide" | "show_self" | "hide_self" | "no_display_self"
                    | "display_self" => {
                        // These should be converted to ActionType constructors
                        let action_code = generate_function_call_code(context, function, args)?;
                        return Ok(quote! {
                            #output_var.push((#action_code).into());
                        });
                    }
                    _ => {}
                }
            }

            let expr_code = generate_expression_code(context, expr)?;

            Ok(quote! {
                #output_var.push((#expr_code).into());
            })
        }
        Statement::Let { name, value } => {
            let var_name = format_ident!("{name}");
            let value_code = generate_expression_code(context, value)?;
            context.add_variable(name);
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

            let condition_code = generate_expression_code(context, condition)?;
            let then_code = generate_block_push(context, then_block, output_var)?;

            let code = if let Some(else_block) = else_block {
                let else_code = generate_block_push(context, else_block, output_var)?;

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
                        then_actions.extend(new_actions);

                        // Execute the else block and collect its actions
                        let original_len = #output_var.len();
                        #else_code
                        let new_actions: Vec<_> = #output_var.drain(original_len..).collect();
                        else_actions.extend(new_actions);

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
                    then_actions.extend(new_actions);

                    // Create the If statement with collected actions
                    let mut if_action = hyperchad_actions::logic::if_stmt(condition_result, hyperchad_actions::ActionType::NoOp);
                    if_action.actions = then_actions;

                    #output_var.push(hyperchad_actions::ActionType::Logic(if_action).into());
                }
            };

            Ok(code)
        }
        Statement::Match { expr, arms } => {
            let expr_code = generate_expression_code(context, expr)?;
            let arms_code: Result<Vec<_>, String> = arms
                .iter()
                .map(|arm| generate_match_arm_push(context, arm, output_var))
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
            let iter_code = generate_expression_code(context, iter)?;
            let body_code = generate_block_push(context, body, output_var)?;

            Ok(quote! {
                for #pattern_name in #iter_code {
                    #body_code
                }
            })
        }
        Statement::While { condition, body } => {
            let condition_code = generate_expression_code(context, condition)?;
            let body_code = generate_block_push(context, body, output_var)?;

            Ok(quote! {
                while #condition_code {
                    #body_code
                }
            })
        }
        Statement::Block(block) => generate_block_push(context, block, output_var),
    }
}

/// Generates Rust code for a statement
///
/// This is the main entry point for statement code generation. It produces token streams
/// that represent `HyperChad` action types.
///
/// # Errors
///
/// * Returns error if expression code generation fails
/// * Returns error if block code generation fails
/// * Returns error if match arm code generation fails
#[allow(clippy::too_many_lines)]
pub fn generate_statement_code(
    context: &mut Context,
    stmt: &Statement,
) -> Result<TokenStream, String> {
    match stmt {
        Statement::Expression(expr) => {
            let expr_code = generate_expression_code(context, expr)?;

            Ok(quote! {
                #expr_code
            })
        }
        Statement::Let { name, value } => {
            let var_name = syn::Lit::Str(syn::LitStr::new(name, proc_macro2::Span::call_site()));
            let value_code = generate_expression_code(context, value)?;
            context.add_variable(name);
            Ok(quote! {
                hyperchad_actions::ActionType::Let {
                    name: #var_name.to_string(),
                    value: #value_code,
                }
            })
        }
        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            // Check if the condition is a boolean literal vs a complex expression
            let constant_condition = if let Expression::Literal(Literal::Bool(x)) = condition {
                Some(*x)
            } else {
                None
            };

            if let Some(condition) = constant_condition {
                let actions = if condition {
                    if then_block.statements.is_empty() {
                        return Ok(quote! { hyperchad_actions::ActionEffect::NoOp });
                    } else if then_block.statements.len() == 1 {
                        let action = generate_statement_code(context, &then_block.statements[0])?;
                        return Ok(quote! { #action });
                    }
                    then_block
                        .statements
                        .iter()
                        .map(|x| generate_statement_code(context, x))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(|effect| {
                            quote! {
                                (#effect).into_action_effect()
                            }
                        })
                        .collect::<Vec<_>>()
                } else if let Some(else_block) = &else_block {
                    if else_block.statements.is_empty() {
                        return Ok(quote! { hyperchad_actions::ActionEffect::NoOp });
                    } else if else_block.statements.len() == 1 {
                        let action = generate_statement_code(context, &else_block.statements[0])?;
                        return Ok(quote! { #action });
                    }
                    else_block
                        .statements
                        .iter()
                        .map(|x| generate_statement_code(context, x))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(|effect| {
                            quote! {
                                (#effect).into_action_effect()
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };

                Ok(quote! {
                    hyperchad_actions::ActionType::MultiEffect(vec![#(#actions),*])
                })
            } else {
                let condition_code = generate_expression_code(context, condition)?;

                let then_actions = then_block
                    .statements
                    .iter()
                    .map(|x| generate_statement_code(context, x))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|effect| {
                        quote! {
                            (#effect).into_action_effect()
                        }
                    })
                    .collect::<Vec<_>>();

                let else_actions = if let Some(else_block) = &else_block {
                    else_block
                        .statements
                        .iter()
                        .map(|x| generate_statement_code(context, x))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(|effect| {
                            quote! {
                                (#effect).into_action_effect()
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };

                Ok(quote! {
                    hyperchad_actions::ActionType::Logic(hyperchad_actions::logic::If {
                        condition: #condition_code,
                        actions: vec![#(#then_actions),*],
                        else_actions: vec![#(#else_actions),*],
                    })
                })
            }
        }
        Statement::Match { expr, arms } => {
            let expr_code = generate_expression_code(context, expr)?;
            let arms_code: Result<Vec<_>, String> = arms
                .iter()
                .map(|x| generate_match_arm_code(context, x))
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
            let iter_code = generate_expression_code(context, iter)?;
            let body_code = generate_block_code(context, body)?;

            Ok(quote! {
                for #pattern_name in #iter_code {
                    #body_code
                }
            })
        }
        Statement::While { condition, body } => {
            let condition_code = generate_expression_code(context, condition)?;
            let body_code = generate_block_code(context, body)?;

            Ok(quote! {
                while #condition_code {
                    #body_code
                }
            })
        }
        Statement::Block(block) => generate_block_code(context, block),
    }
}

/// Generate code for a block
fn generate_block_push(
    context: &mut Context,
    block: &Block,
    output_var: &Ident,
) -> Result<TokenStream, String> {
    let mut statements = Vec::new();

    push_scope!(context, {
        for stmt in &block.statements {
            let code = generate_statement_push(context, stmt, output_var)?;
            statements.push(code);
        }
    });

    Ok(quote! {
        {
            #(#statements)*
        }
    })
}

/// Generate code for a block
fn generate_block_code(context: &mut Context, block: &Block) -> Result<TokenStream, String> {
    let mut statements = Vec::new();

    push_scope!(context, {
        for stmt in &block.statements {
            let code = generate_statement_code(context, stmt)?;
            statements.push(code);
        }
    });

    Ok(quote! {{
        #(#statements)*
    }})
}

/// Generate code for a match arm
fn generate_match_arm_push(
    context: &mut Context,
    arm: &MatchArm,
    output_var: &Ident,
) -> Result<TokenStream, String> {
    let pattern_code = generate_pattern_code(&arm.pattern);
    let body_code = generate_expression_code(context, &arm.body)?;

    Ok(quote! {
        #pattern_code => {
            let _action_result = #body_code;
            #output_var.push(_action_result.into());
        },
    })
}

/// Generate code for a match arm
fn generate_match_arm_code(context: &mut Context, arm: &MatchArm) -> Result<TokenStream, String> {
    let pattern_code = generate_pattern_code(&arm.pattern);
    let body_code = generate_expression_code(context, &arm.body)?;

    Ok(quote! {
        #pattern_code => {
            #body_code
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

/// Generates Rust code for an expression
///
/// Converts DSL expressions into token streams that represent `HyperChad` logic values,
/// actions, or raw Rust code depending on the expression type.
///
/// # Errors
///
/// * Returns error if literal code generation fails
/// * Returns error if function call code generation fails
/// * Returns error if method call code generation fails
/// * Returns error if binary operation code generation fails
/// * Returns error if nested expression generation fails
/// * Returns error if raw Rust code cannot be parsed
#[allow(clippy::too_many_lines)]
pub fn generate_expression_code(
    context: &mut Context,
    expr: &Expression,
) -> Result<TokenStream, String> {
    match expr {
        Expression::Literal(lit) => {
            let lit_code = generate_literal_code(lit);
            Ok(quote! { hyperchad_actions::dsl::Expression::Literal(#lit_code) })
        }
        Expression::Variable(name) => {
            // Generic enum variant handling: Type::Variant pattern
            if let Some((enum_type, variant)) = name.split_once("::") {
                return Ok(Context::resolve_enum_variant(enum_type, variant));
            }

            // Regular variable handling
            if context.is_variable_defined(name) {
                let name = syn::LitStr::new(name, proc_macro2::Span::call_site());
                Ok(quote! {
                    hyperchad_actions::dsl::ElementVariable {
                        name: #name.to_string(),
                    }
                })
            } else {
                // Check if this is a valid Rust identifier that should be used directly
                let var_name = format_ident!("{name}");
                Ok(quote! { #var_name })
            }
        }
        Expression::ElementRef(element_ref) => match &**element_ref {
            Expression::Literal(Literal::String(selector)) => {
                let selector = selector.clone();
                Ok(quote! {
                    hyperchad_actions::dsl::Expression::ElementRef(
                        Box::new(hyperchad_actions::dsl::Expression::Literal(
                            hyperchad_actions::dsl::Literal::String(#selector.to_string())
                        ))
                    )
                })
            }
            Expression::Variable(selector) => {
                let selector = format_ident!("{selector}");
                Ok(quote! {
                    hyperchad_actions::dsl::Expression::ElementRef(
                        Box::new(hyperchad_actions::dsl::Expression::Literal(
                            hyperchad_actions::dsl::Literal::String(
                                format!("#{}", #selector)
                            )
                        ))
                    )
                })
            }
            _ => Err("Invalid element selector".to_string()),
        },

        Expression::Call { function, args } => generate_function_call_code(context, function, args),
        Expression::MethodCall {
            receiver,
            method,
            args,
        } => {
            if let hyperchad_actions::dsl::Expression::ElementRef(element_ref) = &**receiver {
                match &**element_ref {
                    Expression::Literal(Literal::String(selector)) => {
                        let reference = hyperchad_actions::dsl::ElementReference {
                            selector: selector.clone(),
                        };
                        match reference.parse_selector() {
                            hyperchad_actions::dsl::ParsedSelector::Id(id) => {
                                let id = syn::LitStr::new(&id, proc_macro2::Span::call_site());
                                let id = quote! { #id };
                                return generate_action_for_id(context, &id, method, args);
                            }
                            hyperchad_actions::dsl::ParsedSelector::Class(class) => {
                                return generate_action_for_class(context, &class, method, args);
                            }
                            hyperchad_actions::dsl::ParsedSelector::Complex(..)
                            | hyperchad_actions::dsl::ParsedSelector::Invalid => {}
                        }
                    }
                    Expression::Variable(selector) => {
                        let id = format_ident!("{selector}");
                        let id = quote! { #id };
                        return generate_action_for_id(context, &id, method, args);
                    }
                    _ => {}
                }
            }

            let receiver_code = generate_expression_code(context, receiver)?;
            generate_method_call_code(context, &receiver_code, method, args)
        }
        Expression::Field { object, field } => {
            let object_code = generate_expression_code(context, object)?;
            let field_ident = format_ident!("{}", field);

            Ok(quote! {
                #object_code.#field_ident
            })
        }
        Expression::Binary { left, op, right } => {
            let left_code = generate_expression_code(context, left)?;
            let right_code = generate_expression_code(context, right)?;

            // Helper function to wrap enum variants in Value::from for logical operations
            let wrap_for_logic = |expr: &Expression, code: TokenStream| -> TokenStream {
                match expr {
                    Expression::Variable(name) if name.contains("::") => {
                        // This is an enum variant, wrap it in Value::from for logical operations
                        quote! { hyperchad_actions::logic::Value::from(#code) }
                    }
                    _ => code,
                }
            };

            // Handle special cases for logic operations
            match op {
                BinaryOp::Equal => {
                    // For equality, we need to use the logic::eq function to create a Condition
                    // Wrap enum variants in Value::from for logical operations
                    let left_wrapped = wrap_for_logic(left, left_code);
                    let right_wrapped = wrap_for_logic(right, right_code);
                    Ok(quote! {
                        hyperchad_actions::logic::eq(#left_wrapped, #right_wrapped)
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
            let expr_code = generate_expression_code(context, expr)?;
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
            let condition_code = generate_expression_code(context, condition)?;
            let then_code = generate_expression_code(context, then_branch)?;

            if let Some(else_branch) = else_branch {
                let else_code = generate_expression_code(context, else_branch)?;
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
            let block_code = generate_block_push(context, block, &output_var)?;

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
            let exprs_code: Result<Vec<_>, String> = exprs
                .iter()
                .map(|x| generate_expression_code(context, x))
                .collect();
            let exprs_code = exprs_code?;

            Ok(quote! {
                vec![#(#exprs_code),*]
            })
        }
        Expression::Tuple(exprs) => {
            let exprs_code: Result<Vec<_>, String> = exprs
                .iter()
                .map(|x| generate_expression_code(context, x))
                .collect();
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
                generate_expression_code(context, start)?
            } else {
                quote! { 0 }
            };

            let end_code = if let Some(end) = end {
                generate_expression_code(context, end)?
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
            let body_code = generate_expression_code(context, body)?;

            Ok(quote! {
                |#(#param_idents),*| #body_code
            })
        }
        Expression::RawRust(code) => {
            // For raw Rust code, just parse it as tokens and return directly
            // This preserves the original behavior for cases that can't be parsed by the DSL
            let tokens: TokenStream = match code.parse() {
                Ok(tokens) => tokens,
                Err(e) => {
                    return Err(format!("Failed to parse raw Rust code: {e}"));
                }
            };
            Ok(tokens)
        }
        Expression::Grouping(inner) => {
            // Generate the inner expression surrounded by parentheses to preserve grouping
            let inner_code = generate_expression_code(context, inner)?;
            Ok(quote! {
                hyperchad_actions::logic::Arithmetic::Grouping(#inner_code.into())
            })
        }
    }
}

fn generate_action_for_id(
    context: &mut Context,
    id: &TokenStream,
    method: &str,
    args: &[Expression],
) -> Result<TokenStream, String> {
    Ok(match method {
        "show" => {
            if !args.is_empty() {
                return Err("ElementReference.show() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::show_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "hide" => {
            if !args.is_empty() {
                return Err("ElementReference.hide() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::hide_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "toggle_visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.toggle_visibility() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::toggle_visibility_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "set_visibility" => {
            if args.len() != 1 {
                return Err(
                    "ElementReference.set_visibility() expects exactly 1 argument".to_string(),
                );
            }
            let visibility = generate_expression_code(context, &args[0])?;
            quote! {
                hyperchad_actions::ActionType::set_visibility_str_id(#visibility, hyperchad_actions::Target::literal(#id))
            }
        }
        "focus" => {
            if !args.is_empty() {
                return Err("ElementReference.focus() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::focus_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "get_visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.get_visibility() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::get_visibility_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "select" => {
            if !args.is_empty() {
                return Err("ElementReference.select() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::select_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "display" => {
            if !args.is_empty() {
                return Err("ElementReference.display() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::display_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "no_display" => {
            if !args.is_empty() {
                return Err("ElementReference.no_display() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::no_display_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "toggle_display" => {
            if !args.is_empty() {
                return Err("ElementReference.toggle_display() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::toggle_display_str_id(hyperchad_actions::Target::literal(#id))
            }
        }
        "set_display" => {
            if args.len() != 1 {
                return Err("ElementReference.set_display() expects exactly 1 argument".to_string());
            }
            let display = generate_expression_code(context, &args[0])?;
            quote! {
                hyperchad_actions::ActionType::set_display_str_id(#display, hyperchad_actions::Target::literal(#id))
            }
        }
        unknown => {
            return Err(format!("Unknown method: {unknown}"));
        }
    })
}

fn generate_action_for_class(
    context: &mut Context,
    class: &str,
    method: &str,
    args: &[Expression],
) -> Result<TokenStream, String> {
    Ok(match method {
        "show" => {
            if !args.is_empty() {
                return Err("ElementReference.show() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::show_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "hide" => {
            if !args.is_empty() {
                return Err("ElementReference.hide() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::hide_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "toggle_visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.toggle_visibility() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::toggle_visibility_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "set_visibility" => {
            if args.len() != 1 {
                return Err(
                    "ElementReference.set_visibility() expects exactly 1 argument".to_string(),
                );
            }
            let visibility = generate_expression_code(context, &args[0])?;
            quote! {
                hyperchad_actions::ActionType::set_visibility_class(#visibility, hyperchad_actions::Target::literal(#class))
            }
        }
        "focus" => {
            if !args.is_empty() {
                return Err("ElementReference.focus() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::focus_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "get_visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.get_visibility() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::get_visibility_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "select" => {
            if !args.is_empty() {
                return Err("ElementReference.select() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::select_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "display" => {
            if !args.is_empty() {
                return Err("ElementReference.display() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::display_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "no_display" => {
            if !args.is_empty() {
                return Err("ElementReference.no_display() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::no_display_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "toggle_display" => {
            if !args.is_empty() {
                return Err("ElementReference.toggle_display() expects no arguments".to_string());
            }
            quote! {
                hyperchad_actions::ActionType::toggle_display_str_class(hyperchad_actions::Target::literal(#class))
            }
        }
        "set_display" => {
            if args.len() != 1 {
                return Err("ElementReference.set_display() expects exactly 1 argument".to_string());
            }
            let display = generate_expression_code(context, &args[0])?;
            quote! {
                hyperchad_actions::ActionType::set_display_class(#display, hyperchad_actions::Target::literal(#class))
            }
        }
        unknown => {
            return Err(format!("Unknown method: {unknown}"));
        }
    })
}

fn target_to_expr(
    context: &mut Context,
    target: &Expression,
    into: bool,
) -> Result<TokenStream, String> {
    match target {
        Expression::Variable(name) => {
            if context.is_variable_defined(name) {
                let name = syn::LitStr::new(name, proc_macro2::Span::call_site());
                return Ok(quote! {
                    hyperchad_actions::Target::reference(#name)
                });
            }
        }
        Expression::Literal(Literal::String(name)) => {
            let name = syn::LitStr::new(name, proc_macro2::Span::call_site());
            return Ok(quote! {
                hyperchad_actions::Target::literal(#name)
            });
        }
        Expression::Literal(..)
        | Expression::ElementRef(..)
        | Expression::Call { .. }
        | Expression::MethodCall { .. }
        | Expression::Field { .. }
        | Expression::Binary { .. }
        | Expression::Unary { .. }
        | Expression::If { .. }
        | Expression::Match { .. }
        | Expression::Block(..)
        | Expression::Array(..)
        | Expression::Tuple(..)
        | Expression::Range { .. }
        | Expression::Closure { .. }
        | Expression::RawRust(..)
        | Expression::Grouping(..) => {}
    }

    let code = generate_expression_code(context, target)?;

    Ok(if into {
        quote! {
            #code.into()
        }
    } else {
        quote! {
            #code
        }
    })
}

/// Generate code for function calls, mapping DSL functions to hyperchad actions
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn generate_function_call_code(
    context: &mut Context,
    function: &str,
    args: &[Expression],
) -> Result<TokenStream, String> {
    // Handle struct-like enum variants: Type::Variant with named field arguments
    if function.contains("::") {
        let (enum_type, variant) = function.split_once("::").unwrap();

        // Check if this is a struct-like variant (args are tuples with field names)
        if !args.is_empty() && args.iter().all(|arg| matches!(arg, Expression::Tuple(_))) {
            let enum_ident = format_ident!("{}", enum_type);
            let variant_ident = format_ident!("{}", variant);

            // Generate field assignments from tuple arguments
            let mut field_assignments = Vec::new();
            for arg in args {
                if let Expression::Tuple(tuple_args) = arg
                    && tuple_args.len() == 2
                    && let Expression::Literal(Literal::String(field_name)) = &tuple_args[0]
                {
                    let field_ident = format_ident!("{}", field_name);
                    let field_value = generate_expression_code(context, &tuple_args[1])?;
                    field_assignments.push(quote! { #field_ident: #field_value });
                }
            }

            // Generate the struct-like enum construction
            return Ok(quote! {
                #enum_ident::#variant_ident {
                    #(#field_assignments),*
                }
            });
        }

        // For simple enum variants without fields, fall back to generic enum resolution
        if args.is_empty() {
            return Ok(Context::resolve_enum_variant(enum_type, variant));
        }
    }

    match function {
        // Element reference function
        "element" => {
            if args.len() != 1 {
                return Err("element() expects exactly 1 argument".to_string());
            }

            // Note: element() calls that are directly chained with methods (e.g., element("#id").show())
            // are optimized in the method call handler. This case handles standalone element() calls.
            let selector = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::dsl::Expression::ElementRef(
                    hyperchad_actions::dsl::ElementReference {
                        selector: #selector.to_string()
                    }
                )
            })
        }

        // Element visibility functions
        "hide" => {
            if args.len() != 1 {
                return Err("hide() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                    action: hyperchad_actions::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Hidden
                    ),
                }
            })
        }
        "show" => {
            if args.len() != 1 {
                return Err("show() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                    action: hyperchad_actions::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Visible
                    ),
                }
            })
        }
        "set_visibility" => {
            if args.len() != 2 {
                return Err("set_visibility() expects exactly 2 arguments".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            let visibility = generate_expression_code(context, &args[1])?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                    action: hyperchad_actions::StyleAction::SetVisibility(#visibility),
                }
            })
        }

        // Display functions
        "display" => {
            if args.len() != 1 {
                return Err("display() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                    action: hyperchad_actions::StyleAction::SetDisplay(true),
                }
            })
        }
        "no_display" => {
            if args.len() != 1 {
                return Err("no_display() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                    action: hyperchad_actions::StyleAction::SetDisplay(false),
                }
            })
        }
        "set_display" => {
            if args.len() != 2 {
                return Err("set_display() expects exactly 2 arguments".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            let display = generate_expression_code(context, &args[1])?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                    action: hyperchad_actions::StyleAction::SetDisplay(#display),
                }
            })
        }
        "toggle_display" => {
            if args.len() != 1 {
                return Err("toggle_display() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            Ok(quote! {
                hyperchad_actions::ActionType::toggle_display_str_id(#target)
            })
        }

        // Background functions
        "set_background_self" => {
            if args.len() != 1 {
                return Err("set_background() expects exactly 1 argument".to_string());
            }
            let background = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::ActionType::set_background_self(#background.to_string())
            })
        }

        // Visibility functions
        "set_visibility_child_class" => {
            if args.len() != 2 {
                return Err("set_visibility_child_class() expects exactly 2 arguments".to_string());
            }
            let visibility = generate_expression_code(context, &args[0])?;
            let target = target_to_expr(context, &args[1], true)?;
            Ok(quote! {
                hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::ChildClass(#target),
                    action: hyperchad_actions::StyleAction::SetVisibility(#visibility),
                }
            })
        }

        // Display functions
        "display_str_id" => {
            if args.len() != 1 {
                return Err("display_str_id() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::ActionType::display_str_id(#target)
            })
        }
        "no_display_str_id" => {
            if args.len() != 1 {
                return Err("no_display_str_id() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::ActionType::no_display_str_id(#target)
            })
        }
        "display_class" => {
            if args.len() != 1 {
                return Err("display_class() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::ActionType::display_class(#target)
            })
        }
        "no_display_class" => {
            if args.len() != 1 {
                return Err("no_display_class() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::ActionType::no_display_class(#target)
            })
        }

        // Getter functions
        "get_visibility" => {
            if args.len() != 1 {
                return Err("get_visibility() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], true)?;
            Ok(quote! {
                hyperchad_actions::logic::CalcValue::Visibility {
                    target: hyperchad_actions::ElementTarget::StrId(#target),
                }
            })
        }
        "get_display" => {
            if args.len() != 1 {
                return Err("get_display() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::logic::get_display_str_id(#target)
            })
        }
        "get_width" => {
            if args.len() != 1 {
                return Err("get_width() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::logic::get_width_px_str_id(#target)
            })
        }
        "get_width_px_self" => {
            if !args.is_empty() {
                return Err("get_width_px_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_width_px_self()
            })
        }
        "get_height" => {
            if args.len() != 1 {
                return Err("get_height() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::logic::get_height_px_str_id(#target)
            })
        }
        "get_height_px_str_id" => {
            if args.len() != 1 {
                return Err("get_height_px_str_id() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::logic::get_height_px_str_id(#target)
            })
        }
        "get_mouse_x" => {
            if args.is_empty() {
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_x()
                })
            } else if args.len() == 1 {
                let target = target_to_expr(context, &args[0], false)?;
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_x_str_id(#target)
                })
            } else {
                Err("get_mouse_x() expects 0 or 1 arguments".to_string())
            }
        }
        "get_mouse_x_self" => {
            if !args.is_empty() {
                return Err("get_mouse_x_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_mouse_x_self()
            })
        }
        "get_mouse_y" => {
            if args.is_empty() {
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_y()
                })
            } else if args.len() == 1 {
                let target = target_to_expr(context, &args[0], false)?;
                Ok(quote! {
                    hyperchad_actions::logic::get_mouse_y_str_id(#target)
                })
            } else {
                Err("get_mouse_y() expects 0 or 1 arguments".to_string())
            }
        }
        "get_mouse_y_str_id" => {
            if args.len() != 1 {
                return Err("get_mouse_y_str_id() expects exactly 1 argument".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::logic::get_mouse_y_str_id(#target)
            })
        }

        "get_data_attr_value" => {
            if args.len() != 2 {
                return Err("get_data_attr_value() expects exactly 2 arguments".to_string());
            }
            let target = target_to_expr(context, &args[0], false)?;
            let attr = literal_or_stmt(context, &args[1])?;
            Ok(quote! {
                hyperchad_actions::logic::get_data_attr_value(#target, #attr)
            })
        }

        "get_data_attr_value_self" => {
            if args.len() != 1 {
                return Err("get_data_attr_value_self() expects exactly 1 argument".to_string());
            }
            let attr = literal_or_stmt(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::logic::get_data_attr_value_self(#attr)
            })
        }

        // Utility functions
        "noop" => Ok(quote! {
            hyperchad_actions::ActionType::NoOp
        }),
        "log" => {
            if args.len() != 1 {
                return Err("log() expects exactly 1 argument".to_string());
            }
            let message = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::ActionType::Log {
                    message: #message.to_string(),
                    level: hyperchad_actions::LogLevel::Info,
                }
            })
        }
        "navigate" => {
            if args.len() != 1 {
                return Err("navigate() expects exactly 1 argument".to_string());
            }
            let url = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::ActionType::Navigate {
                    url: #url.to_string(),
                }
            })
        }
        "custom" => {
            if args.len() != 1 {
                return Err("custom() expects exactly 1 argument".to_string());
            }
            let action = generate_expression_code(context, &args[0])?;
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
        "displayed" => Ok(quote! {
            hyperchad_actions::logic::displayed()
        }),
        "not_displayed" => Ok(quote! {
            hyperchad_actions::logic::not_displayed()
        }),
        "eq" => {
            if args.len() != 2 {
                return Err("eq() expects exactly 2 arguments".to_string());
            }
            let left = generate_expression_code(context, &args[0])?;
            let right = generate_expression_code(context, &args[1])?;
            Ok(quote! {
                hyperchad_actions::logic::eq(#left, #right)
            })
        }

        // Action invocation function
        "invoke" => {
            if args.len() != 2 {
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
                            if let Expression::Tuple(tuple_args) = arg
                                && let (
                                    Expression::Literal(Literal::String(field_name)),
                                    field_value,
                                ) = (&tuple_args[0], &tuple_args[1])
                            {
                                let field_ident = format_ident!("{}", field_name);
                                let field_value_code =
                                    generate_expression_code(context, field_value)?;
                                field_assignments.push(quote! { #field_ident: #field_value_code });
                            }
                        }

                        quote! { #struct_path_tokens { #(#field_assignments),* } }
                    } else {
                        generate_expression_code(context, &args[0])?
                    }
                } else {
                    generate_expression_code(context, &args[0])?
                }
            } else {
                generate_expression_code(context, &args[0])?
            };

            let value = generate_expression_code(context, &args[1])?;
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
            if args.len() != 2 {
                return Err("throttle() expects exactly 2 arguments (duration, action)".to_string());
            }
            let duration = generate_expression_code(context, &args[0])?;
            let action = generate_expression_code(context, &args[1])?;
            Ok(quote! {
                hyperchad_actions::ActionEffect {
                    action: #action,
                    throttle: Some(#duration as u64),
                    ..Default::default()
                }
            })
        }

        // Delay off function: delay_off(duration, action)
        "delay_off" => {
            if args.len() != 2 {
                return Err(
                    "delay_off() expects exactly 2 arguments (duration, action)".to_string()
                );
            }
            let duration = generate_expression_code(context, &args[0])?;
            let action = generate_expression_code(context, &args[1])?;
            Ok(quote! {
                hyperchad_actions::ActionEffect {
                    action: #action,
                    delay_off: Some(#duration as u64),
                    ..Default::default()
                }
            })
        }

        // Unique function: unique(action)
        "unique" => {
            if args.len() != 1 {
                return Err("unique() expects exactly 1 argument (action)".to_string());
            }
            let action = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::ActionEffect {
                    action: #action,
                    unique: Some(true),
                    ..Default::default()
                }
            })
        }

        // Clamp function: clamp(min, value, max)
        "clamp" => {
            if args.len() != 3 {
                return Err("clamp() expects exactly 3 arguments (min, value, max)".to_string());
            }
            let min = literal_or_stmt(context, &args[0])?;
            let value = generate_expression_code(context, &args[1])?;
            let max = literal_or_stmt(context, &args[2])?;
            Ok(quote! {
                #value.clamp(#min, #max)
            })
        }

        // Group function for arithmetic grouping: group(expression)
        "group" => {
            if args.len() != 1 {
                return Err("group() expects exactly 1 argument".to_string());
            }
            let expr = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::logic::Arithmetic::Grouping(Box::new(#expr))
            })
        }

        // Event handling function: on_event(event_name, closure)
        "on_event" => {
            if args.len() != 2 {
                return Err(
                    "on_event() expects exactly 2 arguments (event_name, closure)".to_string(),
                );
            }
            let event_name = generate_expression_code(context, &args[0])?;
            let closure_expr = &args[1]; // Use the original expression, not the generated code

            // Check if the second argument is a closure
            if let Expression::Closure { params, body } = closure_expr {
                // Transform the closure into hyperchad logic
                // Replace the closure parameter with get_event_value() calls
                let transformed_body = transform_closure_for_event(context, params, body)?;
                Ok(quote! {
                    hyperchad_actions::ActionType::Event {
                        name: #event_name.to_string(),
                        action: Box::new(#transformed_body),
                    }
                })
            } else {
                // If it's not a closure, just use the regular action
                let action = generate_expression_code(context, &args[1])?;
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
                    if args.len() == 1 {
                        // Expecting a tuple with field name and value
                        let url_arg = generate_expression_code(context, &args[0])?;
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
                    if args.len() != 1 {
                        return Err(
                            "hyperchad_actions::ActionType::hide_str_id() expects exactly 1 argument".to_string()
                        );
                    }
                    let target = target_to_expr(context, &args[0], false)?;
                    Ok(quote! {
                        hyperchad_actions::ActionType::hide_str_id(#target)
                    })
                }
                "show_str_id" => {
                    if args.len() != 1 {
                        return Err(
                            "hyperchad_actions::ActionType::show_str_id() expects exactly 1 argument".to_string()
                        );
                    }
                    let target = target_to_expr(context, &args[0], false)?;
                    Ok(quote! {
                        hyperchad_actions::ActionType::show_str_id(#target)
                    })
                }
                "show_self" => {
                    if !args.is_empty() {
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
                    if !args.is_empty() {
                        return Err("hide_self() expects no arguments".to_string());
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::hide_self()
                    })
                }
                "no_display_self" => {
                    if !args.is_empty() {
                        return Err("no_display_self() expects no arguments".to_string());
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::no_display_self()
                    })
                }
                "display_self" => {
                    if !args.is_empty() {
                        return Err("display_self() expects no arguments".to_string());
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::display_self()
                    })
                }
                "remove_background_self" => {
                    if !args.is_empty() {
                        return Err("remove_background_self() expects no arguments".to_string());
                    }
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_self()
                    })
                }
                "remove_background_str_id" => {
                    if args.len() != 1 {
                        return Err(
                            "remove_background_str_id() expects exactly 1 argument (target)"
                                .to_string(),
                        );
                    }
                    let target = target_to_expr(context, &args[0], false)?;
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_str_id(#target)
                    })
                }
                "remove_background_id" => {
                    if args.len() != 1 {
                        return Err("remove_background_id() expects exactly 1 argument (target)"
                            .to_string());
                    }
                    let target = target_to_expr(context, &args[0], false)?;
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_id(#target)
                    })
                }
                "remove_background_class" => {
                    if args.len() != 1 {
                        return Err(
                            "remove_background_class() expects exactly 1 argument (class_name)"
                                .to_string(),
                        );
                    }
                    let class_name = generate_expression_code(context, &args[0])?;
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_class(#class_name)
                    })
                }
                "remove_background_child_class" => {
                    if args.len() != 1 {
                        return Err("remove_background_child_class() expects exactly 1 argument (class_name)".to_string());
                    }
                    let class_name = generate_expression_code(context, &args[0])?;
                    Ok(quote! {
                        hyperchad_actions::ActionType::remove_background_child_class(#class_name)
                    })
                }
                "remove_background_last_child" => {
                    if !args.is_empty() {
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
                    if args.is_empty() {
                        // Simple variant without arguments
                        Ok(quote! {
                            Action::#variant_ident
                        })
                    } else {
                        let args_code = args
                            .iter()
                            .map(|x| generate_expression_code(context, x))
                            .collect::<Result<Vec<_>, String>>()?;
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

        // Key enum variants
        name if name.starts_with("Key::") => {
            let variant = name.strip_prefix("Key::").unwrap();
            let variant_ident = format_ident!("{}", variant);
            Ok(quote! {
                hyperchad_actions::logic::Value::Key(hyperchad_actions::Key::#variant_ident)
            })
        }

        // Self-targeting functions
        "show_self" => {
            if !args.is_empty() {
                return Err("show_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::show_self()
            })
        }
        "hide_self" => {
            if !args.is_empty() {
                return Err("hide_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::hide_self()
            })
        }
        "no_display_self" => {
            if !args.is_empty() {
                return Err("no_display_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::no_display_self()
            })
        }
        "display_self" => {
            if !args.is_empty() {
                return Err("display_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::display_self()
            })
        }
        "remove_background_self" => {
            if !args.is_empty() {
                return Err("remove_background_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_self()
            })
        }
        "remove_background_str_id" => {
            if args.len() != 1 {
                return Err(
                    "remove_background_str_id() expects exactly 1 argument (target)".to_string(),
                );
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_str_id(#target)
            })
        }
        "remove_background_id" => {
            if args.len() != 1 {
                return Err(
                    "remove_background_id() expects exactly 1 argument (target)".to_string()
                );
            }
            let target = target_to_expr(context, &args[0], false)?;
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_id(#target)
            })
        }
        "remove_background_class" => {
            if args.len() != 1 {
                return Err(
                    "remove_background_class() expects exactly 1 argument (class_name)".to_string(),
                );
            }
            let class_name = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_class(#class_name)
            })
        }
        "remove_background_child_class" => {
            if args.len() != 1 {
                return Err(
                    "remove_background_child_class() expects exactly 1 argument (class_name)"
                        .to_string(),
                );
            }
            let class_name = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_child_class(#class_name)
            })
        }
        "remove_background_last_child" => {
            if !args.is_empty() {
                return Err("remove_background_last_child() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::remove_background_last_child()
            })
        }
        "show_last_child" => {
            if !args.is_empty() {
                return Err("show_last_child() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::ActionType::show_last_child()
            })
        }
        "get_visibility_self" => {
            if !args.is_empty() {
                return Err("get_visibility_self() expects no arguments".to_string());
            }
            Ok(quote! {
                hyperchad_actions::logic::get_visibility_self()
            })
        }
        "get_event_value" => {
            if !args.is_empty() {
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
                        if let Expression::Tuple(tuple_args) = arg
                            && let (Expression::Literal(Literal::String(field_name)), field_value) =
                                (&tuple_args[0], &tuple_args[1])
                        {
                            let field_ident = format_ident!("{}", field_name);
                            let field_value_code = generate_expression_code(context, field_value)?;
                            field_assignments.push(quote! { #field_ident: #field_value_code });
                        }
                    }

                    return Ok(quote! {
                        #struct_path_tokens { #(#field_assignments),* }
                    });
                }
            }

            // Regular function call
            let function_ident = format_ident!("{}", function);

            let args_code = args
                .iter()
                .map(|x| generate_expression_code(context, x))
                .collect::<Result<Vec<_>, String>>()?;

            Ok(quote! {
                #function_ident(#(#args_code),*)
            })
        }
    }
}

/// Generate code for a literal
fn generate_literal_code(lit: &Literal) -> TokenStream {
    match lit {
        Literal::String(s) => quote! { hyperchad_actions::dsl::Literal::String(#s.to_string()) },
        Literal::Integer(i) => quote! { hyperchad_actions::dsl::Literal::Integer(#i) },
        Literal::Float(f) => quote! { hyperchad_actions::dsl::Literal::Float(#f) },
        Literal::Bool(b) => quote! { hyperchad_actions::dsl::Literal::Bool(#b) },
        Literal::Unit => quote! { hyperchad_actions::dsl::Literal::Unit },
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

fn literal_or_stmt(context: &mut Context, expr: &Expression) -> Result<TokenStream, String> {
    Ok(match expr {
        Expression::Literal(Literal::Integer(x)) => {
            quote! { #x }
        }
        Expression::Literal(Literal::Float(x)) => {
            quote! { #x }
        }
        Expression::Literal(Literal::Bool(x)) => {
            quote! { #x }
        }
        Expression::Literal(Literal::String(x)) => {
            quote! { #x }
        }
        value => generate_expression_code(context, value)?,
    })
}

/// Generate code for method calls
#[allow(clippy::too_many_lines)]
fn generate_method_call_code(
    context: &mut Context,
    receiver: &TokenStream,
    method: &str,
    args: &[Expression],
) -> Result<TokenStream, String> {
    match method {
        // Element reference methods
        "show" => {
            if !args.is_empty() {
                return Err("ElementReference.show() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.show()
            })
        }
        "hide" => {
            if !args.is_empty() {
                return Err("ElementReference.hide() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.hide()
            })
        }

        "toggle_visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.toggle_visibility() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.toggle_visibility()
            })
        }

        "visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.visibility() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.visibility()
            })
        }
        "get_visibility" => {
            if !args.is_empty() {
                return Err("ElementReference.get_visibility() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.get_visibility()
            })
        }
        "set_visibility" => {
            if args.len() != 1 {
                return Err(
                    "ElementReference.set_visibility() expects exactly 1 argument".to_string(),
                );
            }
            let visibility = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.set_visibility(#visibility)
            })
        }

        "get_width_px" => {
            if !args.is_empty() {
                return Err("get_width_px() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.get_width_px()
            })
        }

        "get_height_px" => {
            if !args.is_empty() {
                return Err("get_height_px() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.get_height_px()
            })
        }

        "get_mouse_x" => {
            if !args.is_empty() {
                return Err("get_mouse_x() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.get_mouse_x()
            })
        }

        "get_mouse_y" => {
            if !args.is_empty() {
                return Err("get_mouse_y() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.get_mouse_y()
            })
        }

        "display" => {
            if !args.is_empty() {
                return Err("ElementReference.display() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.display()
            })
        }

        "no_display" => {
            if !args.is_empty() {
                return Err("ElementReference.no_display() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.no_display()
            })
        }

        "toggle_display" => {
            if !args.is_empty() {
                return Err("ElementReference.toggle_display() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.toggle_display()
            })
        }

        "set_display" => {
            if args.len() != 1 {
                return Err("ElementReference.set_display() expects exactly 1 argument".to_string());
            }
            let display = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.set_display(#display)
            })
        }

        "get_display" => {
            if !args.is_empty() {
                return Err("ElementReference.get_display() expects no arguments".to_string());
            }
            Ok(quote! {
                #receiver.get_display()
            })
        }

        // Logic chaining methods
        "eq" => {
            if args.len() != 1 {
                return Err("eq() expects exactly 1 argument".to_string());
            }
            let value = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.eq(#value)
            })
        }
        "then" => {
            if args.len() != 1 {
                return Err("then() expects exactly 1 argument".to_string());
            }
            let action = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.then(#action)
            })
        }
        "or_else" => {
            if args.len() != 1 {
                return Err("or_else() expects exactly 1 argument".to_string());
            }
            let action = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.or_else(#action)
            })
        }
        "and" => {
            if args.len() != 1 {
                return Err("and() expects exactly 1 argument".to_string());
            }
            let action = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.and(#action)
            })
        }
        "then_pass_to" => {
            if args.len() != 1 {
                return Err("then_pass_to() expects exactly 1 argument".to_string());
            }
            let action = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.then_pass_to(#action)
            })
        }
        // Math operations
        "divide" => {
            if args.len() != 1 {
                return Err("divide() expects exactly 1 argument".to_string());
            }
            let divisor = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.divide(#divisor)
            })
        }
        "minus" => {
            if args.len() != 1 {
                return Err("minus() expects exactly 1 argument".to_string());
            }
            let value = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.minus(#value)
            })
        }
        "plus" => {
            if args.len() != 1 {
                return Err("plus() expects exactly 1 argument".to_string());
            }
            let value = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.plus(#value)
            })
        }
        "multiply" => {
            if args.len() != 1 {
                return Err("multiply() expects exactly 1 argument".to_string());
            }
            let value = generate_expression_code(context, &args[0])?;
            Ok(quote! {
                #receiver.multiply(#value)
            })
        }
        "clamp" => {
            if args.len() != 2 {
                return Err("clamp() expects exactly 2 arguments (min, max)".to_string());
            }
            let min = literal_or_stmt(context, &args[0])?;
            let max = literal_or_stmt(context, &args[1])?;
            Ok(quote! {
                #receiver.clamp(#min, #max)
            })
        }
        // Delay/timing methods
        "delay_off" => {
            if args.len() != 1 {
                return Err("delay_off() expects exactly 1 argument".to_string());
            }
            let value = literal_or_stmt(context, &args[0])?;
            Ok(quote! {
                #receiver.delay_off(#value as u64)
            })
        }
        "throttle" => {
            if args.len() != 1 {
                return Err("throttle() expects exactly 1 argument".to_string());
            }
            let value = literal_or_stmt(context, &args[0])?;
            Ok(quote! {
                #receiver.throttle(#value as u64)
            })
        }
        // Default case - pass through to the receiver object
        _ => {
            let args_code = args
                .iter()
                .map(|x| generate_expression_code(context, x))
                .collect::<Result<Vec<_>, String>>()?;

            let method_ident = format_ident!("{}", method);
            Ok(quote! {
                #receiver.#method_ident(#(#args_code),*)
            })
        }
    }
}

/// Transform a closure used in `on_event` by replacing parameters with `get_event_value()` calls
fn transform_closure_for_event(
    context: &mut Context,
    params: &[String],
    body: &Expression,
) -> Result<TokenStream, String> {
    // For now, assume single parameter named 'value' that should be replaced with get_event_value()
    if params.len() != 1 {
        return Err("on_event closures must have exactly one parameter".to_string());
    }

    let param_name = &params[0];
    let transformed_body = replace_param_with_get_event_value(context, body, param_name)?;
    generate_expression_code(context, &transformed_body)
}

/// Replace a parameter in an expression with `get_event_value()` calls
fn replace_param_with_get_event_value(
    context: &mut Context,
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
            let left_transformed = replace_param_with_get_event_value(context, left, param_name)?;
            let right_transformed = replace_param_with_get_event_value(context, right, param_name)?;
            Ok(Expression::Binary {
                left: Box::new(left_transformed),
                op: op.clone(),
                right: Box::new(right_transformed),
            })
        }
        Expression::Call { function, args } => {
            let transformed_args: Result<Vec<_>, String> = args
                .iter()
                .map(|arg| replace_param_with_get_event_value(context, arg, param_name))
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
            let condition_transformed =
                replace_param_with_get_event_value(context, condition, param_name)?;
            let then_transformed =
                replace_param_with_get_event_value(context, then_branch, param_name)?;
            let else_transformed = if let Some(else_branch) = else_branch {
                Some(Box::new(replace_param_with_get_event_value(
                    context,
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
                .map(|stmt| replace_param_in_statement(context, stmt, param_name))
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
            let receiver_transformed =
                replace_param_with_get_event_value(context, receiver, param_name)?;
            let transformed_args: Result<Vec<_>, String> = args
                .iter()
                .map(|arg| replace_param_with_get_event_value(context, arg, param_name))
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
fn replace_param_in_statement(
    context: &mut Context,
    stmt: &Statement,
    param_name: &str,
) -> Result<Statement, String> {
    match stmt {
        Statement::Expression(expr) => {
            let transformed_expr = replace_param_with_get_event_value(context, expr, param_name)?;
            Ok(Statement::Expression(transformed_expr))
        }
        Statement::Let { name, value } => {
            let transformed_value = replace_param_with_get_event_value(context, value, param_name)?;
            context.add_variable(name);
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
            let condition_transformed =
                replace_param_with_get_event_value(context, condition, param_name)?;
            let then_transformed = replace_param_in_block(context, then_block, param_name)?;
            let else_transformed = if let Some(else_block) = else_block {
                Some(replace_param_in_block(context, else_block, param_name)?)
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
            let transformed_block = replace_param_in_block(context, block, param_name)?;
            Ok(Statement::Block(transformed_block))
        }
        // For other statements, return as-is
        _ => Ok(stmt.clone()),
    }
}

/// Replace a parameter in a block with `get_event_value()` calls
fn replace_param_in_block(
    context: &mut Context,
    block: &Block,
    param_name: &str,
) -> Result<Block, String> {
    let transformed_statements: Result<Vec<_>, String> = block
        .statements
        .iter()
        .map(|stmt| replace_param_in_statement(context, stmt, param_name))
        .collect();
    Ok(Block {
        statements: transformed_statements?,
    })
}
