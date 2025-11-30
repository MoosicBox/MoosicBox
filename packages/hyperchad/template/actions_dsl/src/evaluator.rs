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
    ///
    /// # Must use
    ///
    /// The result indicates whether code generation should treat the name as a variable
    /// reference or a direct identifier
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
    /// The returned token stream represents the enum variant and must be included in
    /// the generated code to properly construct the variant
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

/// Generates code for a statement that pushes actions to an output vector
///
/// This is used for runtime code generation where actions are accumulated dynamically.
///
/// # Errors
///
/// * Returns error if expression code generation fails
/// * Returns error if block code generation fails
/// * Returns error if match arm code generation fails
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

/// Generates code for a block that pushes actions to an output vector
///
/// This is used for runtime code generation where actions are accumulated in a vector.
///
/// # Errors
///
/// * Returns error if statement code generation fails
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

/// Generates code for a block expression
///
/// # Errors
///
/// * Returns error if statement code generation fails
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

/// Generates code for a match arm that pushes the result to an output vector
///
/// # Errors
///
/// * Returns error if pattern or body code generation fails
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

/// Generates code for a match arm expression
///
/// # Errors
///
/// * Returns error if pattern or body code generation fails
fn generate_match_arm_code(context: &mut Context, arm: &MatchArm) -> Result<TokenStream, String> {
    let pattern_code = generate_pattern_code(&arm.pattern);
    let body_code = generate_expression_code(context, &arm.body)?;

    Ok(quote! {
        #pattern_code => {
            #body_code
        },
    })
}

/// Generates Rust code for a pattern
///
/// Converts DSL patterns into token streams for use in match expressions.
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

/// Generates action code for element ID-based selectors
///
/// Creates optimized `HyperChad` actions targeting elements by ID.
///
/// # Errors
///
/// * Returns error if method name is unknown
/// * Returns error if argument count is incorrect for the method
/// * Returns error if expression code generation fails
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

/// Generates action code for element class-based selectors
///
/// Creates optimized `HyperChad` actions targeting elements by CSS class.
///
/// # Errors
///
/// * Returns error if method name is unknown
/// * Returns error if argument count is incorrect for the method
/// * Returns error if expression code generation fails
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

/// Converts a target expression into a `HyperChad` Target token stream
///
/// Determines whether the target is a variable reference or literal and generates
/// appropriate Target construction code.
///
/// # Errors
///
/// * Returns error if expression code generation fails
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

/// Generates code for function calls, mapping DSL functions to `HyperChad` actions
///
/// This is the primary function call handler, supporting:
/// * Element manipulation functions (show, hide, display, etc.)
/// * Event handling (`on_event`)
/// * Action invocation (invoke, throttle, `delay_off`)
/// * Utility functions (clamp, log, navigate)
/// * Enum variant construction
///
/// # Errors
///
/// * Returns error if function name is unknown
/// * Returns error if argument count is incorrect
/// * Returns error if expression code generation fails
/// * Returns error if struct path parsing fails
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

