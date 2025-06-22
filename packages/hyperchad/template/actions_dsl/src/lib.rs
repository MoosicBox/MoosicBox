#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

extern crate proc_macro;

mod evaluator;
mod parser;

use hyperchad_actions::dsl::{Dsl, Expression, Statement};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;

/// Main proc-macro for parsing actions DSL syntax
///
/// This macro allows you to write Rust-like syntax for defining actions:
///
/// ```ignore
/// actions_dsl! {
///     if get_visibility("modal") == Visibility::Hidden {
///         show("modal");
///         log("Modal shown");
///     } else {
///         hide("modal");
///     }
///
///     // Parameterized actions with parameters
///     invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
///     invoke(Action::RefreshVisualization, get_width_px_self());
///
///     // Function forms for common operations
///     throttle(30, invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0)));
///
///     // Event handling with closures
///     on_event("play-track", |value| {
///         if value == get_data_attr_value_self("track-id") {
///             set_background_self("#333");
///             set_visibility_child_class(Visibility::Hidden, "track-number");
///             set_visibility_child_class(Visibility::Visible, "track-playing");
///         } else {
///             remove_background_self();
///             set_visibility_child_class(Visibility::Visible, "track-number");
///         }
///     });
///
///     // Background removal functions
///     remove_background_self();
///     remove_background_str_id("element-id");
///     remove_background_id(123);
///     remove_background_class("my-class");
///     remove_background_child_class("child-class");
///     remove_background_last_child();
/// }
/// ```
#[proc_macro]
pub fn actions_dsl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input2 = proc_macro2::TokenStream::from(input);

    match expand_actions_dsl(&input2) {
        Ok(tokens) => tokens.into(),
        Err(error_msg) => quote! {
            compile_error!(#error_msg)
        }
        .into(),
    }
}

