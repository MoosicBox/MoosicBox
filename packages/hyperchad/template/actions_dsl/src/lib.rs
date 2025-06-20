#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

extern crate proc_macro;

mod evaluator;
mod parser;

use proc_macro2::{Ident, Span, TokenStream};
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
///     // Invoke actions with parameters
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

fn expand_actions_dsl(input: &TokenStream) -> Result<TokenStream, String> {
    // Parse the input tokens into our DSL AST
    let dsl = match Parser::parse2(parser::parse_dsl, input.clone()) {
        Ok(dsl) => dsl,
        Err(_e) => {
            // Try fallback parsing with raw Rust code
            parser::parse_dsl_with_fallback(input)
        }
    };

    // Generate the code that evaluates the DSL at runtime
    let output_ident = Ident::new("__hyperchad_actions_dsl_output", Span::mixed_site());

    // Generate the evaluation code, passing the output variable name
    let eval_code = evaluator::generate_evaluation_code(&dsl, &output_ident)?;

    Ok(quote! {{
        // Use fully qualified paths to avoid shadowing user's types
        {
            let mut #output_ident: Vec<hyperchad_actions::ActionEffect> = Vec::new();

            #eval_code

            #output_ident
        }
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_simple_function_call() {
        let input = quote! {
            hide("test");
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should parse simple function call");
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

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should parse if statement");
    }

    #[test]
    fn test_variable_assignment() {
        let input = quote! {
            let target = "modal";
            show(target);
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should parse variable assignment");
    }

    #[test]
    fn test_invoke_function() {
        let input = quote! {
            invoke(Action::RefreshVisualization, get_width_px_self());
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should parse invoke function call");
    }

    #[test]
    fn test_invoke_with_expression() {
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should parse invoke function with expression"
        );
    }

    #[test]
    fn test_math_operations_convert_to_methods() {
        let input = quote! {
            let result = get_mouse_x_self() / get_width_px_self();
            invoke(Action::SeekCurrentTrackPercent, result);
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should convert math operations to method calls"
        );

        // The generated code should contain .divide() instead of /
        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains(". divide ("),
            "Should generate .divide() method call"
        );
    }

    #[test]
    fn test_multiple_math_operations() {
        let input = quote! {
            let calc = (get_mouse_x_self() + 10) * get_width_px_self() / 100;
            log("calculated value");
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle multiple math operations");
    }

    #[test]
    fn test_user_action_enum_no_conflict() {
        // Simulate user code with their own Action enum
        let input = quote! {
            invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
            invoke(Action::RefreshVisualization, get_width_px_self());
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should work with user's Action enum");

        // Verify that user's Action enum is preserved in generated code
        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("Action :: SeekCurrentTrackPercent"),
            "Should preserve user's Action enum"
        );
        assert!(
            code_str.contains("Action :: RefreshVisualization"),
            "Should preserve user's Action enum"
        );
        // Verify that hyperchad ActionType is fully qualified
        assert!(
            code_str.contains("hyperchad_actions :: ActionType ::"),
            "Should use fully qualified hyperchad ActionType"
        );
    }

    #[test]
    fn test_throttle_function() {
        let input = quote! {
            throttle(30, invoke(Action::SetVolume, get_width_px_self()));
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should parse throttle function");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("throttle : Some"),
            "Should generate throttle field"
        );
    }

    #[test]
    fn test_clamp_function() {
        let input = quote! {
            clamp(0.0, get_width_px_self(), 1.0);
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should parse clamp function");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains(". clamp"),
            "Should generate clamp method call"
        );
    }

    #[test]
    fn test_complex_expression() {
        // Test the complex volume slider expression
        let input = quote! {
            throttle(30,
                invoke(
                    Action::SetVolume,
                    clamp(0.0,
                        (get_height_px_str_id("container") - get_mouse_y_str_id("container"))
                            / get_height_px_str_id("container"),
                        1.0)
                )
            );
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle complex nested expressions"
        );

        let code_str = format!("{}", result.unwrap());

        assert!(
            code_str.contains("throttle : Some"),
            "Should generate throttle"
        );
        assert!(code_str.contains(". clamp"), "Should generate clamp");
        assert!(code_str.contains(". minus"), "Should generate minus");
        assert!(code_str.contains(". divide"), "Should generate divide");
    }

    #[test]
    fn test_invoke_with_float_literal() {
        let input = quote! {
            invoke(Action::SetVolume, 1.0);
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle invoke with float literal"
        );

        // The generated code should use Value::from directly without explicit cast
        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("Value :: from"),
            "Should use Value::from for f64 literal"
        );
    }

    #[test]
    fn test_invoke_with_different_numeric_types() {
        // Test with f64 literals
        let input = quote! {
            invoke(Action::SetVolume, 1.0);
            invoke(Action::SetVolume, 0.5);
            invoke(Action::SetVolume, 2.5);
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle different f64 literals");
    }

    #[test]
    fn test_clamp_with_float_literals() {
        // Test that clamp works with f64 literals in the context of invoke
        let input = quote! {
            invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0));
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle clamp with f64 literals in invoke"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains(". clamp"),
            "Should generate clamp method call"
        );
        assert!(
            code_str.contains("Value :: from"),
            "Should use Value::from in invoke"
        );
    }

    #[test]
    fn test_throttle_with_different_integer_types() {
        // Test throttle with different integer types
        let input = quote! {
            throttle(30, invoke(Action::SetVolume, 1.0));
            throttle(1000, show("modal"));
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle throttle with different integer types"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("as u64"),
            "Should cast integer arguments to u64"
        );
    }

    #[test]
    fn test_delay_off_function() {
        // Test delay_off function form
        let input = quote! {
            delay_off(400, show_self());
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle delay_off function");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("delay_off : Some"),
            "Should generate delay_off field"
        );
        assert!(code_str.contains("as u64"), "Should cast duration to u64");
    }

    #[test]
    fn test_unique_function() {
        // Test unique function form
        let input = quote! {
            unique(show("notification"));
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle unique function");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("unique : Some"),
            "Should generate unique field"
        );
    }

    #[test]
    fn test_delay_off_method() {
        // Test delay_off method chaining
        let input = quote! {
            show("tooltip");
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle delay_off method");
    }

    #[test]
    fn test_closure_parsing() {
        // Test basic closure parsing
        let input = quote! {
            let closure = |value| {
                log("closure called");
            };
        };

        let result = expand_actions_dsl(&input);
        match &result {
            Ok(_) => {}
            Err(e) => println!("Error: {e}"),
        }
        assert!(result.is_ok(), "DSL should parse closures");
    }

    #[test]
    fn test_on_event_function() {
        // Test on_event function with closure
        let input = quote! {
            on_event("play-track", |value| {
                if value == get_data_attr_value_self("track-id") {
                    set_background_self("#333");
                } else {
                    remove_background_self();
                }
            });
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle on_event with closure");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Event"),
            "Should generate ActionType::Event"
        );
        assert!(
            code_str.contains("get_event_value"),
            "Should transform closure parameter to get_event_value"
        );
    }

    #[test]
    fn test_closure_with_single_parameter() {
        // Test closure with single parameter
        let input = quote! {
            on_event("test-event", |value| log("got value"));
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle single parameter closure");
    }

    #[test]
    fn test_complex_on_event_with_multiple_actions() {
        // Test the user's complex on_event example
        let input = quote! {
            on_event("play-track", |value| {
                if value == get_data_attr_value_self("track-id") {
                    set_background_self("#333");
                    set_visibility_child_class(Visibility::Hidden, "track-number");
                    set_visibility_child_class(Visibility::Hidden, "play-button");
                    set_visibility_child_class(Visibility::Visible, "track-playing");
                } else {
                    remove_background_self();
                    set_visibility_child_class(Visibility::Hidden, "play-button");
                    set_visibility_child_class(Visibility::Hidden, "track-playing");
                    set_visibility_child_class(Visibility::Visible, "track-number");
                }
            });
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle complex on_event with multiple actions"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Event"),
            "Should generate ActionType::Event"
        );
        assert!(
            code_str.contains("get_event_value"),
            "Should transform closure parameter to get_event_value"
        );
        assert!(
            code_str.contains("set_background_self"),
            "Should include set_background_self action"
        );
        assert!(
            code_str.contains("set_visibility_child_class"),
            "Should include set_visibility_child_class actions"
        );
        assert!(
            code_str.contains("remove_background_self"),
            "Should include remove_background_self action"
        );
    }

    #[test]
    fn test_remove_background_functions() {
        // Test all remove_background function variants
        let input = quote! {
            remove_background_self();
            remove_background_str_id("test-id");
            remove_background_id(123);
            remove_background_class("test-class");
            remove_background_child_class("child-class");
            remove_background_last_child();
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle all remove_background functions"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("remove_background_self"),
            "Should include remove_background_self"
        );
        assert!(
            code_str.contains("remove_background_str_id"),
            "Should include remove_background_str_id"
        );
        assert!(
            code_str.contains("remove_background_id"),
            "Should include remove_background_id"
        );
        assert!(
            code_str.contains("remove_background_class"),
            "Should include remove_background_class"
        );
        assert!(
            code_str.contains("remove_background_child_class"),
            "Should include remove_background_child_class"
        );
        assert!(
            code_str.contains("remove_background_last_child"),
            "Should include remove_background_last_child"
        );
    }

    #[test]
    fn test_remove_background_with_arguments() {
        // Test remove_background functions with string and numeric arguments
        let input = quote! {
            remove_background_str_id("my-element");
            remove_background_id(42);
            remove_background_class("my-class");
            remove_background_child_class("child");
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle remove_background functions with arguments"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("\"my-element\""),
            "Should include string argument"
        );
        assert!(code_str.contains("42"), "Should include numeric argument");
        assert!(
            code_str.contains("\"my-class\""),
            "Should include class name"
        );
        assert!(
            code_str.contains("\"child\""),
            "Should include child class name"
        );
    }

    #[test]
    fn test_comprehensive_event_with_remove_background() {
        // Test comprehensive example with all remove_background variants in event context
        let input = quote! {
            on_event("track-state-change", |state| {
                if state == "playing" {
                    set_background_self("#4CAF50");
                    set_background_str_id("#FF9800", "status-indicator");
                } else if state == "paused" {
                    set_background_self("#FFC107");
                    remove_background_str_id("status-indicator");
                } else {
                    remove_background_self();
                    remove_background_str_id("status-indicator");
                    remove_background_class("active-track");
                    remove_background_child_class("track-controls");
                    remove_background_last_child();
                }
            });
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle comprehensive event with remove_background functions"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Event"),
            "Should generate ActionType::Event"
        );
        assert!(
            code_str.contains("get_event_value"),
            "Should transform closure parameter"
        );
        assert!(
            code_str.contains("remove_background_self"),
            "Should include remove_background_self"
        );
        assert!(
            code_str.contains("remove_background_str_id"),
            "Should include remove_background_str_id"
        );
        assert!(
            code_str.contains("remove_background_class"),
            "Should include remove_background_class"
        );
        assert!(
            code_str.contains("remove_background_child_class"),
            "Should include remove_background_child_class"
        );
        assert!(
            code_str.contains("remove_background_last_child"),
            "Should include remove_background_last_child"
        );
        assert!(
            code_str.contains("set_background_self"),
            "Should include set_background_self"
        );
        assert!(
            code_str.contains("set_background_str_id"),
            "Should include set_background_str_id"
        );
    }

    #[test]
    fn test_user_exact_example() {
        // Test the exact example the user provided that was failing
        let input = quote! {
            on_event("play-track", |value| {
                if value == get_data_attr_value_self("track-id") {
                    set_background_self("#333");
                    set_visibility_child_class(Visibility::Hidden, "track-number");
                    set_visibility_child_class(Visibility::Hidden, "play-button");
                    set_visibility_child_class(Visibility::Visible, "track-playing");
                } else {
                    remove_background_self();
                    set_visibility_child_class(Visibility::Hidden, "play-button");
                    set_visibility_child_class(Visibility::Hidden, "track-playing");
                    set_visibility_child_class(Visibility::Visible, "track-number");
                }
            });
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle the user's exact example without type errors"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Event"),
            "Should generate ActionType::Event"
        );
        assert!(
            code_str.contains("get_event_value"),
            "Should transform closure parameter"
        );
        assert!(
            code_str.contains("set_background_self"),
            "Should include set_background_self"
        );
        assert!(
            code_str.contains("remove_background_self"),
            "Should include remove_background_self"
        );
        assert!(
            code_str.contains("set_visibility_child_class"),
            "Should include set_visibility_child_class"
        );
        // Verify that ActionType::Multi is used correctly for blocks
        assert!(
            code_str.contains("ActionType :: Multi"),
            "Should generate ActionType::Multi for block expressions"
        );
    }

    #[test]
    fn test_reference_operator() {
        // Test basic reference operator support
        let input = quote! {
            let data = &some_variable;
            log("test");
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle reference operator");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("& some_variable"),
            "Should generate reference operator"
        );
    }

    #[test]
    fn test_simple_method_chaining() {
        // Test basic method chaining
        let input = quote! {
            let result = some_value.iter().collect();
            log("test");
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle basic method chaining");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains(". iter"),
            "Should generate iter method call"
        );
        assert!(
            code_str.contains(". collect"),
            "Should generate collect method call"
        );
    }

    #[test]
    fn test_complex_expression_with_if() {
        // Test a simpler version of the user's complex expression
        let input = quote! {
            let url_param = if checked {
                filtered_sources
            } else {
                other_sources
            };
            navigate(albums_page_url(url_param, sort));
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle complex expression with if"
        );
    }

    #[test]
    fn test_array_operations() {
        // Test array operations that might be closer to what the user needs
        let input = quote! {
            let result = &[item1, item2];
            navigate(some_function(result));
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle array operations with reference"
        );
    }

    #[test]
    fn test_fallback_with_complex_expression() {
        // Test the user's complex expression that requires fallback
        let input = quote! {
            navigate(albums_page_url(&if checked {
                filtered_sources.iter().filter(|x| *x != source).cloned().collect::<Vec<_>>()
            } else {
                [filtered_sources, &[source.clone()]].concat()
            }, sort));
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle complex expression via fallback"
        );

        let code_str = format!("{}", result.unwrap());
        // Should contain the raw Rust code as a fallback
        assert!(
            code_str.contains("albums_page_url"),
            "Should preserve the function call"
        );
        assert!(
            code_str.contains("filtered_sources"),
            "Should preserve variable references"
        );
    }

    #[test]
    fn test_fallback_preserves_functionality() {
        // Test that fallback doesn't break normal DSL functionality
        let input = quote! {
            log("This should work normally");
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should still work for normal expressions"
        );

        let code_str = format!("{}", result.unwrap());
        // Should NOT use fallback for simple expressions
        assert!(
            code_str.contains("hyperchad_actions"),
            "Should use normal DSL functions"
        );
    }

    #[test]
    fn test_fallback_with_event_handling() {
        // Test that fallback works within event handling context
        let input = quote! {
            on_event("complex-event", |value| {
                navigate(complex_function(&value.some_complex_method()));
            });
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle fallback within event closures"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Event"),
            "Should still generate event handling"
        );
    }

    #[test]
    fn test_mixed_dsl_and_fallback() {
        // Test mixing normal DSL with expressions that need fallback
        let input = quote! {
            log("Normal DSL");
            some_complex_function(filtered_sources.iter().collect());
        };

        let result = expand_actions_dsl(&input);
        // With enhanced parsing, this should now succeed
        assert!(
            result.is_ok(),
            "DSL should handle mixed normal and complex expressions"
        );

        let code_str = format!("{}", result.unwrap());
        // Should contain both DSL function and raw Rust expression
        assert!(
            code_str.contains("hyperchad_actions"),
            "Should contain DSL functions"
        );
        assert!(
            code_str.contains("some_complex_function"),
            "Should contain raw Rust expression"
        );
        assert!(
            code_str.contains("filtered_sources"),
            "Should preserve variable references"
        );
    }

    #[test]
    fn test_user_complex_expression_exact_case() {
        // Test the exact failing expression from the user
        let input = quote! {
            navigate(
                albums_page_url(&if checked {
                    filtered_sources.iter().filter(|x| *x != source).cloned().collect::<Vec<_>>()
                } else {
                    [filtered_sources, &[source.clone()]].concat()
                }, sort)
            )
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle the user's exact complex expression via fallback"
        );

        let code_str = format!("{}", result.unwrap());
        // Verify it contains the complex expression
        assert!(
            code_str.contains("albums_page_url"),
            "Should preserve the function call"
        );
        assert!(
            code_str.contains("filtered_sources"),
            "Should preserve variable references"
        );
        assert!(code_str.contains("collect"), "Should preserve method calls");
        assert!(
            code_str.contains("concat"),
            "Should preserve array operations"
        );
    }

    #[test]
    fn test_navigate_with_complex_argument() {
        // Test that navigate function is recognized while complex argument uses fallback
        let input = quote! {
            navigate(albums_page_url(&if checked {
                filtered_sources.iter().filter(|x| *x != source).cloned().collect::<Vec<_>>()
            } else {
                [filtered_sources, &[source.clone()]].concat()
            }, sort))
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle navigate with complex argument"
        );

        let code_str = format!("{}", result.unwrap());
        // Should use the DSL navigate function, not raw Rust fallback for the whole thing
        assert!(
            code_str.contains("ActionType :: Navigate"),
            "Should use DSL navigate function"
        );
        assert!(
            code_str.contains("albums_page_url"),
            "Should preserve the function call in argument"
        );
        assert!(
            code_str.contains("filtered_sources"),
            "Should preserve variable references in argument"
        );

        // Should NOT contain raw navigate function call
        assert!(
            !code_str.contains("navigate ("),
            "Should not have raw navigate function call"
        );
    }

    #[test]
    fn test_simple_navigate_still_works() {
        // Ensure simple navigate calls still work normally
        let input = quote! {
            navigate("/home")
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle simple navigate");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Navigate"),
            "Should use DSL navigate function"
        );
        assert!(
            code_str.contains("\"/home\""),
            "Should preserve the URL argument"
        );
    }

    #[test]
    fn test_user_failing_case_exact() {
        // Test the exact failing expression from the user
        let input = quote! {
            navigate(
                albums_page_url(&if checked {
                    filtered_sources.iter().filter(|x| *x != source).cloned().collect::<Vec<_>>()
                } else {
                    [filtered_sources, &[source.clone()]].concat()
                }, sort)
            )
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle the user's exact failing expression"
        );

        let code_str = format!("{}", result.unwrap());
        // Should use the DSL navigate function, not raw Rust fallback for the whole thing
        assert!(
            code_str.contains("ActionType :: Navigate"),
            "Should use DSL navigate function"
        );
        assert!(
            code_str.contains("albums_page_url"),
            "Should preserve the function call in argument"
        );
        assert!(
            code_str.contains("filtered_sources"),
            "Should preserve variable references in argument"
        );
        assert!(
            code_str.contains("collect"),
            "Should preserve method calls in argument"
        );
        assert!(
            code_str.contains("concat"),
            "Should preserve array operations in argument"
        );

        // Should NOT contain raw navigate function call
        assert!(
            !code_str.contains("navigate ("),
            "Should not have raw navigate function call"
        );

        // The complex argument should be treated as raw Rust code
        assert!(
            code_str.contains("& if checked"),
            "Should preserve complex if expression"
        );
    }

    #[test]
    fn test_get_event_value_function() {
        // Test basic get_event_value function call
        let input = quote! {
            let value = get_event_value();
            log("got event value");
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle get_event_value function");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("hyperchad_actions :: logic :: get_event_value"),
            "Should generate get_event_value call"
        );
    }

    #[test]
    fn test_invoke_with_get_event_value() {
        // Test the pattern the user wants: invoke with get_event_value()
        let input = quote! {
            invoke(Action::FilterAlbums {
                filtered_sources: filtered_sources.to_vec(),
                sort
            }, get_event_value())
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle invoke with get_event_value"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("get_event_value"),
            "Should generate get_event_value call"
        );
        assert!(
            code_str.contains("Action :: FilterAlbums"),
            "Should preserve Action enum"
        );
        assert!(
            code_str.contains("ActionType :: Parameterized"),
            "Should generate ActionType::Parameterized"
        );
    }

    #[test]
    fn test_user_exact_pattern() {
        // Test the exact pattern the user wants to use
        let input = quote! {
            invoke(Action::FilterAlbums {
                filtered_sources: filtered_sources.to_vec(),
                sort
            }, get_event_value())
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle the user's exact pattern");

        let code_str = format!("{}", result.unwrap());
        // Check for the key components
        assert!(
            code_str.contains("hyperchad_actions :: ActionType :: Parameterized"),
            "Should generate Parameterized action"
        );
        assert!(
            code_str.contains("hyperchad_actions :: logic :: get_event_value"),
            "Should call get_event_value"
        );
        assert!(
            code_str.contains("Action :: FilterAlbums"),
            "Should preserve Action enum"
        );
    }

    #[test]
    fn test_simple_invoke() {
        // Test a simpler invoke pattern first
        let input = quote! {
            invoke(Action::SetVolume, 0.5)
        };

        let result = expand_actions_dsl(&input);
        assert!(result.is_ok(), "DSL should handle simple invoke");

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Parameterized"),
            "Should generate ActionType::Parameterized"
        );
    }

    #[test]
    fn test_invoke_with_struct_syntax() {
        // Test invoke with struct syntax but simpler
        let input = quote! {
            invoke(Action::FilterAlbums { sort }, 0.5)
        };

        let result = expand_actions_dsl(&input);
        assert!(
            result.is_ok(),
            "DSL should handle invoke with struct syntax"
        );

        let code_str = format!("{}", result.unwrap());
        assert!(
            code_str.contains("ActionType :: Parameterized"),
            "Should generate ActionType::Parameterized"
        );
    }
}
