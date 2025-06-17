use hyperchad_template::container;
use hyperchad_transformer::Number;

#[test]
fn test_basic_calc_syntax() {
    let containers = container! {
        div width=calc(100 - 20) height=calc(50 + 30) {
            "Basic calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Test the string representation of the calc expressions
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(100 - 20)"
    );

    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(50 + 30)"
    );
}

#[test]
fn test_calc_with_variables() {
    let margin = 10;
    let base_width = 100;

    let containers = container! {
        div width=calc(base_width - margin) height=calc(50 + margin) {
            "Variable calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Variables are evaluated at compile time but preserved in calc format
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(100 - 10)"
    );
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(50 + 10)"
    );
}

#[test]
fn test_calc_with_parentheses() {
    let containers = container! {
        div width=calc(10 + 20 * 3) height=calc((10 + 20) * 3) {
            "Parentheses calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Math operations are preserved in calc expressions, respecting precedence
    // Width: 10 + (20 * 3) = Add(10, Multiply(20, 3))
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(10 + 20 * 3)"
    );

    // Height: (10 + 20) * 3 = Multiply(Add(10, 20), 3)
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(10 + 20 * 3)"
    );

    // The expressions should be different structures
    assert_ne!(
        containers[0].width, containers[0].height,
        "Width and height should have different calculation structures"
    );
}

#[test]
fn test_calc_with_unit_functions() {
    let height_value = 50;
    let width_value = 80;

    let containers = container! {
        div width=calc(percent(width_value) - 10) height=calc(vh(height_value) + 20) {
            "Unit function calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Unit functions are evaluated and preserved in calc expressions
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(80% - 10)"
    );
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(50vh + 20)"
    );
}

#[test]
fn test_calc_with_number_types() {
    let base_width = Number::Integer(80);
    let margin = Number::Integer(20);

    let containers = container! {
        div width=calc(base_width - 10) height=calc(margin * 2) {
            "Number types calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Number types are handled in calc expressions
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(80 - 10)"
    );
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(20 * 2)"
    );
}

