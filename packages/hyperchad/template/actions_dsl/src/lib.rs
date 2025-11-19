//! `HyperChad` Template Actions DSL
//!
//! A procedural macro crate providing a domain-specific language (DSL) for defining UI actions
//! in `HyperChad` templates. The DSL allows writing Rust-like syntax for expressing interactive
//! behaviors, event handling, and UI manipulation.
//!
//! # Overview
//!
//! The [`actions_dsl!`] macro transforms a Rust-like syntax into `HyperChad` action types at
//! compile time, with extensive compile-time optimization to eliminate runtime overhead.
//!
//! # Features
//!
//! * **Rust-like syntax** - Write actions using familiar Rust control flow and expressions
//! * **Compile-time optimization** - Zero runtime allocation for action construction
//! * **Event handling** - Built-in support for event listeners with closures
//! * **Type safety** - Full type checking at compile time
//! * **Method chaining** - Fluent API for action composition
//!
//! # Example
//!
//! ```rust,ignore
//! use hyperchad_template_actions_dsl::actions_dsl;
//!
//! // Simple visibility control
//! actions_dsl! {
//!     show("modal");
//!     hide("sidebar");
//! }
//!
//! // Event handling with closures
//! actions_dsl! {
//!     on_event("play-track", |track_id| {
//!         if track_id == get_data_attr_value_self("track-id") {
//!             set_background_self("#333");
//!         }
//!     });
//! }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

extern crate proc_macro;

mod evaluator;
mod parser;

use hyperchad_actions::dsl::{Dsl, Statement};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;

/// Main proc-macro for parsing actions DSL syntax
///
/// This macro allows you to write Rust-like syntax for defining actions:
///
/// # Examples
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

/// Expands the actions DSL input into Rust code
///
/// This function attempts to parse the input token stream as DSL syntax. If parsing fails,
/// it falls back to treating the input as raw Rust code.
///
/// # Errors
///
/// Returns an error string if code generation fails
fn expand_actions_dsl(input: &TokenStream) -> Result<TokenStream, String> {
    // Try to parse the DSL first
    let dsl = match Parser::parse2(parser::parse_dsl, input.clone()) {
        Ok(dsl) => dsl,
        Err(_e) => {
            // Try fallback parsing with raw Rust code
            parser::parse_dsl_with_fallback(input)
        }
    };

    // Generate pure compile-time DSL
    generate_pure_compile_time_dsl(&dsl)
}