/// Generates Rust code for a literal value
///
/// Converts DSL literals into `HyperChad` literal token streams.
fn generate_literal_code(lit: &Literal) -> TokenStream {
    match lit {
        Literal::String(s) => quote! { hyperchad_actions::dsl::Literal::String(#s.to_string()) },
        Literal::Integer(i) => quote! { hyperchad_actions::dsl::Literal::Integer(#i) },
        Literal::Float(f) => quote! { hyperchad_actions::dsl::Literal::Float(#f) },
        Literal::Bool(b) => quote! { hyperchad_actions::dsl::Literal::Bool(#b) },
        Literal::Unit => quote! { hyperchad_actions::dsl::Literal::Unit },
    }
}

/// Generates Rust operator tokens for binary operations
///
/// Maps DSL binary operators to their Rust token equivalents.
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

/// Generates Rust operator tokens for unary operations
///
/// Maps DSL unary operators to their Rust token equivalents.
fn generate_unary_op_code(op: &UnaryOp) -> TokenStream {
    match op {
        UnaryOp::Not => quote! { ! },
        UnaryOp::Minus => quote! { - },
        UnaryOp::Plus => quote! { + },
        UnaryOp::Ref => quote! { & },
    }
}

/// Generates code for an expression, preferring direct literals when possible
///
/// For literal values, generates the literal directly. For other expressions,
/// generates full expression code.
///
/// # Errors
///
/// * Returns error if expression code generation fails
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

/// Generates code for method calls on various types
///
/// Handles method calls on element references, logic values, and arithmetic operations.
///
/// # Errors
///
/// * Returns error if method name is unknown
/// * Returns error if argument count is incorrect for the method
/// * Returns error if expression code generation fails
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

/// Transforms a closure for use in event handlers
///
/// Replaces closure parameters with `get_event_value()` calls to access event data.
///
/// # Errors
///
/// * Returns error if closure has more than one parameter
/// * Returns error if body transformation fails
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

/// Replaces parameter references in an expression with `get_event_value()` calls
///
/// Recursively transforms the expression tree, replacing all uses of the parameter
/// with runtime event value access.
///
/// # Errors
///
/// * Returns error if nested expression transformation fails
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

/// Replaces parameter references in a statement with `get_event_value()` calls
///
/// # Errors
///
/// * Returns error if expression or block transformation fails
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

/// Replaces parameter references in a block with `get_event_value()` calls
///
/// # Errors
///
/// * Returns error if statement transformation fails
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

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_actions::dsl::{BinaryOp, Expression, Literal, Pattern, Statement};

    #[test_log::test]
    fn test_context_default_creates_one_scope() {
        let context = Context::default();
        assert_eq!(context.scopes.len(), 1);
    }

    #[test_log::test]
    fn test_context_add_variable() {
        let mut context = Context::default();
        context.add_variable("x");
        assert!(context.is_variable_defined("x"));
    }

    #[test_log::test]
    fn test_context_is_variable_defined_returns_false_for_undefined() {
        let context = Context::default();
        assert!(!context.is_variable_defined("undefined_var"));
    }

    #[test_log::test]
    fn test_context_is_variable_defined_returns_true_after_adding() {
        let mut context = Context::default();
        context.add_variable("my_var");
        assert!(context.is_variable_defined("my_var"));
    }

    #[test_log::test]
    fn test_context_multiple_variables_in_same_scope() {
        let mut context = Context::default();
        context.add_variable("x");
        context.add_variable("y");
        context.add_variable("z");
        assert!(context.is_variable_defined("x"));
        assert!(context.is_variable_defined("y"));
        assert!(context.is_variable_defined("z"));
    }

    #[test_log::test]
    fn test_context_resolve_enum_variant_generates_correct_code() {
        let result = Context::resolve_enum_variant("Visibility", "Hidden");
        let result_str = result.to_string();
        assert!(result_str.contains("Visibility"));
        assert!(result_str.contains("Hidden"));
    }

    #[test_log::test]
    fn test_generate_pattern_code_for_literal_integer() {
        let pattern = Pattern::Literal(Literal::Integer(42));
        let result = generate_pattern_code(&pattern);
        let result_str = result.to_string();
        assert!(result_str.contains("42"));
    }

    #[test_log::test]
    fn test_generate_pattern_code_for_literal_string() {
        let pattern = Pattern::Literal(Literal::String("test".to_string()));
        let result = generate_pattern_code(&pattern);
        let result_str = result.to_string();
        assert!(result_str.contains("test"));
    }

    #[test_log::test]
    fn test_generate_pattern_code_for_variable() {
        let pattern = Pattern::Variable("x".to_string());
        let result = generate_pattern_code(&pattern);
        let result_str = result.to_string();
        assert_eq!(result_str, "x");
    }

    #[test_log::test]
    fn test_generate_pattern_code_for_wildcard() {
        let pattern = Pattern::Wildcard;
        let result = generate_pattern_code(&pattern);
        let result_str = result.to_string();
        assert_eq!(result_str, "_");
    }

    #[test_log::test]
    fn test_generate_pattern_code_for_enum_variant() {
        let pattern = Pattern::Variant {
            enum_name: "Option".to_string(),
            variant: "Some".to_string(),
            fields: vec![],
        };
        let result = generate_pattern_code(&pattern);
        let result_str = result.to_string();
        assert!(result_str.contains("Option"));
        assert!(result_str.contains("Some"));
    }

    #[test_log::test]
    fn test_generate_literal_code_for_integer() {
        let literal = Literal::Integer(123);
        let result = generate_literal_code(&literal);
        let result_str = result.to_string();
        assert!(result_str.contains("123i64"));
    }

    #[test_log::test]
    fn test_generate_literal_code_for_float() {
        let literal = Literal::Float(2.5);
        let result = generate_literal_code(&literal);
        let result_str = result.to_string();
        assert!(result_str.contains("2.5f64"));
    }

    #[test_log::test]
    fn test_generate_literal_code_for_string() {
        let literal = Literal::String("hello".to_string());
        let result = generate_literal_code(&literal);
        let result_str = result.to_string();
        assert!(result_str.contains("hello"));
        assert!(result_str.contains("to_string"));
    }

    #[test_log::test]
    fn test_generate_literal_code_for_bool_true() {
        let literal = Literal::Bool(true);
        let result = generate_literal_code(&literal);
        let result_str = result.to_string();
        assert!(result_str.contains("Literal :: Bool"));
        assert!(result_str.contains("true"));
    }

    #[test_log::test]
    fn test_generate_literal_code_for_bool_false() {
        let literal = Literal::Bool(false);
        let result = generate_literal_code(&literal);
        let result_str = result.to_string();
        assert!(result_str.contains("Literal :: Bool"));
        assert!(result_str.contains("false"));
    }

    #[test_log::test]
    fn test_generate_literal_code_for_unit() {
        let literal = Literal::Unit;
        let result = generate_literal_code(&literal);
        let result_str = result.to_string();
        assert!(result_str.contains("Literal :: Unit"));
    }

    #[test_log::test]
    fn test_generate_binary_op_code_add() {
        let result = generate_binary_op_code(&BinaryOp::Add);
        let result_str = result.to_string();
        assert_eq!(result_str, "+");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_subtract() {
        let result = generate_binary_op_code(&BinaryOp::Subtract);
        let result_str = result.to_string();
        assert_eq!(result_str, "-");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_multiply() {
        let result = generate_binary_op_code(&BinaryOp::Multiply);
        let result_str = result.to_string();
        assert_eq!(result_str, "*");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_divide() {
        let result = generate_binary_op_code(&BinaryOp::Divide);
        let result_str = result.to_string();
        assert_eq!(result_str, "/");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_modulo() {
        let result = generate_binary_op_code(&BinaryOp::Modulo);
        let result_str = result.to_string();
        assert_eq!(result_str, "%");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_and() {
        let result = generate_binary_op_code(&BinaryOp::And);
        let result_str = result.to_string();
        assert_eq!(result_str, "&&");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_or() {
        let result = generate_binary_op_code(&BinaryOp::Or);
        let result_str = result.to_string();
        assert_eq!(result_str, "||");
    }

    #[test_log::test]
    fn test_generate_unary_op_code_not() {
        let result = generate_unary_op_code(&UnaryOp::Not);
        let result_str = result.to_string();
        assert_eq!(result_str, "!");
    }

    #[test_log::test]
    fn test_generate_unary_op_code_minus() {
        let result = generate_unary_op_code(&UnaryOp::Minus);
        let result_str = result.to_string();
        assert_eq!(result_str, "-");
    }

    #[test_log::test]
    fn test_generate_unary_op_code_plus() {
        let result = generate_unary_op_code(&UnaryOp::Plus);
        let result_str = result.to_string();
        assert_eq!(result_str, "+");
    }

    #[test_log::test]
    fn test_generate_unary_op_code_ref() {
        let result = generate_unary_op_code(&UnaryOp::Ref);
        let result_str = result.to_string();
        assert_eq!(result_str, "&");
    }

    #[test_log::test]
    fn test_generate_expression_code_for_literal() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::Integer(42));
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hyperchad_actions :: dsl :: Expression :: Literal"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_undefined_variable() {
        let mut context = Context::default();
        let expr = Expression::Variable("undefined".to_string());
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert_eq!(result_str, "undefined");
    }

    #[test_log::test]
    fn test_generate_expression_code_for_defined_variable() {
        let mut context = Context::default();
        context.add_variable("my_var");
        let expr = Expression::Variable("my_var".to_string());
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ElementVariable"));
        assert!(result_str.contains("my_var"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_enum_variant() {
        let mut context = Context::default();
        let expr = Expression::Variable("Key::Escape".to_string());
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("Key"));
        assert!(result_str.contains("Escape"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_binary_add() {
        let mut context = Context::default();
        let left = Expression::Literal(Literal::Integer(1));
        let right = Expression::Literal(Literal::Integer(2));
        let expr = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Add,
            right: Box::new(right),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("plus"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_binary_subtract() {
        let mut context = Context::default();
        let left = Expression::Literal(Literal::Integer(5));
        let right = Expression::Literal(Literal::Integer(3));
        let expr = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Subtract,
            right: Box::new(right),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("minus"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_binary_multiply() {
        let mut context = Context::default();
        let left = Expression::Literal(Literal::Integer(3));
        let right = Expression::Literal(Literal::Integer(4));
        let expr = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Multiply,
            right: Box::new(right),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("multiply"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_binary_divide() {
        let mut context = Context::default();
        let left = Expression::Literal(Literal::Integer(10));
        let right = Expression::Literal(Literal::Integer(2));
        let expr = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Divide,
            right: Box::new(right),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("divide"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_binary_equal() {
        let mut context = Context::default();
        let left = Expression::Variable("x".to_string());
        let right = Expression::Variable("y".to_string());
        let expr = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Equal,
            right: Box::new(right),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hyperchad_actions :: logic :: eq"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_binary_equal_with_enum_variant() {
        let mut context = Context::default();
        let left = Expression::Variable("value".to_string());
        let right = Expression::Variable("Key::Escape".to_string());
        let expr = Expression::Binary {
            left: Box::new(left),
            op: BinaryOp::Equal,
            right: Box::new(right),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        // Enum variant should be wrapped in Value::from
        assert!(result_str.contains("hyperchad_actions :: logic :: Value :: from"));
        assert!(result_str.contains("Key"));
        assert!(result_str.contains("Escape"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_unary_not() {
        let mut context = Context::default();
        let inner = Expression::Literal(Literal::Bool(true));
        let expr = Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(inner),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains('!'));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_unary_minus() {
        let mut context = Context::default();
        let inner = Expression::Literal(Literal::Integer(42));
        let expr = Expression::Unary {
            op: UnaryOp::Minus,
            expr: Box::new(inner),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains('-'));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_grouping() {
        let mut context = Context::default();
        let inner = Expression::Binary {
            left: Box::new(Expression::Literal(Literal::Integer(1))),
            op: BinaryOp::Add,
            right: Box::new(Expression::Literal(Literal::Integer(2))),
        };
        let expr = Expression::Grouping(Box::new(inner));
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("Grouping"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_array() {
        let mut context = Context::default();
        let expr = Expression::Array(vec![
            Expression::Literal(Literal::Integer(1)),
            Expression::Literal(Literal::Integer(2)),
            Expression::Literal(Literal::Integer(3)),
        ]);
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("vec !"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_empty_array() {
        let mut context = Context::default();
        let expr = Expression::Array(vec![]);
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("vec !"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_tuple() {
        let mut context = Context::default();
        let expr = Expression::Tuple(vec![
            Expression::Literal(Literal::Integer(1)),
            Expression::Literal(Literal::String("test".to_string())),
        ]);
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains('('));
        assert!(result_str.contains(','));
        assert!(result_str.contains(')'));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_closure_with_params() {
        let mut context = Context::default();
        let expr = Expression::Closure {
            params: vec!["x".to_string(), "y".to_string()],
            body: Box::new(Expression::Variable("x".to_string())),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains('|'));
        assert!(result_str.contains('x'));
        assert!(result_str.contains('y'));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_raw_rust() {
        let mut context = Context::default();
        let expr = Expression::RawRust("some_function()".to_string());
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert_eq!(result_str, "some_function ()");
    }

    #[test_log::test]
    fn test_generate_expression_code_for_raw_rust_invalid() {
        let mut context = Context::default();
        let expr = Expression::RawRust("{{{{invalid".to_string());
        let result = generate_expression_code(&mut context, &expr);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_generate_statement_code_for_let() {
        let mut context = Context::default();
        let stmt = Statement::Let {
            name: "x".to_string(),
            value: Expression::Literal(Literal::Integer(42)),
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hyperchad_actions :: ActionType :: Let"));
        assert!(result_str.contains('x'));
    }

    #[test_log::test]
    fn test_generate_action_for_id_show_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "show", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("show_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_hide_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "hide", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hide_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_show_with_args_returns_error() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_action_for_id(&mut context, &id, "show", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_unknown_method_returns_error() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "unknown_method", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown method"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_show_no_args() {
        let mut context = Context::default();
        let result = generate_action_for_class(&mut context, "test-class", "show", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("show_str_class"));
        assert!(result_str.contains("test-class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_hide_no_args() {
        let mut context = Context::default();
        let result = generate_action_for_class(&mut context, "test-class", "hide", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hide_str_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_unknown_method_returns_error() {
        let mut context = Context::default();
        let result = generate_action_for_class(&mut context, "test-class", "unknown", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown method"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_toggle_visibility_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "toggle_visibility", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_visibility_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_toggle_visibility_with_args_returns_error() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_action_for_id(&mut context, &id, "toggle_visibility", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_set_visibility_with_correct_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let args = vec![Expression::Variable("Visibility::Hidden".to_string())];
        let result = generate_action_for_id(&mut context, &id, "set_visibility", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_visibility_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_set_visibility_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "set_visibility", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_focus_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "focus", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("focus_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_focus_with_args_returns_error() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_action_for_id(&mut context, &id, "focus", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_get_visibility_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "get_visibility", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_visibility_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_select_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "select", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("select_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_display_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("display_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_no_display_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "no_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("no_display_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_toggle_display_no_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "toggle_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_display_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_set_display_with_correct_args() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let args = vec![Expression::Literal(Literal::Bool(true))];
        let result = generate_action_for_id(&mut context, &id, "set_display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_display_str_id"));
    }

    #[test_log::test]
    fn test_generate_action_for_id_set_display_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let id = quote::quote! { "test-id" };
        let result = generate_action_for_id(&mut context, &id, "set_display", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_toggle_visibility_no_args() {
        let mut context = Context::default();
        let result =
            generate_action_for_class(&mut context, "test-class", "toggle_visibility", &[])
                .unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_visibility_str_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_set_visibility_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Variable("Visibility::Hidden".to_string())];
        let result =
            generate_action_for_class(&mut context, "test-class", "set_visibility", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_visibility_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_focus_no_args() {
        let mut context = Context::default();
        let result = generate_action_for_class(&mut context, "test-class", "focus", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("focus_str_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_get_visibility_no_args() {
        let mut context = Context::default();
        let result =
            generate_action_for_class(&mut context, "test-class", "get_visibility", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_visibility_str_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_select_no_args() {
        let mut context = Context::default();
        let result = generate_action_for_class(&mut context, "test-class", "select", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("select_str_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_display_no_args() {
        let mut context = Context::default();
        let result = generate_action_for_class(&mut context, "test-class", "display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("display_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_no_display_no_args() {
        let mut context = Context::default();
        let result =
            generate_action_for_class(&mut context, "test-class", "no_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("no_display_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_toggle_display_no_args() {
        let mut context = Context::default();
        let result =
            generate_action_for_class(&mut context, "test-class", "toggle_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_display_str_class"));
    }

    #[test_log::test]
    fn test_generate_action_for_class_set_display_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Bool(true))];
        let result =
            generate_action_for_class(&mut context, "test-class", "set_display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_display_class"));
    }

    #[test_log::test]
    fn test_context_scope_push_and_pop_isolates_variables() {
        let mut context = Context::default();
        context.add_variable("outer");

        // Manually push a new scope
        context.scopes.push_front(Scope::default());
        context.add_variable("inner");

        // Inner scope should see both variables
        assert!(context.is_variable_defined("outer"));
        assert!(context.is_variable_defined("inner"));

        // Pop the inner scope
        context.scopes.pop_front();

        // Should only see outer variable now
        assert!(context.is_variable_defined("outer"));
        assert!(!context.is_variable_defined("inner"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_range_with_start_and_end() {
        let mut context = Context::default();
        let expr = Expression::Range {
            start: Some(Box::new(Expression::Literal(Literal::Integer(0)))),
            end: Some(Box::new(Expression::Literal(Literal::Integer(10)))),
            inclusive: false,
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains(".."));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_range_inclusive() {
        let mut context = Context::default();
        let expr = Expression::Range {
            start: Some(Box::new(Expression::Literal(Literal::Integer(0)))),
            end: Some(Box::new(Expression::Literal(Literal::Integer(10)))),
            inclusive: true,
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("..="));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_range_without_start() {
        let mut context = Context::default();
        let expr = Expression::Range {
            start: None,
            end: Some(Box::new(Expression::Literal(Literal::Integer(10)))),
            inclusive: false,
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        // Should default to 0 for the start
        assert!(result_str.contains('0'));
        assert!(result_str.contains(".."));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_range_without_end_returns_error() {
        let mut context = Context::default();
        let expr = Expression::Range {
            start: Some(Box::new(Expression::Literal(Literal::Integer(0)))),
            end: None,
            inclusive: false,
        };
        let result = generate_expression_code(&mut context, &expr);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Range without end is not supported")
        );
    }

    #[test_log::test]
    fn test_generate_expression_code_for_field_access() {
        let mut context = Context::default();
        let expr = Expression::Field {
            object: Box::new(Expression::Variable("obj".to_string())),
            field: "my_field".to_string(),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("obj"));
        assert!(result_str.contains("my_field"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_if_with_else() {
        let mut context = Context::default();
        let expr = Expression::If {
            condition: Box::new(Expression::Literal(Literal::Bool(true))),
            then_branch: Box::new(Expression::Literal(Literal::Integer(1))),
            else_branch: Some(Box::new(Expression::Literal(Literal::Integer(2)))),
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("if"));
        assert!(result_str.contains("else"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_if_without_else() {
        let mut context = Context::default();
        let expr = Expression::If {
            condition: Box::new(Expression::Literal(Literal::Bool(true))),
            then_branch: Box::new(Expression::Literal(Literal::Integer(1))),
            else_branch: None,
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("if"));
        // Should have default else
        assert!(result_str.contains("else { () }"));
    }

    #[test_log::test]
    fn test_generate_binary_op_code_equal() {
        let result = generate_binary_op_code(&BinaryOp::Equal);
        let result_str = result.to_string();
        assert_eq!(result_str, "==");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_not_equal() {
        let result = generate_binary_op_code(&BinaryOp::NotEqual);
        let result_str = result.to_string();
        assert_eq!(result_str, "!=");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_less() {
        let result = generate_binary_op_code(&BinaryOp::Less);
        let result_str = result.to_string();
        assert_eq!(result_str, "<");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_less_equal() {
        let result = generate_binary_op_code(&BinaryOp::LessEqual);
        let result_str = result.to_string();
        assert_eq!(result_str, "<=");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_greater() {
        let result = generate_binary_op_code(&BinaryOp::Greater);
        let result_str = result.to_string();
        assert_eq!(result_str, ">");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_greater_equal() {
        let result = generate_binary_op_code(&BinaryOp::GreaterEqual);
        let result_str = result.to_string();
        assert_eq!(result_str, ">=");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_bit_and() {
        let result = generate_binary_op_code(&BinaryOp::BitAnd);
        let result_str = result.to_string();
        assert_eq!(result_str, "&");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_bit_or() {
        let result = generate_binary_op_code(&BinaryOp::BitOr);
        let result_str = result.to_string();
        assert_eq!(result_str, "|");
    }

    #[test_log::test]
    fn test_generate_binary_op_code_bit_xor() {
        let result = generate_binary_op_code(&BinaryOp::BitXor);
        let result_str = result.to_string();
        assert_eq!(result_str, "^");
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_replaces_variable() {
        let mut context = Context::default();
        let expr = Expression::Variable("value".to_string());
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::Call { function, args } => {
                assert_eq!(function, "get_event_value");
                assert!(args.is_empty());
            }
            _ => panic!("Expected Call expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_preserves_other_variables() {
        let mut context = Context::default();
        let expr = Expression::Variable("other".to_string());
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::Variable(name) => {
                assert_eq!(name, "other");
            }
            _ => panic!("Expected Variable expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_in_binary_expression() {
        let mut context = Context::default();
        let expr = Expression::Binary {
            left: Box::new(Expression::Variable("value".to_string())),
            op: BinaryOp::Equal,
            right: Box::new(Expression::Literal(Literal::String("test".to_string()))),
        };
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::Binary {
                left,
                op: _,
                right: _,
            } => match *left {
                Expression::Call { function, args } => {
                    assert_eq!(function, "get_event_value");
                    assert!(args.is_empty());
                }
                _ => panic!("Expected Call expression in left"),
            },
            _ => panic!("Expected Binary expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_in_call_args() {
        let mut context = Context::default();
        let expr = Expression::Call {
            function: "show".to_string(),
            args: vec![Expression::Variable("value".to_string())],
        };
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::Call { function, args } => {
                assert_eq!(function, "show");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expression::Call {
                        function: inner_fn, ..
                    } => {
                        assert_eq!(inner_fn, "get_event_value");
                    }
                    _ => panic!("Expected Call expression in args"),
                }
            }
            _ => panic!("Expected Call expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_in_raw_rust() {
        let mut context = Context::default();
        let expr = Expression::RawRust("value + 1".to_string());
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::RawRust(code) => {
                assert!(code.contains("get_event_value()"));
            }
            _ => panic!("Expected RawRust expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_preserves_literals() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::Integer(42));
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::Literal(Literal::Integer(n)) => {
                assert_eq!(n, 42);
            }
            _ => panic!("Expected Integer literal"),
        }
    }

    #[test_log::test]
    fn test_transform_closure_for_event_with_multiple_params_returns_error() {
        let mut context = Context::default();
        let params = vec!["x".to_string(), "y".to_string()];
        let body = Expression::Variable("x".to_string());
        let result = transform_closure_for_event(&mut context, &params, &body);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly one parameter"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_element_ref_with_string_literal() {
        let mut context = Context::default();
        let expr = Expression::ElementRef(Box::new(Expression::Literal(Literal::String(
            "#my-element".to_string(),
        ))));
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ElementRef"));
        assert!(result_str.contains("my-element"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_element_ref_with_variable() {
        let mut context = Context::default();
        let expr = Expression::ElementRef(Box::new(Expression::Variable("element_id".to_string())));
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ElementRef"));
        assert!(result_str.contains("element_id"));
    }

    #[test_log::test]
    fn test_generate_expression_code_for_element_ref_with_invalid_expr_returns_error() {
        let mut context = Context::default();
        let expr = Expression::ElementRef(Box::new(Expression::Literal(Literal::Integer(42))));
        let result = generate_expression_code(&mut context, &expr);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid element selector"));
    }

    #[test_log::test]
    fn test_literal_or_stmt_with_integer() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::Integer(42));
        let result = literal_or_stmt(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert_eq!(result_str, "42i64");
    }

    #[test_log::test]
    fn test_literal_or_stmt_with_float() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::Float(2.5));
        let result = literal_or_stmt(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("2.5"));
    }

    #[test_log::test]
    fn test_literal_or_stmt_with_bool() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::Bool(true));
        let result = literal_or_stmt(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert_eq!(result_str, "true");
    }

    #[test_log::test]
    fn test_literal_or_stmt_with_string() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::String("hello".to_string()));
        let result = literal_or_stmt(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hello"));
    }

    #[test_log::test]
    fn test_literal_or_stmt_with_complex_expr_uses_generate_expression_code() {
        let mut context = Context::default();
        let expr = Expression::Variable("my_var".to_string());
        let result = literal_or_stmt(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert_eq!(result_str, "my_var");
    }

    #[test_log::test]
    fn test_generate_expression_code_for_match_returns_noop() {
        let mut context = Context::default();
        let expr = Expression::Match {
            expr: Box::new(Expression::Variable("x".to_string())),
            arms: vec![],
        };
        let result = generate_expression_code(&mut context, &expr).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: NoOp"));
    }

    #[test_log::test]
    fn test_generate_expression_code_binary_not_equal_returns_error() {
        let mut context = Context::default();
        let expr = Expression::Binary {
            left: Box::new(Expression::Variable("x".to_string())),
            op: BinaryOp::NotEqual,
            right: Box::new(Expression::Variable("y".to_string())),
        };
        let result = generate_expression_code(&mut context, &expr);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("NotEqual operation is not yet supported")
        );
    }

    #[test_log::test]
    fn test_generate_expression_code_binary_greater_returns_error() {
        let mut context = Context::default();
        let expr = Expression::Binary {
            left: Box::new(Expression::Variable("x".to_string())),
            op: BinaryOp::Greater,
            right: Box::new(Expression::Variable("y".to_string())),
        };
        let result = generate_expression_code(&mut context, &expr);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Greater than operation is not yet supported")
        );
    }

    #[test_log::test]
    fn test_generate_statement_code_for_if_with_empty_then_block_and_true_condition() {
        let mut context = Context::default();
        let stmt = Statement::If {
            condition: Expression::Literal(Literal::Bool(true)),
            then_block: Block { statements: vec![] },
            else_block: None,
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        // Empty then block with true condition should return NoOp
        assert!(result_str.contains("ActionEffect :: NoOp"));
    }

    #[test_log::test]
    fn test_generate_statement_code_for_if_with_empty_else_block_and_false_condition() {
        let mut context = Context::default();
        let stmt = Statement::If {
            condition: Expression::Literal(Literal::Bool(false)),
            then_block: Block {
                statements: vec![Statement::Expression(Expression::Call {
                    function: "show".to_string(),
                    args: vec![Expression::Literal(Literal::String("test".to_string()))],
                })],
            },
            else_block: Some(Block { statements: vec![] }),
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        // With false condition, should execute else block which is empty -> NoOp
        assert!(result_str.contains("ActionEffect :: NoOp"));
    }

    #[test_log::test]
    fn test_target_to_expr_with_defined_variable() {
        let mut context = Context::default();
        context.add_variable("my_target");
        let expr = Expression::Variable("my_target".to_string());
        let result = target_to_expr(&mut context, &expr, false).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("Target :: reference"));
    }

    #[test_log::test]
    fn test_target_to_expr_with_string_literal() {
        let mut context = Context::default();
        let expr = Expression::Literal(Literal::String("element-id".to_string()));
        let result = target_to_expr(&mut context, &expr, false).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("Target :: literal"));
        assert!(result_str.contains("element-id"));
    }

    #[test_log::test]
    fn test_target_to_expr_with_into_true() {
        let mut context = Context::default();
        let expr = Expression::Variable("some_var".to_string());
        let result = target_to_expr(&mut context, &expr, true).unwrap();
        let result_str = result.to_string();
        // Test that the result contains `.into()` call
        assert!(
            result_str.contains("into"),
            "Expected 'into' in: {result_str}"
        );
    }

    #[test_log::test]
    fn test_generate_method_call_code_divide_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Literal(Literal::Integer(2))];
        let result = generate_method_call_code(&mut context, &receiver, "divide", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("divide"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_divide_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let result = generate_method_call_code(&mut context, &receiver, "divide", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_clamp_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![
            Expression::Literal(Literal::Integer(0)),
            Expression::Literal(Literal::Integer(100)),
        ];
        let result = generate_method_call_code(&mut context, &receiver, "clamp", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("clamp"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_clamp_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Literal(Literal::Integer(0))];
        let result = generate_method_call_code(&mut context, &receiver, "clamp", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 2 arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_delay_off_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { action };
        let args = vec![Expression::Literal(Literal::Integer(1000))];
        let result =
            generate_method_call_code(&mut context, &receiver, "delay_off", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("delay_off"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_throttle_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { action };
        let args = vec![Expression::Literal(Literal::Integer(30))];
        let result = generate_method_call_code(&mut context, &receiver, "throttle", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("throttle"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_unknown_method_passes_through() {
        let mut context = Context::default();
        let receiver = quote::quote! { obj };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result =
            generate_method_call_code(&mut context, &receiver, "custom_method", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("custom_method"));
    }

    // Tests for generate_function_call_code - DSL function handlers

    #[test_log::test]
    fn test_generate_function_call_code_hide_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("element".to_string()))];
        let result = generate_function_call_code(&mut context, "hide", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Style"));
        assert!(result_str.contains("SetVisibility"));
        assert!(result_str.contains("Hidden"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_hide_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "hide", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_show_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("modal".to_string()))];
        let result = generate_function_call_code(&mut context, "show", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Style"));
        assert!(result_str.contains("SetVisibility"));
        assert!(result_str.contains("Visible"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_show_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "show", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_visibility_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::String("elem".to_string())),
            Expression::Variable("Visibility::Hidden".to_string()),
        ];
        let result = generate_function_call_code(&mut context, "set_visibility", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Style"));
        assert!(result_str.contains("SetVisibility"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_visibility_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "set_visibility", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 2 arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_display_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("SetDisplay (true)"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_display_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "display", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_no_display_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "no_display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("SetDisplay (false)"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_no_display_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "no_display", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_display_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::String("elem".to_string())),
            Expression::Literal(Literal::Bool(true)),
        ];
        let result = generate_function_call_code(&mut context, "set_display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("SetDisplay"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_display_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "set_display", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 2 arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_toggle_display_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "toggle_display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_display_str_id"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_toggle_display_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "toggle_display", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_background_self_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("#333".to_string()))];
        let result =
            generate_function_call_code(&mut context, "set_background_self", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_background_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_background_self_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "set_background_self", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_visibility_child_class_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Variable("Visibility::Hidden".to_string()),
            Expression::Literal(Literal::String("child".to_string())),
        ];
        let result =
            generate_function_call_code(&mut context, "set_visibility_child_class", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ChildClass"));
        assert!(result_str.contains("SetVisibility"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_set_visibility_child_class_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Variable("Visibility::Hidden".to_string())];
        let result = generate_function_call_code(&mut context, "set_visibility_child_class", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 2 arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_visibility_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "get_visibility", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("CalcValue :: Visibility"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_visibility_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_visibility", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_width_px_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_width_px_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_width_px_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_width_px_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "get_width_px_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_mouse_x_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_mouse_x", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_mouse_x"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_mouse_x_with_one_arg() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result = generate_function_call_code(&mut context, "get_mouse_x", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_mouse_x_str_id"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_mouse_x_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::Integer(1)),
            Expression::Literal(Literal::Integer(2)),
        ];
        let result = generate_function_call_code(&mut context, "get_mouse_x", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects 0 or 1 arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_mouse_x_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_mouse_x_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_mouse_x_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_mouse_x_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "get_mouse_x_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_data_attr_value_self_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("attr".to_string()))];
        let result =
            generate_function_call_code(&mut context, "get_data_attr_value_self", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_data_attr_value_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_data_attr_value_self_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_data_attr_value_self", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_noop_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "noop", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: NoOp"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_log_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("message".to_string()))];
        let result = generate_function_call_code(&mut context, "log", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Log"));
        assert!(result_str.contains("Info"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_log_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "log", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_navigate_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("/home".to_string()))];
        let result = generate_function_call_code(&mut context, "navigate", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Navigate"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_navigate_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "navigate", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_custom_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String(
            "custom_action".to_string(),
        ))];
        let result = generate_function_call_code(&mut context, "custom", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Custom"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_custom_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "custom", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_visible() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "visible", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("logic :: visible"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_hidden() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "hidden", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("logic :: hidden"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_displayed() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "displayed", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("logic :: displayed"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_not_displayed() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "not_displayed", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("logic :: not_displayed"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_eq_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Variable("a".to_string()),
            Expression::Variable("b".to_string()),
        ];
        let result = generate_function_call_code(&mut context, "eq", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("logic :: eq"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_eq_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Variable("a".to_string())];
        let result = generate_function_call_code(&mut context, "eq", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 2 arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_invoke_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Variable("Action::Test".to_string()),
            Expression::Literal(Literal::String("value".to_string())),
        ];
        let result = generate_function_call_code(&mut context, "invoke", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Parameterized"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_invoke_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Variable("Action::Test".to_string())];
        let result = generate_function_call_code(&mut context, "invoke", &args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 2 arguments (action, value)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_throttle_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::Integer(30)),
            Expression::Call {
                function: "noop".to_string(),
                args: vec![],
            },
        ];
        let result = generate_function_call_code(&mut context, "throttle", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionEffect"));
        assert!(result_str.contains("throttle"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_throttle_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(30))];
        let result = generate_function_call_code(&mut context, "throttle", &args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 2 arguments (duration, action)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_delay_off_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::Integer(1000)),
            Expression::Call {
                function: "noop".to_string(),
                args: vec![],
            },
        ];
        let result = generate_function_call_code(&mut context, "delay_off", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionEffect"));
        assert!(result_str.contains("delay_off"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_delay_off_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1000))];
        let result = generate_function_call_code(&mut context, "delay_off", &args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 2 arguments (duration, action)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_unique_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Call {
            function: "noop".to_string(),
            args: vec![],
        }];
        let result = generate_function_call_code(&mut context, "unique", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionEffect"));
        assert!(result_str.contains("unique"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_unique_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "unique", &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 1 argument (action)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_clamp_with_correct_args() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::Float(0.0)),
            Expression::Variable("value".to_string()),
            Expression::Literal(Literal::Float(1.0)),
        ];
        let result = generate_function_call_code(&mut context, "clamp", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("clamp"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_clamp_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::Float(0.0)),
            Expression::Variable("value".to_string()),
        ];
        let result = generate_function_call_code(&mut context, "clamp", &args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 3 arguments (min, value, max)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_group_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Variable("expr".to_string())];
        let result = generate_function_call_code(&mut context, "group", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("Arithmetic :: Grouping"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_group_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "group", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_on_event_with_closure() {
        let mut context = Context::default();
        let closure = Expression::Closure {
            params: vec!["value".to_string()],
            body: Box::new(Expression::Call {
                function: "show".to_string(),
                args: vec![Expression::Variable("value".to_string())],
            }),
        };
        let args = vec![
            Expression::Literal(Literal::String("click".to_string())),
            closure,
        ];
        let result = generate_function_call_code(&mut context, "on_event", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Event"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_on_event_without_closure() {
        let mut context = Context::default();
        let args = vec![
            Expression::Literal(Literal::String("click".to_string())),
            Expression::Call {
                function: "noop".to_string(),
                args: vec![],
            },
        ];
        let result = generate_function_call_code(&mut context, "on_event", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: Event"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_on_event_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("click".to_string()))];
        let result = generate_function_call_code(&mut context, "on_event", &args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 2 arguments (event_name, closure)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_show_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "show_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("show_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_show_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "show_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_hide_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "hide_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hide_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_hide_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "hide_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_display_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "display_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("display_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_display_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "display_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_no_display_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "no_display_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("no_display_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_no_display_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "no_display_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_self_no_args() {
        let mut context = Context::default();
        let result =
            generate_function_call_code(&mut context, "remove_background_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("remove_background_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "remove_background_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_event_value_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_event_value", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_event_value"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_event_value_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "get_event_value", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_visibility_self_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "get_visibility_self", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_visibility_self"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_get_visibility_self_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "get_visibility_self", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_show_last_child_no_args() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "show_last_child", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("show_last_child"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_show_last_child_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_function_call_code(&mut context, "show_last_child", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_str_id_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("elem".to_string()))];
        let result =
            generate_function_call_code(&mut context, "remove_background_str_id", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("remove_background_str_id"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_str_id_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "remove_background_str_id", &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 1 argument (target)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_id_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(42))];
        let result =
            generate_function_call_code(&mut context, "remove_background_id", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("remove_background_id"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_id_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "remove_background_id", &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 1 argument (target)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_class_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("my-class".to_string()))];
        let result =
            generate_function_call_code(&mut context, "remove_background_class", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("remove_background_class"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_class_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "remove_background_class", &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 1 argument (class_name)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_child_class_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("child".to_string()))];
        let result =
            generate_function_call_code(&mut context, "remove_background_child_class", &args)
                .unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("remove_background_child_class"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_child_class_wrong_arg_count_returns_error()
     {
        let mut context = Context::default();
        let result =
            generate_function_call_code(&mut context, "remove_background_child_class", &[]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expects exactly 1 argument (class_name)")
        );
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_last_child_no_args() {
        let mut context = Context::default();
        let result =
            generate_function_call_code(&mut context, "remove_background_last_child", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("remove_background_last_child"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_remove_background_last_child_with_args_returns_error() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result =
            generate_function_call_code(&mut context, "remove_background_last_child", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_element_with_correct_args() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::String("#my-id".to_string()))];
        let result = generate_function_call_code(&mut context, "element", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ElementRef"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_element_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let result = generate_function_call_code(&mut context, "element", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_function_call_code_unknown_function_passes_through() {
        let mut context = Context::default();
        let args = vec![Expression::Literal(Literal::Integer(42))];
        let result = generate_function_call_code(&mut context, "unknown_function", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("unknown_function"));
    }

    // Tests for generate_method_call_code - element and logic methods

    #[test_log::test]
    fn test_generate_method_call_code_show_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "show", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("show"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_show_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "show", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_hide_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "hide", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("hide"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_hide_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "hide", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_toggle_visibility_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "toggle_visibility", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_visibility"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_toggle_visibility_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "toggle_visibility", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_visibility_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "visibility", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("visibility"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_visibility_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "visibility", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_visibility_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "get_visibility", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_visibility"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_visibility_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "get_visibility", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_set_visibility_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Variable("Visibility::Hidden".to_string())];
        let result =
            generate_method_call_code(&mut context, &receiver, "set_visibility", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_visibility"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_set_visibility_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "set_visibility", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_width_px_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "get_width_px", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_width_px"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_width_px_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "get_width_px", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_height_px_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "get_height_px", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_height_px"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_height_px_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "get_height_px", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_mouse_x_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "get_mouse_x", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_mouse_x"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_mouse_x_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "get_mouse_x", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_mouse_y_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "get_mouse_y", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_mouse_y"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_mouse_y_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "get_mouse_y", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_display_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("display"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_display_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "display", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_no_display_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "no_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("no_display"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_no_display_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "no_display", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_toggle_display_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "toggle_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("toggle_display"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_toggle_display_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "toggle_display", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_set_display_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Bool(true))];
        let result =
            generate_method_call_code(&mut context, &receiver, "set_display", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("set_display"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_set_display_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result = generate_method_call_code(&mut context, &receiver, "set_display", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_display_no_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let result =
            generate_method_call_code(&mut context, &receiver, "get_display", &[]).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("get_display"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_get_display_with_args_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { element };
        let args = vec![Expression::Literal(Literal::Integer(1))];
        let result = generate_method_call_code(&mut context, &receiver, "get_display", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects no arguments"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_eq_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Literal(Literal::Integer(42))];
        let result = generate_method_call_code(&mut context, &receiver, "eq", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("eq"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_eq_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let result = generate_method_call_code(&mut context, &receiver, "eq", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_then_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { condition };
        let args = vec![Expression::Call {
            function: "noop".to_string(),
            args: vec![],
        }];
        let result = generate_method_call_code(&mut context, &receiver, "then", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("then"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_then_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { condition };
        let result = generate_method_call_code(&mut context, &receiver, "then", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_or_else_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { condition };
        let args = vec![Expression::Call {
            function: "noop".to_string(),
            args: vec![],
        }];
        let result = generate_method_call_code(&mut context, &receiver, "or_else", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("or_else"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_or_else_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { condition };
        let result = generate_method_call_code(&mut context, &receiver, "or_else", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_and_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { condition };
        let args = vec![Expression::Call {
            function: "noop".to_string(),
            args: vec![],
        }];
        let result = generate_method_call_code(&mut context, &receiver, "and", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("and"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_and_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { condition };
        let result = generate_method_call_code(&mut context, &receiver, "and", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_then_pass_to_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Variable("Action::Test".to_string())];
        let result =
            generate_method_call_code(&mut context, &receiver, "then_pass_to", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("then_pass_to"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_then_pass_to_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let result = generate_method_call_code(&mut context, &receiver, "then_pass_to", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_minus_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Literal(Literal::Integer(5))];
        let result = generate_method_call_code(&mut context, &receiver, "minus", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("minus"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_minus_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let result = generate_method_call_code(&mut context, &receiver, "minus", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_plus_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Literal(Literal::Integer(5))];
        let result = generate_method_call_code(&mut context, &receiver, "plus", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("plus"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_plus_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let result = generate_method_call_code(&mut context, &receiver, "plus", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_multiply_with_correct_args() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let args = vec![Expression::Literal(Literal::Integer(3))];
        let result = generate_method_call_code(&mut context, &receiver, "multiply", &args).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("multiply"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_multiply_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { value };
        let result = generate_method_call_code(&mut context, &receiver, "multiply", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_delay_off_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { action };
        let result = generate_method_call_code(&mut context, &receiver, "delay_off", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    #[test_log::test]
    fn test_generate_method_call_code_throttle_wrong_arg_count_returns_error() {
        let mut context = Context::default();
        let receiver = quote::quote! { action };
        let result = generate_method_call_code(&mut context, &receiver, "throttle", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects exactly 1 argument"));
    }

    // Tests for statement code generation

    #[test_log::test]
    fn test_generate_statement_code_for_for_loop() {
        let mut context = Context::default();
        let stmt = Statement::For {
            pattern: "item".to_string(),
            iter: Expression::Variable("items".to_string()),
            body: Block {
                statements: vec![Statement::Expression(Expression::Call {
                    function: "log".to_string(),
                    args: vec![Expression::Variable("item".to_string())],
                })],
            },
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("for"));
        assert!(result_str.contains("item"));
        assert!(result_str.contains("items"));
    }

    #[test_log::test]
    fn test_generate_statement_code_for_while_loop() {
        let mut context = Context::default();
        let stmt = Statement::While {
            condition: Expression::Literal(Literal::Bool(true)),
            body: Block {
                statements: vec![Statement::Expression(Expression::Call {
                    function: "log".to_string(),
                    args: vec![Expression::Literal(Literal::String("test".to_string()))],
                })],
            },
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("while"));
    }

    #[test_log::test]
    fn test_generate_statement_code_for_match() {
        let mut context = Context::default();
        let stmt = Statement::Match {
            expr: Expression::Variable("value".to_string()),
            arms: vec![hyperchad_actions::dsl::MatchArm {
                pattern: Pattern::Wildcard,
                body: Expression::Call {
                    function: "noop".to_string(),
                    args: vec![],
                },
            }],
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("match"));
        assert!(result_str.contains('_'));
    }

    #[test_log::test]
    fn test_generate_statement_code_for_block() {
        let mut context = Context::default();
        let stmt = Statement::Block(Block {
            statements: vec![Statement::Expression(Expression::Call {
                function: "noop".to_string(),
                args: vec![],
            })],
        });
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: NoOp"));
    }

    #[test_log::test]
    fn test_generate_statement_code_for_if_with_single_then_statement_and_true_condition() {
        let mut context = Context::default();
        let stmt = Statement::If {
            condition: Expression::Literal(Literal::Bool(true)),
            then_block: Block {
                statements: vec![Statement::Expression(Expression::Call {
                    function: "show".to_string(),
                    args: vec![Expression::Literal(Literal::String("elem".to_string()))],
                })],
            },
            else_block: None,
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        // With true condition and single statement, it should return the action directly
        assert!(result_str.contains("Style"));
    }

    #[test_log::test]
    fn test_generate_statement_code_for_if_with_non_literal_condition() {
        let mut context = Context::default();
        let stmt = Statement::If {
            condition: Expression::Binary {
                left: Box::new(Expression::Variable("a".to_string())),
                op: BinaryOp::Equal,
                right: Box::new(Expression::Variable("b".to_string())),
            },
            then_block: Block {
                statements: vec![Statement::Expression(Expression::Call {
                    function: "show".to_string(),
                    args: vec![Expression::Literal(Literal::String("elem".to_string()))],
                })],
            },
            else_block: Some(Block {
                statements: vec![Statement::Expression(Expression::Call {
                    function: "hide".to_string(),
                    args: vec![Expression::Literal(Literal::String("elem".to_string()))],
                })],
            }),
        };
        let result = generate_statement_code(&mut context, &stmt).unwrap();
        let result_str = result.to_string();
        // Should generate Logic If with condition
        assert!(result_str.contains("ActionType :: Logic"));
    }

    // Tests for replace_param functions with different expression types

    #[test_log::test]
    fn test_replace_param_with_get_event_value_in_if_expression() {
        let mut context = Context::default();
        let expr = Expression::If {
            condition: Box::new(Expression::Variable("value".to_string())),
            then_branch: Box::new(Expression::Literal(Literal::Integer(1))),
            else_branch: Some(Box::new(Expression::Literal(Literal::Integer(2)))),
        };
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::If { condition, .. } => match *condition {
                Expression::Call { function, .. } => {
                    assert_eq!(function, "get_event_value");
                }
                _ => panic!("Expected Call in condition"),
            },
            _ => panic!("Expected If expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_with_get_event_value_in_method_call() {
        let mut context = Context::default();
        let expr = Expression::MethodCall {
            receiver: Box::new(Expression::Variable("value".to_string())),
            method: "to_string".to_string(),
            args: vec![],
        };
        let result = replace_param_with_get_event_value(&mut context, &expr, "value").unwrap();
        match result {
            Expression::MethodCall { receiver, .. } => match *receiver {
                Expression::Call { function, .. } => {
                    assert_eq!(function, "get_event_value");
                }
                _ => panic!("Expected Call in receiver"),
            },
            _ => panic!("Expected MethodCall expression"),
        }
    }

    #[test_log::test]
    fn test_replace_param_in_statement_for_if() {
        let mut context = Context::default();
        let stmt = Statement::If {
            condition: Expression::Variable("value".to_string()),
            then_block: Block {
                statements: vec![Statement::Expression(Expression::Variable(
                    "value".to_string(),
                ))],
            },
            else_block: None,
        };
        let result = replace_param_in_statement(&mut context, &stmt, "value").unwrap();
        match result {
            Statement::If { condition, .. } => match condition {
                Expression::Call { function, .. } => {
                    assert_eq!(function, "get_event_value");
                }
                _ => panic!("Expected Call in condition"),
            },
            _ => panic!("Expected If statement"),
        }
    }

    #[test_log::test]
    fn test_replace_param_in_statement_for_let() {
        let mut context = Context::default();
        let stmt = Statement::Let {
            name: "x".to_string(),
            value: Expression::Variable("value".to_string()),
        };
        let result = replace_param_in_statement(&mut context, &stmt, "value").unwrap();
        match result {
            Statement::Let { name, value } => {
                assert_eq!(name, "x");
                match value {
                    Expression::Call { function, .. } => {
                        assert_eq!(function, "get_event_value");
                    }
                    _ => panic!("Expected Call in value"),
                }
            }
            _ => panic!("Expected Let statement"),
        }
    }

    #[test_log::test]
    fn test_replace_param_in_statement_for_block() {
        let mut context = Context::default();
        let stmt = Statement::Block(Block {
            statements: vec![Statement::Expression(Expression::Variable(
                "value".to_string(),
            ))],
        });
        let result = replace_param_in_statement(&mut context, &stmt, "value").unwrap();
        match result {
            Statement::Block(block) => {
                assert_eq!(block.statements.len(), 1);
                match &block.statements[0] {
                    Statement::Expression(Expression::Call { function, .. }) => {
                        assert_eq!(function, "get_event_value");
                    }
                    _ => panic!("Expected Call in block statement"),
                }
            }
            _ => panic!("Expected Block statement"),
        }
    }

    #[test_log::test]
    fn test_replace_param_in_statement_preserves_other_statements() {
        let mut context = Context::default();
        let stmt = Statement::For {
            pattern: "i".to_string(),
            iter: Expression::Variable("items".to_string()),
            body: Block { statements: vec![] },
        };
        let result = replace_param_in_statement(&mut context, &stmt, "value").unwrap();
        // For statements are returned as-is
        match result {
            Statement::For { pattern, .. } => {
                assert_eq!(pattern, "i");
            }
            _ => panic!("Expected For statement"),
        }
    }
}