#[test]
fn test_calc_with_all_operations() {
    let containers = container! {
        div {
            // Addition
            div width=calc(50 + 30) { "Addition" }
            // Subtraction
            div width=calc(100 - 20) { "Subtraction" }
            // Multiplication
            div width=calc(25 * 4) { "Multiplication" }
            // Division
            div width=calc(200 / 2) { "Division" }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");
    assert_eq!(containers[0].children.len(), 4, "Should have 4 child containers");

    // All operations should be preserved in calc format
    assert_eq!(
        containers[0].children[0].width.as_ref().unwrap().to_string(),
        "calc(50 + 30)"
    );
    assert_eq!(
        containers[0].children[1].width.as_ref().unwrap().to_string(),
        "calc(100 - 20)"
    );
    assert_eq!(
        containers[0].children[2].width.as_ref().unwrap().to_string(),
        "calc(25 * 4)"
    );
    assert_eq!(
        containers[0].children[3].width.as_ref().unwrap().to_string(),
        "calc(200 / 2)"
    );
}

#[test]
fn test_calc_with_percent() {
    let containers = container! {
        div width=calc(50% + 30) { "Percent" }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(50% + 30)",
    );
}

#[test]
fn test_calc_with_conditionals() {
    let is_mobile = true;
    let desktop_width = 80;
    let mobile_width = 90;

    let containers = container! {
        div width=calc(if is_mobile { mobile_width } else { desktop_width }) {
            "Conditional calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Conditional expressions are evaluated at compile time but preserved in calc
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(90)"
    );
}

#[test]
fn test_calc_division_safety() {
    let containers = container! {
        div width=calc(100 / 2) height=calc(50 / 1) {
            "Division safety test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Division operations should be preserved in calc format
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(100 / 2)"
    );
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(50 / 1)"
    );
}

#[test]
fn test_calc_with_method_calls() {
    let values = [10, 20, 30];
    let len = values.len() as i64;
    let first = values[0];
    let second = values[1];

    let containers = container! {
        div width=calc(len * 20) height=calc(first + second) {
            "Method calls calc test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Method call results are evaluated at compile time but preserved in calc
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(3 * 20)"
    );
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(10 + 20)"
    );
}

#[test]
fn test_calc_with_helper_functions() {
    let value = 50;

    let containers = container! {
        div {
            div width=calc(percent(value)) { "Percent helper" }
            div width=calc(vw(value)) { "VW helper" }
            div width=calc(vh(value)) { "VH helper" }
            div width=calc(dvw(value)) { "DVW helper" }
            div width=calc(dvh(value)) { "DVH helper" }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");
    assert_eq!(containers[0].children.len(), 5, "Should have 5 child containers");

    // Helper functions are evaluated and preserved in calc expressions
    assert_eq!(
        containers[0].children[0].width.as_ref().unwrap().to_string(),
        "calc(50%)"
    );
    assert_eq!(
        containers[0].children[1].width.as_ref().unwrap().to_string(),
        "calc(50vw)"
    );
    assert_eq!(
        containers[0].children[2].width.as_ref().unwrap().to_string(),
        "calc(50vh)"
    );
    assert_eq!(
        containers[0].children[3].width.as_ref().unwrap().to_string(),
        "calc(50dvw)"
    );
    assert_eq!(
        containers[0].children[4].width.as_ref().unwrap().to_string(),
        "calc(50dvh)"
    );
}

#[test]
fn test_calc_vs_regular_attributes() {
    // Test that calc() DSL works alongside regular numeric attributes
    let containers_calc = container! {
        div width=calc(80 + 20) { "Calc width" }
    };

    let containers_regular = container! {
        div width=100 { "Regular width" }
    };

    // Both should generate valid containers
    assert_eq!(
        containers_calc.len(),
        1,
        "Calc should generate one container"
    );
    assert_eq!(
        containers_regular.len(),
        1,
        "Regular should generate one container"
    );

    // Calc should generate calc() expression, regular should generate simple value
    assert_eq!(
        containers_calc[0].width.as_ref().unwrap().to_string(),
        "calc(80 + 20)"
    );
    assert_eq!(
        containers_regular[0].width.as_ref().unwrap().to_string(),
        "100"
    );
}

#[test]
fn test_calc_syntax_parsing() {
    // The main value of calc() is that it provides a natural syntax for mathematical expressions
    // This test verifies the syntax is parsed correctly
    let a = 10;
    let b = 20;
    let c = 30;

    let containers = container! {
        div width=calc(((a + b) * 2) - c) {
            "Complex expression test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Complex expressions should be preserved in calc format
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(10 + 20 * 2 - 30)"
    );
}

#[test]
fn test_calc_error_handling() {
    // Test with valid expressions that might cause edge cases
    let containers = container! {
        div width=calc(100 * 0) height=calc(50 + 0) {
            "Edge case test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    // Edge cases should be preserved in calc format
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(100 * 0)"
    );
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(50 + 0)"
    );
}

#[test]
fn test_calc_with_min_function() {
    let containers = container! {
        div height=calc(min(65%, dvw(50))) { "Min function test" }
    };
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(min(65%, 50dvw))"
    );
}

#[test]
fn test_calc_with_max_function() {
    let containers = container! {
        div width=calc(max(300, 20%)) { "Max function test" }
    };
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(max(300, 20%))"
    );
}

#[test]
fn test_calc_with_clamp_function() {
    let containers = container! {
        div width=calc(clamp(300, 50%, 800)) { "Clamp function test" }
    };
    // clamp(min, preferred, max) expands to max(min, min(preferred, max))
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(max(300, min(50%, 800)))"
    );
}

#[test]
fn test_min_function_standalone() {
    let containers = container! {
        div height=min(65%, dvw(50)) { "Standalone min test" }
    };
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(min(65%, 50dvw))"
    );
}

#[test]
fn test_max_function_standalone() {
    let containers = container! {
        div width=max(300, 20%) { "Standalone max test" }
    };
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(max(300, 20%))"
    );
}

#[test]
fn test_clamp_function_standalone() {
    let containers = container! {
        div width=clamp(300, 50%, 800) { "Standalone clamp test" }
    };
    // clamp(min, preferred, max) expands to max(min, min(preferred, max))
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(max(300, min(50%, 800)))"
    );
}

#[test]
fn test_nested_css_math_functions() {
    let containers = container! {
        div height=calc(max(min(50%, 400), 200)) { "Nested CSS math test" }
    };
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(max(min(50%, 400), 200))"
    );
}

#[test]
fn test_css_math_with_variables() {
    let base_width = 400;
    let max_width = 800;
    let preferred = 50;

    let containers = container! {
        div width=clamp(base_width, percent(preferred), max_width) {
            "CSS math with variables"
        }
    };
    // clamp(min, preferred, max) expands to max(min, min(preferred, max))
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(max(400, min(50%, 800)))"
    );
}

#[test]
fn test_complex_css_math_expression() {
    let containers = container! {
        div height=calc(min(vh(100) - 80, max(400, 50% + 100))) {
            "Complex CSS math expression"
        }
    };
    // The sub-expressions are simplified into a single calc expression
    assert_eq!(
        containers[0].height.as_ref().unwrap().to_string(),
        "calc(min(100vh - 80, max(400, 50% + 100)))"
    );
}

#[test]
fn test_css_math_with_multiple_arguments() {
    let containers = container! {
        div width=min(300, 50%, vh(80), vw(90)) { "Multiple arguments test" }
    };
    // min(a, b, c, d) chains as min(a, min(b, min(c, d)))
    assert_eq!(
        containers[0].width.as_ref().unwrap().to_string(),
        "calc(min(300, min(50%, min(80vh, 90vw))))"
    );
}