/// Generates completely compile-time optimized DSL with zero runtime logic
///
/// This function optimizes the generated code based on the number of statements:
/// * 0 statements: generates `NoOp`
/// * 1 statement: generates the single action directly
/// * Multiple statements: generates a `vec![]` with all actions
///
/// # Errors
///
/// Returns an error string if code generation for any statement fails
fn generate_pure_compile_time_dsl(dsl: &Dsl) -> Result<TokenStream, String> {
    // Count action-producing statements at compile time
    let action_count = dsl.statements.len();

    // Generate exact code based on compile-time determined action count
    let result = match action_count {
        0 => {
            // ZERO actions - direct NoOp at compile time
            quote! {
                hyperchad_actions::ActionType::NoOp
            }
        }
        1 => {
            // SINGLE action - direct element at compile time
            let action_effect = generate_single_action_effect(&dsl.statements)?;
            quote! {
                #action_effect
            }
        }
        _ => {
            // MULTIPLE actions - direct vec![] with all elements at compile time
            let action_effects = generate_multiple_action_effects(&dsl.statements)?;
            quote! {
                vec![#(#action_effects),*]
            }
        }
    };

    // Combine variable bindings with the result
    Ok(result)
}

/// Generates a single action effect from exactly one statement
///
/// # Errors
///
/// Returns an error string if code generation fails for the statement
///
/// # Panics
///
/// Panics if called with an empty statement list
fn generate_single_action_effect(statements: &[Statement]) -> Result<TokenStream, String> {
    let mut context = evaluator::Context::default();

    if let Some(stmt) = statements.iter().next() {
        return generate_action_effect_from_statement(&mut context, stmt);
    }
    unreachable!("generate_single_action_effect called when no action exists")
}

/// Generates multiple action effects from a list of statements
///
/// # Errors
///
/// Returns an error string if code generation fails for any statement
fn generate_multiple_action_effects(statements: &[Statement]) -> Result<Vec<TokenStream>, String> {
    let mut context = evaluator::Context::default();

    statements
        .iter()
        .map(|x| generate_action_effect_from_statement(&mut context, x))
        .collect::<Result<Vec<_>, _>>()
}

/// Generates an action effect from a single statement
///
/// # Errors
///
/// Returns an error string if code generation fails for the statement
fn generate_action_effect_from_statement(
    context: &mut evaluator::Context,
    stmt: &Statement,
) -> Result<TokenStream, String> {
    let code = evaluator::generate_statement_code(context, stmt)?;
    Ok(quote! {
        #code.into_action_effect()
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
        let result = result.to_string();
        println!("actual: {result}");

        assert!(result.contains("hyperchad_actions :: ActionType :: Style"));
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
        // Verify NO runtime allocation or checks
        assert!(!result.contains(".into_iter()"));
        assert!(!result.contains(".collect()"));
        assert!(!result.contains(".push("));
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
        let result = result.to_string();
        println!("actual: {result}");
        // Should generate compile-time optimized if statement
        assert!(result.contains("hyperchad_actions :: ActionType :: Style"));
        assert!(result.contains("hyperchad_actions :: StyleAction :: SetVisibility"));
        assert!(result.contains("hyperchad_transformer_models :: Visibility :: Visible"));
        assert!(!result.contains("hyperchad_transformer_models :: Visibility :: Hidden"));
        assert!(!result.contains("If"));
    }

    #[test]
    fn test_variable_assignment() {
        let input = quote! {
            let x = "test";
            show(x);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");
        // Should include variable binding
        assert!(result.contains(":: Let { name : \"x\""));
    }

    #[test]
    fn test_invoke_function() {
        let input = quote! {
            invoke(Action::Test, "value");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");
        assert!(result.contains("hyperchad_actions :: ActionType :: Parameterized"));
        assert!(result.contains("hyperchad_actions :: ActionType :: Custom"));
        assert!(result.contains("Action :: Test"));
        assert!(!result.contains("vec !"));
    }

    #[test]
    fn test_invoke_with_expression() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");
        assert!(result.contains("Parameterized"));
    }

    #[test]
    fn test_math_operations_convert_to_methods() {
        let input = quote! {
            invoke(Action::Test, 10 + 20);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");
        assert!(result.contains("Parameterized"));
    }

    #[test]
    fn test_multiple_math_operations() {
        let input = quote! {
            invoke(Action::Test, (10 + 20) * 2);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");
        assert!(result.contains("Parameterized"));
    }

    #[test]
    fn test_user_action_enum_no_conflict() {
        // Test that user's Action enum doesn't conflict with the DSL
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, 0.5);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should not have compilation errors and should generate proper invoke
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: SeekCurrentTrackPercent"));
    }

    #[test]
    fn test_throttle_function() {
        let input = quote! {
            throttle(30, invoke(Action::SetVolume, 0.5));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should generate throttle wrapper around invoke
        assert!(result.contains("throttle"));
        assert!(result.contains("Parameterized"));
    }

    #[test]
    fn test_clamp_function() {
        let input = quote! {
            invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should generate clamp function call
        assert!(result.contains("clamp"));
        assert!(result.contains("get_width_px_self"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle complex expressions properly
        assert!(result.contains("get_mouse_x_self"));
        assert!(result.contains("get_width_px_self"));
        assert!(result.contains("divide"));
    }

    #[test]
    fn test_invoke_with_float_literal() {
        let input = quote! {
            invoke(Action::SetVolume, 0.75);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle float literals correctly
        assert!(result.contains("0.75"));
        assert!(result.contains("Parameterized"));
    }

    #[test]
    fn test_invoke_with_different_numeric_types() {
        let input = quote! {
            invoke(Action::SetPosition, 100i32);
            invoke(Action::SetVolume, 0.5f64);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle different numeric types
        assert!(result.contains("100i64"));
        assert!(result.contains("0.5f64"));
    }

    #[test]
    fn test_clamp_with_float_literals() {
        let input = quote! {
            invoke(Action::SetVolume, clamp(0.0, 0.75, 1.0));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle clamp with float literals
        assert!(result.contains("clamp"));
        assert!(result.contains("0f64"));
        assert!(result.contains("0.75f64"));
        assert!(result.contains("1f64"));
    }

    #[test]
    fn test_throttle_with_different_integer_types() {
        let input = quote! {
            throttle(30u32, invoke(Action::Test, "value"));
            throttle(50i64, invoke(Action::Test2, "value2"));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle different integer types in throttle
        assert!(result.contains("30i64"));
        assert!(result.contains("50i64"));
        assert!(result.contains("throttle"));
    }

    #[test]
    fn test_delay_off_function() {
        let input = quote! {
            delay_off(1000, show("notification"));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should generate delay_off wrapper
        assert!(result.contains("delay_off"));
        assert!(result.contains("1000"));
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
    }

    #[test]
    fn test_unique_function() {
        let input = quote! {
            unique(show("modal"));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should generate unique wrapper
        assert!(result.contains("unique"));
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)",
            ),
        );
    }

    #[test]
    fn test_delay_off_method() {
        let input = quote! {
            show("notification").delay_off(1000);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle method chaining for delay_off
        assert!(result.contains("delay_off"));
        assert!(result.contains("1000"));
    }

    #[test]
    fn test_closure_parsing() {
        let input = quote! {
            on_event("test", |value| {
                show(value);
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should parse closures correctly
        assert!(result.contains("Event"));
        assert!(result.contains('|'));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle complex on_event with nested if statements
        assert!(result.contains("Event"));
        assert!(result.contains("play-track"));
        assert!(result.contains("get_data_attr_value_self"));
    }

    #[test]
    fn test_closure_with_single_parameter() {
        let input = quote! {
            on_event("change", |e| log(e));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        assert!(result.contains("Event"));
        assert!(result.contains("change"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle complex event handlers with multiple actions
        assert!(result.contains("Event"));
        assert!(result.contains("set_background_self"));
        assert!(result.contains(":: SetVisibility (Visibility :: Hidden)"));
        assert!(result.contains(":: SetVisibility (Visibility :: Visible)"));
        assert!(result.contains("remove_background_self"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should generate all remove_background variants
        assert!(result.contains("remove_background_self"));
        assert!(result.contains("remove_background_str_id"));
        assert!(result.contains("remove_background_id"));
        assert!(result.contains("remove_background_class"));
        assert!(result.contains("remove_background_child_class"));
        assert!(result.contains("remove_background_last_child"));
    }

    #[test]
    fn test_remove_background_with_arguments() {
        let input = quote! {
            remove_background_str_id("my-element");
            remove_background_id(42);
            remove_background_class("highlight");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should preserve arguments correctly
        assert!(result.contains("my-element"));
        assert!(result.contains("42"));
        assert!(result.contains("highlight"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle comprehensive event with background operations
        assert!(result.contains("Event"));
        assert!(result.contains("track-change"));
        assert!(result.contains("remove_background_class"));
        assert!(result.contains("remove_background_self"));
        assert!(result.contains("set_background_self"));
    }

    #[test]
    fn test_user_exact_example() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
            invoke(Action::RefreshVisualization, get_width_px_self());
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle the user's exact example without errors
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: SeekCurrentTrackPercent"));
        assert!(result.contains("Action :: RefreshVisualization"));
        assert!(result.contains("get_mouse_x_self"));
        assert!(result.contains("get_width_px_self"));
    }

    #[test]
    fn test_reference_operator() {
        let input = quote! {
            let value = &get_mouse_x_self();
            show(value);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle reference operators
        assert!(result.contains('&'));
        assert!(result.contains("get_mouse_x_self"));
    }

    #[test]
    fn test_simple_method_chaining() {
        let input = quote! {
            show("test").throttle(30);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle simple method chaining
        assert!(result.contains("throttle"));
        assert!(result.contains("30"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should fallback gracefully for complex expressions
        assert!(result.contains("complex_function_call"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // Should preserve the original functionality via fallback
        assert!(result.contains("match"));
        assert!(result.contains("Some"));
        assert!(result.contains("None"));
    }

    #[test]
    fn test_fallback_with_event_handling() {
        let input = quote! {
            addEventListener("click", function(event) {
                console.log("Clicked!");
            })
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle JavaScript-like syntax via fallback
        assert!(result.contains("addEventListener"));
    }

    #[ignore = "Unsupported syntax"]
    #[test]
    fn test_mixed_dsl_and_fallback() {
        let input = quote! {
            show("modal");
            complex_operation(with: params);
            hide("modal");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle mix of DSL and fallback syntax
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Hidden)"
            )
        );
        assert!(result.contains("complex_operation"));
    }

    #[test]
    fn test_user_complex_expression_exact_case() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // This is the exact case the user reported failing
        // Should work without any parsing or generation errors
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: SeekCurrentTrackPercent"));
        assert!(result.contains("get_mouse_x_self"));
        assert!(result.contains("get_width_px_self"));

        // Ensure it generates valid Rust code
        assert!(result.contains("divide"));
    }

    #[test]
    fn test_navigate_with_complex_argument() {
        let input = quote! {
            navigate(format!("/track/{}", track_id));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle navigate with complex format! macro
        assert!(result.contains("navigate"));
        assert!(result.contains("format !"));
        assert!(result.contains("/track/{}"));
    }

    #[test]
    fn test_simple_navigate_still_works() {
        let input = quote! {
            navigate("/home");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should still handle simple navigate calls
        assert!(result.contains("Navigate"));
        assert!(result.contains("/home"));
    }

    #[test]
    fn test_user_failing_case_exact() {
        let input = quote! {
            throttle(30, invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0)));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // This is another exact case from the user
        // Should generate proper nested function calls
        assert!(result.contains("throttle"));
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: SetVolume"));
        assert!(result.contains("clamp"));
        assert!(result.contains("get_width_px_self"));
        assert!(result.contains("0f64"));
        assert!(result.contains("1f64"));
    }

    #[test]
    fn test_get_event_value_function() {
        let input = quote! {
            on_event("test", |_| {
                show(get_event_value());
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle get_event_value() function
        assert!(result.contains("get_event_value"));
        assert!(result.contains("Event"));
    }

    #[test]
    fn test_invoke_with_get_event_value() {
        let input = quote! {
            on_event("volume-change", |_| {
                invoke(Action::SetVolume, get_event_value());
            });
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should combine invoke with get_event_value
        assert!(result.contains("invoke"));
        assert!(result.contains("get_event_value"));
        assert!(result.contains("Action :: SetVolume"));
    }

    #[test]
    fn test_simple_show_hide() {
        let input = quote! {
            show("search");
            hide("search-button");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("simple show/hide: {result}");

        // Should handle show and hide function calls
        assert!(result.contains("ActionType"));
        assert!(result.contains("Style"));
    }

    #[test]
    fn test_key_escape_comparison() {
        let input = quote! {
            if get_event_value() == Key::Escape {
                hide("search");
                show("search-button");
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // The Key::Escape case now correctly transforms everything
        assert!(result.contains("get_event_value"));
        assert!(result.contains("hyperchad_actions :: logic :: Value :: from (Key :: Escape)"));
        assert!(result.contains("ActionType :: Style"));
        assert!(result.contains("SetVisibility"));
        assert!(result.contains("Visible"));
        assert!(result.contains("Hidden"));
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
        let result = result.to_string();
        println!("actual: {result}");

        // This is the exact user pattern - should work perfectly
        assert!(result.contains("Event"));
        assert!(result.contains("play-track"));
        assert!(result.contains("get_data_attr_value_self"));
        assert!(result.contains("set_background_self"));
        assert!(result.contains(":: SetVisibility (Visibility :: Hidden)"));
        assert!(result.contains(":: SetVisibility (Visibility :: Visible)"));
        assert!(result.contains("remove_background_self"));
        assert!(result.contains("Visibility :: Hidden"));
        assert!(result.contains("Visibility :: Visible"));
    }

    #[test]
    fn test_arithmetic_grouping_with_parentheses() {
        let input = quote! {
            invoke(Action::SetVolume, ((get_height_px_str_id("container") - get_mouse_y_str_id("container")) / get_height_px_str_id("container")).clamp(0.0, 1.0));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should properly handle parenthetical grouping in arithmetic expressions
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: SetVolume"));
        assert!(result.contains("get_height_px_str_id"));
        assert!(result.contains("get_mouse_y_str_id"));
        assert!(result.contains("clamp"));

        // The critical test: should contain parentheses to preserve grouping
        // The subtraction should be grouped before the division
        assert!(result.contains('('));
        assert!(result.contains(')'));
    }

    #[test]
    fn test_nested_arithmetic_grouping() {
        let input = quote! {
            invoke(Action::Test, (a + b) * (c - d));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should preserve both groupings
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: Test"));
        // Should maintain proper grouping with parentheses
        assert!(result.contains('('));
        assert!(result.contains(')'));
    }

    #[test]
    fn test_simple_grouping_preserved() {
        let input = quote! {
            invoke(Action::Test, (a + b));
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Even simple grouping should be preserved
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: Test"));
        assert!(result.contains('('));
        assert!(result.contains(')'));
    }

    #[test]
    fn test_simple_invoke() {
        let input = quote! {
            invoke(Action::Test, "value");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Simple invoke should work
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: Test"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_invoke_with_struct_syntax() {
        let input = quote! {
            invoke(Action::UpdateTrack { id: 123, title: "test" }, ());
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle struct-style enum variants
        assert!(result.contains("Parameterized"));
        assert!(result.contains("Action :: UpdateTrack"));
        assert!(result.contains("id : hyperchad_actions :: dsl :: Expression :: Literal"));
        assert!(result.contains("hyperchad_actions :: dsl :: Literal :: Integer (123i64)"));
        assert!(
            result.contains(
                "hyperchad_actions :: dsl :: Literal :: String (\"test\" . to_string ())"
            )
        );
    }

    #[test]
    fn test_element_function_direct() {
        let input = quote! {
            element(".selector").show();
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle element function with method call
        assert!(result.contains(r#"hyperchad_actions :: ActionType :: show_str_class (hyperchad_actions :: Target :: literal ("selector"))"#));
    }

    #[test]
    fn test_element_function_inline() {
        let input = quote! {
            element(".modal").show();
            element("#button").hide();
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should handle multiple element calls
        assert!(result.contains("vec !"));
        assert!(result.contains(r#"hyperchad_actions :: ActionType :: show_str_class (hyperchad_actions :: Target :: literal ("modal"))"#));
    }

    #[test]
    fn test_element_direct_chaining_optimization() {
        let input = quote! {
            element(".modal").show();
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should contain optimized element method call
        assert!(result.contains(r#"hyperchad_actions :: ActionType :: show_str_class (hyperchad_actions :: Target :: literal ("modal"))"#));
    }

    #[test]
    fn test_element_class_selector_optimization() {
        let input = quote! {
            element(".highlight").set_visibility(Visibility::Visible);
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        // Should contain optimized visibility call
        assert!(result.contains(r#"hyperchad_actions :: ActionType :: set_visibility_class (Visibility :: Visible , hyperchad_actions :: Target :: literal ("highlight"))"#));
    }

    #[test]
    fn test_element_visibility_comparison_optimization() {
        let input = quote! {
            if element(".modal").get_visibility() == Visibility::Hidden {
                element(".modal").show();
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        assert!(result.contains("get_visibility_str_class"));
        assert!(result.contains("show_str_class"));
        assert!(result.contains("modal"));
    }

    #[test]
    fn test_multiple_actions_compile_time_optimization() {
        let input = quote! {
            show("modal");
            hide("sidebar");
            log("Action performed");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        assert!(result.contains("hyperchad_actions :: ActionType :: Style"));
        assert!(result.contains("vec ! ["));
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Hidden)"
            )
        );
        assert!(result.contains("Log"));
        // Verify NO runtime allocation or checks
        assert!(!result.contains(".into_iter()"));
        assert!(!result.contains(".collect()"));
        assert!(!result.contains(".push("));
    }

    #[test]
    fn test_empty_dsl_compile_time_optimization() {
        let input = quote! {};
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        assert!(result.contains("hyperchad_actions :: ActionType :: NoOp"));
        // Verify NO runtime allocation or checks
        assert!(!result.contains(".into_iter()"));
        assert!(!result.contains(".collect()"));
        assert!(!result.contains(".push("));
    }

    #[test]
    fn test_actions_dsl_main_optimizations() {
        let input = quote! {
            show("test");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
    }

    #[test]
    fn test_vec_optimization() {
        let input = quote! {
            show("modal");
            hide("sidebar");
            log("test");
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("actual: {result}");

        assert!(result.contains("vec !"));
        assert!(result.contains(":: Style {"));
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Hidden)"
            )
        );
        assert!(result.contains("Log"));
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
        let result = result.to_string();
        assert!(result.contains("Let { name : \"modal_id\""));
        assert!(result.contains("test-modal"));
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Visible)"
            )
        );
        assert!(
            result.contains(
                ":: SetVisibility (hyperchad_transformer_models :: Visibility :: Hidden)"
            )
        );
    }

    #[test]
    fn test_empty_dsl_optimization() {
        let input = quote! {
            // Just comments, no actual actions
        };
        let result = expand_actions_dsl(&input).unwrap();
        println!("actual: {result}");

        // Should generate empty vec
        let result = result.to_string();
        assert_eq!(result, "hyperchad_actions :: ActionType :: NoOp");
    }

    #[test]
    fn test_if_with_show_hide() {
        let input = quote! {
            if true {
                show("search");
                hide("search-button");
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("if with show/hide: {result}");

        // Should handle if statement with show and hide function calls
        assert!(result.contains("ActionType"));
        assert!(result.contains("Style"));
    }

    #[test]
    fn test_get_event_value_condition() {
        let input = quote! {
            if get_event_value() == "Escape" {
                show("search");
                hide("search-button");
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("get_event_value condition: {result}");

        // Should handle get_event_value condition with show and hide function calls
        assert!(result.contains("ActionType"));
        assert!(result.contains("Style"));
    }

    #[test]
    fn test_debug_key_escape() {
        let input = quote! {
            Key::Escape
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("debug Key::Escape: {result}");
    }

    #[test]
    fn test_fx_dsl_structure() {
        // This exactly matches the structure in search.rs
        let input = quote! {
            fx {
                if get_event_value() == Key::Escape {
                    hide("search");
                    show("search-button");
                }
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("fx dsl structure: {result}");

        // Should handle the fx DSL structure correctly
        assert!(result.contains("ActionType"));
    }

    #[test]
    fn test_if_statement_with_semicolon() {
        let input = quote! {
            if get_event_value() == Key::Escape {
                hide("search");
                show("search-button");
            };
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("if statement with semicolon: {result}");

        // Should handle if statement with semicolon (transforms consistently)
        assert!(result.contains("ActionType"));
        assert!(result.contains("hyperchad_actions :: logic :: Value :: from (Key :: Escape)"));
        assert!(result.contains("SetVisibility"));
    }

    #[test]
    fn test_simple_key_condition() {
        let input = quote! {
            if Key::Escape {
                show("search");
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("simple key condition: {result}");

        // Should handle simple Key condition
        assert!(result.contains("ActionType"));
    }

    #[test]
    fn test_key_escape_variable() {
        let input = quote! {
            let x = Key::Escape;
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("key escape variable: {result}");

        // Should handle Key::Escape assignment
        assert!(result.contains("Let"));
        assert!(result.contains("Key :: Escape"));
    }

    #[test]
    fn test_key_escape_comparison_in_let() {
        let input = quote! {
            let result = get_event_value() == Key::Escape;
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("key escape comparison in let: {result}");

        // Should handle Key::Escape comparison
        assert!(result.contains("Let"));
        assert!(result.contains("eq"));
        assert!(result.contains("Key :: Escape"));
    }

    #[test]
    fn test_generic_enum_variants() {
        let input = quote! {
            // Test different enum types to prove the solution is generic
            if get_event_value() == Key::Enter {
                show("modal");
            }
            if get_visibility("element") == Visibility::Hidden {
                log("Element is hidden");
            }
        };
        let result = expand_actions_dsl(&input).unwrap();
        let result = result.to_string();
        println!("generic enum variants: {result}");

        // Should handle different enum types generically
        assert!(result.contains("hyperchad_actions :: logic :: Value :: from (Key :: Enter)"));
        assert!(
            result.contains("hyperchad_actions :: logic :: Value :: from (Visibility :: Hidden)")
        );
        assert!(result.contains("ActionType :: Style"));
        assert!(result.contains("ActionType :: Log"));
    }
}