/// Internal proc-macro for generating single `ActionEffect` with compile-time optimizations
/// This is used by the template system and returns `ActionEffect` instead of Vec<ActionEffect>
#[proc_macro]
pub fn actions_dsl_single(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input2 = proc_macro2::TokenStream::from(input);

    match expand_actions_dsl_single(&input2) {
        Ok(tokens) => tokens.into(),
        Err(error_msg) => quote! {
            compile_error!(#error_msg)
        }
        .into(),
    }
}

fn expand_actions_dsl(input: &TokenStream) -> Result<TokenStream, String> {
    // Parse the input tokens into our DSL AST
    let dsl = match Parser::parse2(parser::parse_dsl, input.clone()) {
        Ok(dsl) => dsl,
        Err(_e) => {
            // Try fallback parsing with raw Rust code
            parser::parse_dsl_with_fallback(input)
        }
    };

    // Generate completely compile-time optimized code - ZERO runtime logic
    generate_pure_compile_time_dsl(&dsl)
}

// Generate completely compile-time optimized DSL with ZERO runtime logic
fn generate_pure_compile_time_dsl(dsl: &Dsl) -> Result<TokenStream, String> {
    // Count action-producing statements at compile time
    let action_count = dsl.statements.len();

    // Process let statements for variable bindings
    let mut variable_bindings = Vec::new();
    for stmt in &dsl.statements {
        if let Statement::Let { name, value } = stmt {
            let var_name = quote::format_ident!("{}", name);
            let value_code = evaluator::generate_expression_code(value)?;
            variable_bindings.push(quote! {
                let #var_name = #value_code;
            });
        }
    }

    // Generate exact code based on compile-time determined action count
    let result = match action_count {
        0 => {
            // ZERO actions - direct empty vec![] at compile time
            quote! {{
                vec![] as Vec<hyperchad_actions::ActionEffect>
            }}
        }
        1 => {
            // SINGLE action - direct vec![] with one element at compile time
            let action_effect = generate_single_action_effect(&dsl.statements)?;
            quote! {{
                vec![#action_effect] as Vec<hyperchad_actions::ActionEffect>
            }}
        }
        _ => {
            // MULTIPLE actions - direct vec![] with all elements at compile time
            let action_effects = generate_multiple_action_effects(&dsl.statements)?;
            quote! {{
                vec![#(#action_effects),*] as Vec<hyperchad_actions::ActionEffect>
            }}
        }
    };

    // Combine variable bindings with the result
    if variable_bindings.is_empty() {
        Ok(result)
    } else {
        Ok(quote! {{
            #(#variable_bindings)*
            #result
        }})
    }
}

// Generate single action effect - called only when we know there's exactly one action
fn generate_single_action_effect(statements: &[Statement]) -> Result<TokenStream, String> {
    if let Some(stmt) = statements.iter().next() {
        return generate_action_effect_from_statement(stmt);
    }
    unreachable!("generate_single_action_effect called when no action exists")
}

// Generate multiple action effects - called only when we know there are multiple actions
fn generate_multiple_action_effects(statements: &[Statement]) -> Result<Vec<TokenStream>, String> {
    statements
        .iter()
        .map(generate_action_effect_from_statement)
        .collect::<Result<Vec<_>, _>>()
}

// Generate action effect from a single statement
fn generate_action_effect_from_statement(stmt: &Statement) -> Result<TokenStream, String> {
    match stmt {
        Statement::Expression(expr) => {
            let action_code = evaluator::generate_expression_code(expr)?;
            Ok(quote! {
                (#action_code).into()
            })
        }
        Statement::If {
            condition,
            then_block,
            else_block,
        } => generate_pure_compile_time_if_action(condition, then_block, else_block.as_ref()),
        _ => Ok(quote! {
            hyperchad_actions::ActionType::NoOp.into()
        }),
    }
}

// Check if a statement produces an action - pure compile-time check
// Generate pure compile-time if action with ZERO runtime logic
#[allow(clippy::too_many_lines)]
fn generate_pure_compile_time_if_action(
    condition: &Expression,
    then_block: &hyperchad_actions::dsl::Block,
    else_block: Option<&hyperchad_actions::dsl::Block>,
) -> Result<TokenStream, String> {
    // Handle boolean literals by converting them to Condition types
    let condition_code = match condition {
        Expression::Literal(hyperchad_actions::dsl::Literal::Bool(true)) => {
            // true becomes eq(visible(), visible()) - always true
            quote! { hyperchad_actions::logic::eq(hyperchad_actions::logic::visible(), hyperchad_actions::logic::visible()) }
        }
        Expression::Literal(hyperchad_actions::dsl::Literal::Bool(false)) => {
            // false becomes eq(visible(), hidden()) - always false
            quote! { hyperchad_actions::logic::eq(hyperchad_actions::logic::visible(), hyperchad_actions::logic::hidden()) }
        }
        _ => {
            // Regular condition expression
            evaluator::generate_expression_code(condition)?
        }
    };

    // Extract action effects from then block at compile time
    let mut then_effects = Vec::new();
    let mut then_variable_bindings = Vec::new();
    for stmt in &then_block.statements {
        match stmt {
            Statement::Expression(expr) if evaluator::expression_produces_action(expr) => {
                let action_code = evaluator::generate_expression_code(expr)?;
                then_effects.push(quote! {
                    (#action_code).into()
                });
            }
            Statement::If {
                condition: if_cond,
                then_block: if_then,
                else_block: if_else,
            } => {
                let nested_if =
                    generate_pure_compile_time_if_action(if_cond, if_then, if_else.as_ref())?;
                then_effects.push(nested_if);
            }
            Statement::Let { name, value } => {
                // Handle scoped variables within if blocks
                let var_name = quote::format_ident!("{}", name);
                let value_code = evaluator::generate_expression_code(value)?;
                then_variable_bindings.push(quote! {
                    let #var_name = #value_code;
                });
            }
            _ => {} // Ignore other non-action statements
        }
    }

    // Extract action effects from else block at compile time
    let mut else_effects = Vec::new();
    let mut else_variable_bindings = Vec::new();
    if let Some(else_block) = else_block {
        for stmt in &else_block.statements {
            match stmt {
                Statement::Expression(expr) if evaluator::expression_produces_action(expr) => {
                    let action_code = evaluator::generate_expression_code(expr)?;
                    else_effects.push(quote! {
                        (#action_code).into()
                    });
                }
                Statement::If {
                    condition: if_cond,
                    then_block: if_then,
                    else_block: if_else,
                } => {
                    let nested_if =
                        generate_pure_compile_time_if_action(if_cond, if_then, if_else.as_ref())?;
                    else_effects.push(nested_if);
                }
                Statement::Let { name, value } => {
                    // Handle scoped variables within else blocks
                    let var_name = quote::format_ident!("{}", name);
                    let value_code = evaluator::generate_expression_code(value)?;
                    else_variable_bindings.push(quote! {
                        let #var_name = #value_code;
                    });
                }
                _ => {} // Ignore other non-action statements
            }
        }
    }

    // Always generate logic If statements - this handles both bool and Condition types
    // The hyperchad_actions::logic::if_stmt function handles the condition evaluation correctly
    let then_actions = then_effects
        .iter()
        .map(|effect| {
            quote! {
                hyperchad_actions::Action {
                    trigger: hyperchad_actions::ActionTrigger::Immediate,
                    effect: {
                        #(#then_variable_bindings)*
                        #effect
                    },
                }
            }
        })
        .collect::<Vec<_>>();

    let else_actions = else_effects
        .iter()
        .map(|effect| {
            quote! {
                hyperchad_actions::Action {
                    trigger: hyperchad_actions::ActionTrigger::Immediate,
                    effect: {
                        #(#else_variable_bindings)*
                        #effect
                    },
                }
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        {
            let mut if_action = hyperchad_actions::logic::if_stmt(
                #condition_code,
                hyperchad_actions::ActionType::NoOp
            );

            // Direct vec![] with compile-time generated Action structs - ZERO runtime logic
            if_action.actions = vec![#(#then_actions),*];
            if_action.else_actions = vec![#(#else_actions),*];

            hyperchad_actions::ActionType::Logic(if_action).into()
        }
    })
}

fn expand_actions_dsl_single(input: &TokenStream) -> Result<TokenStream, String> {
    // Parse the input tokens into our DSL AST
    let dsl = match Parser::parse2(parser::parse_dsl, input.clone()) {
        Ok(dsl) => dsl,
        Err(_e) => {
            // Try fallback parsing with raw Rust code
            parser::parse_dsl_with_fallback(input)
        }
    };

    // For single action, find the first action-producing statement
    for stmt in &dsl.statements {
        match stmt {
            Statement::Expression(expr) if evaluator::expression_produces_action(expr) => {
                let action_code = evaluator::generate_expression_code(expr)?;
                return Ok(quote! {
                    (#action_code).into()
                });
            }
            Statement::If {
                condition,
                then_block,
                else_block,
            } => {
                return generate_pure_compile_time_if_action(
                    condition,
                    then_block,
                    else_block.as_ref(),
                );
            }
            _ => {}
        }
    }

    // No action-producing statements - return NoOp
    Ok(quote! {
        hyperchad_actions::ActionType::NoOp.into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_simple_function_call() {
        let input = quote! {
            show("test");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        // Should generate a direct vec![] with one element - ZERO runtime logic
        let result_str = result.to_string();
        assert!(result_str.contains("Vec < hyperchad_actions :: ActionEffect >"));
        assert!(result_str.contains("vec ! ["));
        assert!(result_str.contains("show"));
        // Verify NO runtime allocation or checks
        assert!(!result_str.contains(".into_iter()"));
        assert!(!result_str.contains(".collect()"));
        assert!(!result_str.contains(".push("));
    }

    #[test]
    fn test_if_statement() {
        let input = quote! {
            if true {
                show("modal");
            } else {
                hide("modal");
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        // Should generate compile-time optimized if statement
        assert!(result.to_string().contains("if"));
    }

    #[test]
    fn test_variable_assignment() {
        let input = quote! {
            let x = "test";
            show(x);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        // Should include variable binding
        assert!(result.to_string().contains("let x"));
    }

    #[test]
    fn test_invoke_function() {
        let input = quote! {
            invoke(Action::Test, "value");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        assert!(result.to_string().contains("vec !"));
    }

    #[test]
    fn test_invoke_with_expression() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        assert!(result.to_string().contains("Parameterized"));
    }

    #[test]
    fn test_math_operations_convert_to_methods() {
        let input = quote! {
            invoke(Action::Test, 10 + 20);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        assert!(result.to_string().contains("Parameterized"));
    }

    #[test]
    fn test_multiple_math_operations() {
        let input = quote! {
            invoke(Action::Test, (10 + 20) * 2);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");
        assert!(result.to_string().contains("Parameterized"));
    }

    #[test]
    fn test_user_action_enum_no_conflict() {
        // Test that user's Action enum doesn't conflict with the DSL
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, 0.5);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should not have compilation errors and should generate proper invoke
        assert!(result.to_string().contains("Parameterized"));
        assert!(
            result
                .to_string()
                .contains("Action :: SeekCurrentTrackPercent")
        );
    }

    #[test]
    fn test_throttle_function() {
        let input = quote! {
            throttle(30, invoke(Action::SetVolume, 0.5));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate throttle wrapper around invoke
        assert!(result.to_string().contains("throttle"));
        assert!(result.to_string().contains("Parameterized"));
    }

    #[test]
    fn test_clamp_function() {
        let input = quote! {
            invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate clamp function call
        assert!(result.to_string().contains("clamp"));
        assert!(result.to_string().contains("get_width_px_self"));
    }

    #[test]
    fn test_complex_expression() {
        let input = quote! {
            invoke(
                Action::SeekCurrentTrackPercent,
                get_mouse_x_self() / get_width_px_self()
            );
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle complex expressions properly
        assert!(result.to_string().contains("get_mouse_x_self"));
        assert!(result.to_string().contains("get_width_px_self"));
        assert!(result.to_string().contains("divide"));
    }

    #[test]
    fn test_invoke_with_float_literal() {
        let input = quote! {
            invoke(Action::SetVolume, 0.75);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle float literals correctly
        assert!(result.to_string().contains("0.75"));
        assert!(result.to_string().contains("Parameterized"));
    }

    #[test]
    fn test_invoke_with_different_numeric_types() {
        let input = quote! {
            invoke(Action::SetPosition, 100i32);
            invoke(Action::SetVolume, 0.5f64);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle different numeric types
        assert!(result.to_string().contains("100i64"));
        assert!(result.to_string().contains("0.5f64"));
    }

    #[test]
    fn test_clamp_with_float_literals() {
        let input = quote! {
            invoke(Action::SetVolume, clamp(0.0, 0.75, 1.0));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle clamp with float literals
        assert!(result.to_string().contains("clamp"));
        assert!(result.to_string().contains("0f64"));
        assert!(result.to_string().contains("0.75f64"));
        assert!(result.to_string().contains("1f64"));
    }

    #[test]
    fn test_throttle_with_different_integer_types() {
        let input = quote! {
            throttle(30u32, invoke(Action::Test, "value"));
            throttle(50i64, invoke(Action::Test2, "value2"));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle different integer types in throttle
        assert!(result.to_string().contains("30i64"));
        assert!(result.to_string().contains("50i64"));
        assert!(result.to_string().contains("throttle"));
    }

    #[test]
    fn test_delay_off_function() {
        let input = quote! {
            delay_off(1000, show("notification"));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate delay_off wrapper
        assert!(result.to_string().contains("delay_off"));
        assert!(result.to_string().contains("1000"));
        assert!(result.to_string().contains("show"));
    }

    #[test]
    fn test_unique_function() {
        let input = quote! {
            unique(show("modal"));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate unique wrapper
        assert!(result.to_string().contains("unique"));
        assert!(result.to_string().contains("show"));
    }

    #[test]
    fn test_delay_off_method() {
        let input = quote! {
            show("notification").delay_off(1000);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle method chaining for delay_off
        assert!(result.to_string().contains("delay_off"));
        assert!(result.to_string().contains("1000"));
    }

    #[test]
    fn test_closure_parsing() {
        let input = quote! {
            on_event("test", |value| {
                show(value);
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should parse closures correctly
        assert!(result.to_string().contains("Event"));
        assert!(result.to_string().contains('|'));
    }

    #[test]
    fn test_on_event_function() {
        let input = quote! {
            on_event("play-track", |value| {
                if value == get_data_attr_value_self("track-id") {
                    set_background_self("#333");
                } else {
                    remove_background_self();
                }
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle complex on_event with nested if statements
        assert!(result.to_string().contains("Event"));
        assert!(result.to_string().contains("play-track"));
        assert!(result.to_string().contains("get_data_attr_value_self"));
    }

    #[test]
    fn test_closure_with_single_parameter() {
        let input = quote! {
            on_event("change", |e| log(e));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        assert!(result.to_string().contains("Event"));
        assert!(result.to_string().contains("change"));
    }

    #[test]
    fn test_complex_on_event_with_multiple_actions() {
        let input = quote! {
            on_event("play-track", |value| {
                if value == get_data_attr_value_self("track-id") {
                    set_background_self("#333");
                    set_visibility_child_class(Visibility::Hidden, "track-number");
                    set_visibility_child_class(Visibility::Visible, "track-playing");
                } else {
                    remove_background_self();
                    set_visibility_child_class(Visibility::Visible, "track-number");
                    set_visibility_child_class(Visibility::Hidden, "track-playing");
                }
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle complex event handlers with multiple actions
        assert!(result.to_string().contains("Event"));
        assert!(result.to_string().contains("set_background_self"));
        assert!(result.to_string().contains("set_visibility_child_class"));
        assert!(result.to_string().contains("remove_background_self"));
    }

    #[test]
    fn test_remove_background_functions() {
        let input = quote! {
            remove_background_self();
            remove_background_str_id("element-id");
            remove_background_id(123);
            remove_background_class("my-class");
            remove_background_child_class("child-class");
            remove_background_last_child();
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate all remove_background variants
        assert!(result.to_string().contains("remove_background_self"));
        assert!(result.to_string().contains("remove_background_str_id"));
        assert!(result.to_string().contains("remove_background_id"));
        assert!(result.to_string().contains("remove_background_class"));
        assert!(result.to_string().contains("remove_background_child_class"));
        assert!(result.to_string().contains("remove_background_last_child"));
    }

    #[test]
    fn test_remove_background_with_arguments() {
        let input = quote! {
            remove_background_str_id("my-element");
            remove_background_id(42);
            remove_background_class("highlight");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should preserve arguments correctly
        assert!(result.to_string().contains("my-element"));
        assert!(result.to_string().contains("42"));
        assert!(result.to_string().contains("highlight"));
    }

    #[test]
    fn test_comprehensive_event_with_remove_background() {
        let input = quote! {
            on_event("track-change", |track_id| {
                if track_id == get_data_attr_value_self("current-track") {
                    set_background_self("#4a9eff");
                    remove_background_class("inactive");
                } else {
                    remove_background_self();
                    set_background_class("#ddd", "inactive");
                }
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle comprehensive event with background operations
        assert!(result.to_string().contains("Event"));
        assert!(result.to_string().contains("track-change"));
        assert!(result.to_string().contains("remove_background_class"));
        assert!(result.to_string().contains("remove_background_self"));
        assert!(result.to_string().contains("set_background_self"));
    }

    #[test]
    fn test_user_exact_example() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
            invoke(Action::RefreshVisualization, get_width_px_self());
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle the user's exact example without errors
        assert!(result.to_string().contains("Parameterized"));
        assert!(
            result
                .to_string()
                .contains("Action :: SeekCurrentTrackPercent")
        );
        assert!(
            result
                .to_string()
                .contains("Action :: RefreshVisualization")
        );
        assert!(result.to_string().contains("get_mouse_x_self"));
        assert!(result.to_string().contains("get_width_px_self"));
    }

    #[test]
    fn test_reference_operator() {
        let input = quote! {
            let value = &get_mouse_x_self();
            show(value);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle reference operators
        assert!(result.to_string().contains('&'));
        assert!(result.to_string().contains("get_mouse_x_self"));
    }

    #[test]
    fn test_simple_method_chaining() {
        let input = quote! {
            show("test").throttle(30);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle simple method chaining
        assert!(result.to_string().contains("throttle"));
        assert!(result.to_string().contains("30"));
    }

    #[test]
    fn test_fallback_with_complex_expression() {
        let input = quote! {
            complex_function_call(
                param1: get_value(),
                param2: calculate_position(),
            )
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should fallback gracefully for complex expressions
        assert!(result.to_string().contains("complex_function_call"));
    }

    #[test]
    fn test_fallback_preserves_functionality() {
        let input = quote! {
            // Complex Rust code that should fallback
            match some_value {
                Some(x) => process(x),
                None => default_action(),
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should preserve the original functionality via fallback
        assert!(result.to_string().contains("match"));
        assert!(result.to_string().contains("Some"));
        assert!(result.to_string().contains("None"));
    }

    #[test]
    fn test_fallback_with_event_handling() {
        let input = quote! {
            addEventListener("click", function(event) {
                console.log("Clicked!");
            })
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle JavaScript-like syntax via fallback
        assert!(result.to_string().contains("addEventListener"));
    }

    #[test]
    fn test_mixed_dsl_and_fallback() {
        let input = quote! {
            show("modal");
            complex_operation(with: params);
            hide("modal");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle mix of DSL and fallback syntax
        assert!(result.to_string().contains("show"));
        assert!(result.to_string().contains("hide"));
        assert!(result.to_string().contains("complex_operation"));
    }

    #[test]
    fn test_user_complex_expression_exact_case() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // This is the exact case the user reported failing
        // Should work without any parsing or generation errors
        assert!(result.to_string().contains("Parameterized"));
        assert!(
            result
                .to_string()
                .contains("Action :: SeekCurrentTrackPercent")
        );
        assert!(result.to_string().contains("get_mouse_x_self"));
        assert!(result.to_string().contains("get_width_px_self"));

        // Ensure it generates valid Rust code
        assert!(result.to_string().contains("divide"));
    }

    #[test]
    fn test_navigate_with_complex_argument() {
        let input = quote! {
            navigate(format!("/track/{}", track_id));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle navigate with complex format! macro
        assert!(result.to_string().contains("navigate"));
        assert!(result.to_string().contains("format !"));
        assert!(result.to_string().contains("/track/{}"));
    }

    #[test]
    fn test_simple_navigate_still_works() {
        let input = quote! {
            navigate("/home");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should still handle simple navigate calls
        assert!(result.to_string().contains("Navigate"));
        assert!(result.to_string().contains("/home"));
    }

    #[test]
    fn test_user_failing_case_exact() {
        let input = quote! {
            throttle(30, invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0)));
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // This is another exact case from the user
        // Should generate proper nested function calls
        assert!(result.to_string().contains("throttle"));
        assert!(result.to_string().contains("Parameterized"));
        assert!(result.to_string().contains("Action :: SetVolume"));
        assert!(result.to_string().contains("clamp"));
        assert!(result.to_string().contains("get_width_px_self"));
        assert!(result.to_string().contains("0f64"));
        assert!(result.to_string().contains("1f64"));
    }

    #[test]
    fn test_get_event_value_function() {
        let input = quote! {
            on_event("test", |_| {
                show(get_event_value());
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle get_event_value() function
        assert!(result.to_string().contains("get_event_value"));
        assert!(result.to_string().contains("Event"));
    }

    #[test]
    fn test_invoke_with_get_event_value() {
        let input = quote! {
            on_event("volume-change", |_| {
                invoke(Action::SetVolume, get_event_value());
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should combine invoke with get_event_value
        assert!(result.to_string().contains("invoke"));
        assert!(result.to_string().contains("get_event_value"));
        assert!(result.to_string().contains("Action :: SetVolume"));
    }

    #[test]
    fn test_user_exact_pattern() {
        let input = quote! {
            on_event("play-track", |value| {
                if value == get_data_attr_value_self("track-id") {
                    set_background_self("#333");
                    set_visibility_child_class(Visibility::Hidden, "track-number");
                    set_visibility_child_class(Visibility::Visible, "track-playing");
                } else {
                    remove_background_self();
                    set_visibility_child_class(Visibility::Visible, "track-number");
                }
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // This is the exact user pattern - should work perfectly
        assert!(result.to_string().contains("Event"));
        assert!(result.to_string().contains("play-track"));
        assert!(result.to_string().contains("get_data_attr_value_self"));
        assert!(result.to_string().contains("set_background_self"));
        assert!(result.to_string().contains("set_visibility_child_class"));
        assert!(result.to_string().contains("remove_background_self"));
        assert!(result.to_string().contains("Visibility :: Hidden"));
        assert!(result.to_string().contains("Visibility :: Visible"));
    }

    #[test]
    fn test_simple_invoke() {
        let input = quote! {
            invoke(Action::Test, "value");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Simple invoke should work
        assert!(result.to_string().contains("Parameterized"));
        assert!(result.to_string().contains("Action :: Test"));
        assert!(result.to_string().contains("value"));
    }

    #[test]
    fn test_invoke_with_struct_syntax() {
        let input = quote! {
            invoke(Action::UpdateTrack { id: 123, title: "test" }, ());
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle struct-style enum variants
        assert!(result.to_string().contains("Parameterized"));
        assert!(result.to_string().contains("Action :: UpdateTrack"));
        assert!(result.to_string().contains("id : 123"));
        assert!(result.to_string().contains("title : \"test\""));
    }

    #[test]
    fn test_element_function_direct() {
        let input = quote! {
            element(".selector").show();
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle element function with method call
        assert!(result.to_string().contains("show_class (\"selector\")"));
    }

    #[test]
    fn test_element_function_inline() {
        let input = quote! {
            element(".modal").show();
            element("#button").hide();
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should handle multiple element calls
        assert!(result.to_string().contains("show_class (\"modal\")"));
        assert!(result.to_string().contains("hide_str_id (\"button\")"));
    }

    #[test]
    fn test_element_direct_chaining_optimization() {
        let input = quote! {
            element(".modal").show();
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should optimize direct element chaining
        let result_str = result.to_string();

        // Should contain optimized element method call
        assert!(result_str.contains("show_class"));
        assert!(result_str.contains("modal"));
    }

    #[test]
    fn test_element_class_selector_optimization() {
        let input = quote! {
            element(".highlight").set_visibility(Visibility::Visible);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should optimize class selectors
        let result_str = result.to_string();

        // Should contain optimized visibility call
        assert!(result_str.contains("set_visibility_class"));
        assert!(result_str.contains("highlight"));
        assert!(result_str.contains("Visibility :: Visible"));
    }

    #[test]
    fn test_element_visibility_comparison_optimization() {
        let input = quote! {
            if element(".modal").get_visibility() == Visibility::Hidden {
                element(".modal").show();
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should optimize visibility comparisons
        let result_str = result.to_string();
        assert!(result_str.contains("element (\".modal\")"));
        assert!(result_str.contains("get_visibility"));
        assert!(result_str.contains("element (\".modal\") . show ()"));
    }

    #[test]
    fn test_multiple_actions_compile_time_optimization() {
        let input = quote! {
            show("modal");
            hide("sidebar");
            log("Action performed");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate direct vec![] with all actions at compile time - ZERO runtime logic
        let result_str = result.to_string();

        assert!(result_str.contains("Vec < hyperchad_actions :: ActionEffect >"));
        assert!(result_str.contains("vec ! ["));
        assert!(result_str.contains("show"));
        assert!(result_str.contains("hide"));
        assert!(result_str.contains("Log"));
        // Verify NO runtime allocation or checks
        assert!(!result_str.contains(".into_iter()"));
        assert!(!result_str.contains(".collect()"));
        assert!(!result_str.contains(".push("));
    }

    #[test]
    fn test_empty_dsl_compile_time_optimization() {
        let input = quote! {};
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate empty vec![] at compile time - ZERO runtime logic
        let result_str = result.to_string();
        assert!(result_str.contains("Vec < hyperchad_actions :: ActionEffect >"));
        assert!(result_str.contains("vec ! []"));
        // Verify NO runtime allocation or checks
        assert!(!result_str.contains(".into_iter()"));
        assert!(!result_str.contains(".collect()"));
        assert!(!result_str.contains(".push("));
    }

    #[test]
    fn test_actions_dsl_single_element_optimization() {
        let input = quote! {
            element(".modal").show();
        };
        let result = expand_actions_dsl_single(&input).unwrap();
        println!("actual: {result}");

        // Should generate single action effect
        let result_str = result.to_string();
        assert!(result_str.contains("show_class"));
        assert!(result_str.contains("modal"));
    }

    #[test]
    fn test_actions_dsl_single_multiple_actions() {
        let input = quote! {
            show("modal");
            hide("sidebar");
        };
        let result = expand_actions_dsl_single(&input).unwrap();
        println!("actual: {result}");

        // Should return only the first action
        let result_str = result.to_string();
        assert!(result_str.contains("show"));
        assert!(!result_str.contains("hide"));
    }

    #[test]
    fn test_actions_dsl_single_empty() {
        let input = quote! {};
        let result = expand_actions_dsl_single(&input).unwrap();
        println!("actual: {result}");

        // Should return NoOp
        let result_str = result.to_string();
        assert!(result_str.contains("ActionType :: NoOp"));
    }

    #[test]
    fn test_actions_dsl_main_optimizations() {
        let input = quote! {
            show("test");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate optimized single action vec
        let result_str = result.to_string();
        assert!(result_str.contains("vec !"));
        assert!(result_str.contains("show"));
    }

    #[test]
    fn test_vec_optimization() {
        let input = quote! {
            show("modal");
            hide("sidebar");
            log("test");
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate direct vec![] with all actions
        let result_str = result.to_string();
        assert!(result_str.contains("vec !"));
        assert!(result_str.contains("show"));
        assert!(result_str.contains("hide"));
        assert!(result_str.contains("Log"));
    }

    #[test]
    fn test_variable_scoping_generated_code() {
        let input = quote! {
            let modal_id = "test-modal";
            show(modal_id);
            hide(modal_id);
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate proper variable scoping
        let result_str = result.to_string();
        assert!(result_str.contains("let modal_id"));
        assert!(result_str.contains("test-modal"));
        assert!(result_str.contains("show"));
        assert!(result_str.contains("hide"));
    }

    #[test]
    fn test_empty_dsl_optimization() {
        let input = quote! {
            // Just comments, no actual actions
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate empty vec
        let result_str = result.to_string();
        assert_eq!(
            result_str.trim(),
            "{ vec ! [] as Vec < hyperchad_actions :: ActionEffect > }"
        );
    }
}
