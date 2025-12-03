//! Unit tests for hyperchad_template library functions
//!
//! These tests focus on testing the library code directly rather than
//! the macro system, covering RenderContainer implementations, helper
//! functions, and trait implementations.

use hyperchad_color::Color;
use hyperchad_template::{
    ContainerList, ContainerVecExt, Containers, IntoBorder, RenderContainer, calc, color_functions,
    unit_functions,
};
use hyperchad_transformer::Number;
use pretty_assertions::assert_eq;
use std::sync::Arc;

// ============================================================================
// RenderContainer trait implementation tests
// ============================================================================

#[test_log::test]
fn render_container_str() {
    let text = "Hello, World!";
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Hello, World!"
    );
}

#[test_log::test]
fn render_container_str_empty() {
    let text = "";
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    // Empty strings should not add a container
    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn render_container_string() {
    let text = String::from("Test String");
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Test String"
    );
}

#[test_log::test]
fn render_container_string_empty() {
    let text = String::new();
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    // Empty strings should not add a container
    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn render_container_bool() {
    let mut containers_true = Vec::new();
    true.render_to(&mut containers_true).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_true, false, false).unwrap(),
        "true"
    );

    let mut containers_false = Vec::new();
    false.render_to(&mut containers_false).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_false, false, false).unwrap(),
        "false"
    );
}

#[test_log::test]
fn render_container_char() {
    let mut containers = Vec::new();
    'A'.render_to(&mut containers).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "A"
    );

    let mut containers_unicode = Vec::new();
    '🦀'.render_to(&mut containers_unicode).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_unicode, false, false).unwrap(),
        "🦀"
    );
}

#[test_log::test]
fn render_container_integers() {
    // Test various integer types
    let mut containers_i8 = Vec::new();
    42_i8.render_to(&mut containers_i8).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_i8, false, false).unwrap(),
        "42"
    );

    let mut containers_i16 = Vec::new();
    (-1234_i16).render_to(&mut containers_i16).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_i16, false, false).unwrap(),
        "-1234"
    );

    let mut containers_i32 = Vec::new();
    1_000_000_i32.render_to(&mut containers_i32).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_i32, false, false).unwrap(),
        "1000000"
    );

    let mut containers_u8 = Vec::new();
    255_u8.render_to(&mut containers_u8).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_u8, false, false).unwrap(),
        "255"
    );

    let mut containers_u64 = Vec::new();
    18_446_744_073_709_551_615_u64
        .render_to(&mut containers_u64)
        .unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_u64, false, false).unwrap(),
        "18446744073709551615"
    );
}

#[test_log::test]
fn render_container_floats() {
    let mut containers_f32 = Vec::new();
    3.15_f32.render_to(&mut containers_f32).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_f32, false, false).unwrap(),
        "3.15"
    );

    let mut containers_f64 = Vec::new();
    2.7_f64.render_to(&mut containers_f64).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_f64, false, false).unwrap(),
        "2.7"
    );

    // Test zero
    let mut containers_zero = Vec::new();
    0.0_f32.render_to(&mut containers_zero).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_zero, false, false).unwrap(),
        "0.0"
    );
}

#[test_log::test]
fn render_container_option_some() {
    let value = Some("Present");
    let mut containers = Vec::new();
    value.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Present"
    );
}

#[test_log::test]
fn render_container_option_none() {
    let value: Option<&str> = None;
    let mut containers = Vec::new();
    value.render_to(&mut containers).unwrap();

    // None should not add any containers
    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn render_container_reference_types() {
    // Test &T implementation
    let text = String::from("Referenced");
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Referenced"
    );

    // Test &mut T implementation
    let number = 42;
    let mut containers_mut = Vec::new();
    number.render_to(&mut containers_mut).unwrap();
    assert_eq!(
        ContainerVecExt::display_to_string(&containers_mut, false, false).unwrap(),
        "42"
    );
}

#[test_log::test]
fn render_container_box() {
    let boxed = Box::new("Boxed Value");
    let mut containers = Vec::new();
    boxed.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Boxed Value"
    );
}

#[test_log::test]
fn render_container_arc() {
    let arc = Arc::new("Arc Value");
    let mut containers = Vec::new();
    arc.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Arc Value"
    );
}

// ============================================================================
// ContainerVecMethods and ContainerVecExt tests
// ============================================================================

#[test_log::test]
fn container_vec_to_string_vs_into_string() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Test" }
    };

    let str1 = ContainerVecExt::to_string(&containers);
    let str2 = containers.clone().into_string();

    // Both methods should produce the same output
    assert_eq!(str1, str2);
    assert!(str1.contains("Test"));
}

#[test_log::test]
fn container_list_new_and_into_inner() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Content" }
    };

    let list = ContainerList::new(containers.clone());
    let inner = list.into_inner();

    assert_eq!(inner, containers);
}

#[test_log::test]
fn container_list_iteration() {
    use hyperchad_template::container;

    let containers = container! {
        div { "A" }
        div { "B" }
        div { "C" }
    };

    let list = ContainerList::from(containers);
    let count = list.iter().count();

    assert_eq!(count, 3);
}

#[test_log::test]
fn container_list_display() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Display Test" }
    };

    let list = ContainerList::new(containers);
    let display_str = format!("{list}");

    assert!(display_str.contains("Display Test"));
}

// ============================================================================
// calc module tests
// ============================================================================

#[test_log::test]
fn calc_add_numbers_same_unit() {
    // Integer addition
    let result = calc::add_numbers(&Number::Integer(10), &Number::Integer(20));
    assert_eq!(result, Number::Integer(30));

    // Real addition
    let result = calc::add_numbers(&Number::Real(10.5), &Number::Real(20.5));
    assert_eq!(result, Number::Real(31.0));

    // Percent addition
    let result = calc::add_numbers(&Number::IntegerPercent(50), &Number::IntegerPercent(25));
    assert_eq!(result, Number::IntegerPercent(75));
}

#[test_log::test]
fn calc_add_numbers_mixed_types() {
    // Integer + Real
    let result = calc::add_numbers(&Number::Integer(10), &Number::Real(5.5));
    assert_eq!(result, Number::Real(15.5));

    // IntegerPercent + RealPercent
    let result = calc::add_numbers(&Number::IntegerPercent(50), &Number::RealPercent(25.5));
    assert_eq!(result, Number::RealPercent(75.5));

    // Vw units
    let result = calc::add_numbers(&Number::IntegerVw(50), &Number::RealVw(25.5));
    assert_eq!(result, Number::RealVw(75.5));
}

#[test_log::test]
fn calc_subtract_numbers_same_unit() {
    let result = calc::subtract_numbers(&Number::Integer(30), &Number::Integer(10));
    assert_eq!(result, Number::Integer(20));

    let result = calc::subtract_numbers(&Number::RealPercent(75.5), &Number::RealPercent(25.5));
    assert_eq!(result, Number::RealPercent(50.0));
}

#[test_log::test]
fn calc_subtract_numbers_negative_result() {
    let result = calc::subtract_numbers(&Number::Integer(10), &Number::Integer(20));
    assert_eq!(result, Number::Integer(-10));
}

#[test_log::test]
fn calc_multiply_numbers() {
    // Integer multiplication
    let result = calc::multiply_numbers(&Number::Integer(5), &Number::Integer(4));
    assert_eq!(result, Number::Integer(20));

    // Real multiplication
    let result = calc::multiply_numbers(&Number::Real(2.5), &Number::Real(4.0));
    assert_eq!(result, Number::Real(10.0));

    // Percent with scalar
    let result = calc::multiply_numbers(&Number::IntegerPercent(50), &Number::Integer(2));
    assert_eq!(result, Number::IntegerPercent(100));
}

#[test_log::test]
fn calc_divide_numbers() {
    // Integer division (returns Real)
    let result = calc::divide_numbers(&Number::Integer(20), &Number::Integer(4));
    assert_eq!(result, Number::Real(5.0));

    // Real division
    let result = calc::divide_numbers(&Number::Real(10.0), &Number::Real(2.0));
    assert_eq!(result, Number::Real(5.0));

    // Percent division
    let result = calc::divide_numbers(&Number::RealPercent(100.0), &Number::Real(2.0));
    assert_eq!(result, Number::RealPercent(50.0));
}

#[test_log::test]
fn calc_divide_by_zero() {
    // Division by zero should return 0.0 to avoid panics
    let result = calc::divide_numbers(&Number::Integer(10), &Number::Integer(0));
    assert_eq!(result, Number::Real(0.0));

    let result = calc::divide_numbers(&Number::Real(10.0), &Number::Real(0.0));
    assert_eq!(result, Number::Real(0.0));
}

#[test_log::test]
fn calc_to_percent_number() {
    let result = calc::to_percent_number(50);
    assert_eq!(result, Number::IntegerPercent(50));

    let result = calc::to_percent_number(75.5);
    assert_eq!(result, Number::RealPercent(75.5));

    // Already a percent type should remain unchanged
    let result = calc::to_percent_number(Number::IntegerPercent(100));
    assert_eq!(result, Number::IntegerPercent(100));
}

#[test_log::test]
fn calc_viewport_unit_conversions() {
    // Test vw conversion
    let result = calc::to_vw_number(50);
    assert_eq!(result, Number::IntegerVw(50));

    // Test vh conversion
    let result = calc::to_vh_number(75.5);
    assert_eq!(result, Number::RealVh(75.5));

    // Test dvw conversion
    let result = calc::to_dvw_number(100);
    assert_eq!(result, Number::IntegerDvw(100));

    // Test dvh conversion
    let result = calc::to_dvh_number(90.0);
    assert_eq!(result, Number::RealDvh(90.0));
}

// ============================================================================
// unit_functions module tests
// ============================================================================

#[test_log::test]
fn unit_functions_vw() {
    assert_eq!(unit_functions::vw(50), Number::IntegerVw(50));
    assert_eq!(unit_functions::vw(75.5), Number::RealVw(75.5));
}

#[test_log::test]
fn unit_functions_vh() {
    assert_eq!(unit_functions::vh(100), Number::IntegerVh(100));
    assert_eq!(unit_functions::vh(50.5), Number::RealVh(50.5));
}

#[test_log::test]
fn unit_functions_dvw() {
    assert_eq!(unit_functions::dvw(80), Number::IntegerDvw(80));
    assert_eq!(unit_functions::dvw(90.5), Number::RealDvw(90.5));
}

#[test_log::test]
fn unit_functions_dvh() {
    assert_eq!(unit_functions::dvh(60), Number::IntegerDvh(60));
    assert_eq!(unit_functions::dvh(70.5), Number::RealDvh(70.5));
}

#[test_log::test]
fn unit_functions_with_number_types() {
    // Test converting existing Number types
    let num = Number::Integer(50);
    assert_eq!(unit_functions::vw(num), Number::IntegerVw(50));

    let num = Number::Real(75.5);
    assert_eq!(unit_functions::vh(num), Number::RealVh(75.5));
}

// ============================================================================
// color_functions module tests
// ============================================================================

#[test_log::test]
fn color_rgb_basic() {
    let color = color_functions::rgb(255, 0, 0);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgb_with_different_types() {
    // Test with i32
    let color = color_functions::rgb(255_i32, 128_i32, 64_i32);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 128,
            b: 64,
            a: None
        }
    );

    // Test with u8
    let color = color_functions::rgb(200_u8, 100_u8, 50_u8);
    assert_eq!(
        color,
        Color {
            r: 200,
            g: 100,
            b: 50,
            a: None
        }
    );

    // Test with f32 (should clamp and convert)
    let color = color_functions::rgb(255.5_f32, 128.2_f32, 64.9_f32);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 128,
            b: 65,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgb_clamping() {
    // Values over 255 should be clamped
    let color = color_functions::rgb(300_i32, 400_i32, 500_i32);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: None
        }
    );

    // Negative values should be clamped to 0
    let color = color_functions::rgb(-10_i32, -20_i32, -30_i32);
    assert_eq!(
        color,
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgba_with_float_alpha() {
    let color = color_functions::rgba(255, 0, 0, 0.5);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        }
    ); // 0.5 * 255 = 127.5 -> 128

    let color = color_functions::rgba(0, 255, 0, 1.0);
    assert_eq!(
        color,
        Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(255)
        }
    );
}

#[test_log::test]
fn color_rgba_with_integer_alpha() {
    let color = color_functions::rgba(255, 0, 0, 128_u8);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        }
    );

    let color = color_functions::rgba(0, 255, 0, 200_i32);
    assert_eq!(
        color,
        Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(200)
        }
    );
}

#[test_log::test]
fn color_rgba_with_percentage_alpha() {
    let color = color_functions::rgba(255, 0, 0, "50%");
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        }
    ); // 50% = 127.5 -> 128

    let color = color_functions::rgba(0, 255, 0, "100%");
    assert_eq!(
        color,
        Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(255)
        }
    );

    let color = color_functions::rgba(0, 0, 255, "0%");
    assert_eq!(
        color,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(0)
        }
    );
}

#[test_log::test]
fn color_alpha_value_conversions() {
    use color_functions::AlphaValue;

    // Float alpha
    assert_eq!(AlphaValue::Float(0.0).to_u8(), 0);
    assert_eq!(AlphaValue::Float(0.5).to_u8(), 128);
    assert_eq!(AlphaValue::Float(1.0).to_u8(), 255);

    // Integer alpha
    assert_eq!(AlphaValue::Integer(0).to_u8(), 0);
    assert_eq!(AlphaValue::Integer(128).to_u8(), 128);
    assert_eq!(AlphaValue::Integer(255).to_u8(), 255);

    // Percentage alpha
    assert_eq!(AlphaValue::Percentage(0.0).to_u8(), 0);
    assert_eq!(AlphaValue::Percentage(50.0).to_u8(), 128);
    assert_eq!(AlphaValue::Percentage(100.0).to_u8(), 255);
}

#[test_log::test]
fn color_alpha_value_clamping() {
    use color_functions::AlphaValue;

    // Float clamping
    assert_eq!(AlphaValue::Float(-0.5).to_u8(), 0);
    assert_eq!(AlphaValue::Float(1.5).to_u8(), 255);

    // Percentage clamping
    assert_eq!(AlphaValue::Percentage(-10.0).to_u8(), 0);
    assert_eq!(AlphaValue::Percentage(150.0).to_u8(), 255);
}

#[test_log::test]
fn color_alpha_value_from_string() {
    use color_functions::AlphaValue;

    // Percentage string
    let alpha = AlphaValue::from("50%");
    assert_eq!(alpha.to_u8(), 128);

    // Float string
    let alpha = AlphaValue::from("0.75");
    assert_eq!(alpha.to_u8(), 191);

    // Integer string
    let alpha = AlphaValue::from("200");
    assert_eq!(alpha.to_u8(), 200);

    // Invalid string should default to 0
    let alpha = AlphaValue::from("invalid");
    assert_eq!(alpha.to_u8(), 0);
}

#[test_log::test]
fn color_rgb_alpha_function() {
    // Test the rgb_alpha function directly
    let color = color_functions::rgb_alpha(255, 128, 64, 0.8);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 128,
            b: 64,
            a: Some(204)
        }
    ); // 0.8 * 255 = 204
}

// ============================================================================
// IntoBorder trait tests
// ============================================================================

#[test_log::test]
fn into_border_color_number() {
    let border = (Color::from_hex("#FF0000"), Number::Integer(2)).into_border();
    assert_eq!(border.0, Color::from_hex("#FF0000"));
    assert_eq!(border.1, Number::Integer(2));
}

#[test_log::test]
fn into_border_color_i32() {
    let border = (Color::from_hex("#00FF00"), 4_i32).into_border();
    assert_eq!(border.0, Color::from_hex("#00FF00"));
    assert_eq!(border.1, Number::Integer(4));
}

#[test_log::test]
fn into_border_color_f32() {
    let border = (Color::from_hex("#0000FF"), 2.5_f32).into_border();
    assert_eq!(border.0, Color::from_hex("#0000FF"));
    assert_eq!(border.1, Number::Real(2.5));
}

#[test_log::test]
fn into_border_reversed_order() {
    // Test (i32, Color) order
    let border = (3_i32, Color::from_hex("#FF00FF")).into_border();
    assert_eq!(border.0, Color::from_hex("#FF00FF"));
    assert_eq!(border.1, Number::Integer(3));

    // Test (f32, Color) order
    let border = (1.5_f32, Color::from_hex("#00FFFF")).into_border();
    assert_eq!(border.0, Color::from_hex("#00FFFF"));
    assert_eq!(border.1, Number::Real(1.5));
}

#[test_log::test]
fn into_border_hex_string() {
    let border = ("#FF0000", 2_i32).into_border();
    assert_eq!(border.0, Color::from_hex("#FF0000"));
    assert_eq!(border.1, Number::Integer(2));

    let border = (2_i32, "#00FF00").into_border();
    assert_eq!(border.0, Color::from_hex("#00FF00"));
    assert_eq!(border.1, Number::Integer(2));
}

#[test_log::test]
fn into_border_owned_string() {
    let color_str = String::from("#0000FF");
    let border = (color_str, 3_i32).into_border();
    assert_eq!(border.0, Color::from_hex("#0000FF"));
    assert_eq!(border.1, Number::Integer(3));
}

// ============================================================================
// Helper function tests
// ============================================================================

#[test_log::test]
fn to_html_helper() {
    use hyperchad_template::{container, to_html};

    let containers = container! {
        div { "Test Content" }
    };

    let html = to_html(&containers);
    assert!(html.contains("Test Content"));
}

#[test_log::test]
fn into_html_helper() {
    use hyperchad_template::{container, into_html};

    let containers = container! {
        div { "Test Content" }
    };

    let html = into_html(&containers);
    assert!(html.contains("Test Content"));
}

// ============================================================================
// Edge cases and error handling
// ============================================================================

#[test_log::test]
fn render_to_string_empty_containers() {
    let containers: Containers = Vec::new();
    let html = ContainerVecExt::display_to_string(&containers, false, false).unwrap();
    assert_eq!(html, "");
}

#[test_log::test]
fn render_multiple_primitives() {
    let mut containers = Vec::new();

    // Add multiple primitive values
    42.render_to(&mut containers).unwrap();
    " - ".render_to(&mut containers).unwrap();
    true.render_to(&mut containers).unwrap();
    " - ".render_to(&mut containers).unwrap();
    3.25_f32.render_to(&mut containers).unwrap();

    let result = ContainerVecExt::display_to_string(&containers, false, false).unwrap();
    assert_eq!(result, "42 - true - 3.25");
}

#[test_log::test]
fn container_list_deref() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Test" }
    };

    let list = ContainerList::new(containers);

    // Test Deref functionality
    assert_eq!(list.len(), 1);
    assert!(!list.is_empty());
}

#[test_log::test]
fn calc_operations_preserve_units() {
    // Adding two vh values should preserve vh unit
    let result = calc::add_numbers(&Number::IntegerVh(50), &Number::IntegerVh(30));
    assert_eq!(result, Number::IntegerVh(80));

    // Subtracting dvw values should preserve dvw unit
    let result = calc::subtract_numbers(&Number::RealDvw(100.0), &Number::RealDvw(25.5));
    assert_eq!(result, Number::RealDvw(74.5));
}

#[test_log::test]
fn color_rgb_value_trait_edge_cases() {
    use color_functions::ToRgbValue;

    // Test u64 conversion and clamping
    assert_eq!(300_u64.to_rgb_value(), 255);
    assert_eq!(100_u64.to_rgb_value(), 100);

    // Test i64 conversion and clamping
    assert_eq!((-50_i64).to_rgb_value(), 0);
    assert_eq!(300_i64.to_rgb_value(), 255);

    // Test f64 conversion
    assert_eq!(127.5_f64.to_rgb_value(), 128);
    assert_eq!(300.0_f64.to_rgb_value(), 255);
    assert_eq!((-10.0_f64).to_rgb_value(), 0);
}

#[test_log::test]
fn alpha_value_from_various_integer_types() {
    use color_functions::AlphaValue;

    // Test u16
    assert_eq!(AlphaValue::from(128_u16).to_u8(), 128);
    assert_eq!(AlphaValue::from(300_u16).to_u8(), 255);

    // Test u32
    assert_eq!(AlphaValue::from(200_u32).to_u8(), 200);
    assert_eq!(AlphaValue::from(500_u32).to_u8(), 255);

    // Test u64
    assert_eq!(AlphaValue::from(100_u64).to_u8(), 100);
    assert_eq!(AlphaValue::from(1000_u64).to_u8(), 255);

    // Test i32
    assert_eq!(AlphaValue::from(150_i32).to_u8(), 150);
    assert_eq!(AlphaValue::from(-50_i32).to_u8(), 0);

    // Test i64
    assert_eq!(AlphaValue::from(200_i64).to_u8(), 200);
    assert_eq!(AlphaValue::from(300_i64).to_u8(), 255);
}

// ============================================================================
// RenderContainer additional type tests
// ============================================================================

#[test_log::test]
fn render_container_cow_borrowed() {
    use std::borrow::Cow;

    let text: Cow<'_, str> = Cow::Borrowed("Borrowed text");
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Borrowed text"
    );
}

#[test_log::test]
fn render_container_cow_owned() {
    use std::borrow::Cow;

    let text: Cow<'_, str> = Cow::Owned(String::from("Owned text"));
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Owned text"
    );
}

#[test_log::test]
fn render_container_cow_empty() {
    use std::borrow::Cow;

    let text: Cow<'_, str> = Cow::Borrowed("");
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    // Empty Cow strings should not add a container
    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn render_container_arguments() {
    let value = 42;
    let args = format_args!("Value is {}", value);
    let mut containers = Vec::new();
    args.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Value is 42"
    );
}

#[test_log::test]
fn render_container_render_method() {
    // Test the render() method which creates a new Vec
    let text = "Test render method";
    let containers = text.render().unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Test render method"
    );
}

#[test_log::test]
fn render_container_render_to_string_method() {
    // Test the render_to_string() method
    let number = 12345_i32;
    let result = number.render_to_string().unwrap();

    // In test mode, Container's Display impl may add debug formatting
    // (XML tags, syntax highlighting), so we check for containment
    assert!(
        result.contains("12345"),
        "Expected result to contain '12345', got: {result:?}"
    );
}

// ============================================================================
// ContainerList additional tests
// ============================================================================

#[test_log::test]
fn container_list_into_iterator_owned() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Item 1" }
        div { "Item 2" }
    };

    let list = ContainerList::new(containers);

    // Test owned into_iter
    let collected: Vec<_> = list.into_iter().collect();
    assert_eq!(collected.len(), 2);
}

#[test_log::test]
fn container_list_as_inner() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Content" }
    };

    let list = ContainerList::new(containers.clone());

    // Test as_inner
    let inner = list.as_inner();
    assert_eq!(inner.len(), 1);
    assert_eq!(inner, &containers);
}

#[test_log::test]
fn container_list_into_string() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Inline" }
    };

    let list = ContainerList::new(containers);
    let html = list.into_string();

    assert!(html.contains("Inline"));
}

#[test_log::test]
fn container_list_from_vec() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Test" }
    };

    let list: ContainerList = containers.clone().into();
    let back: Vec<hyperchad_transformer::Container> = list.into();

    assert_eq!(back, containers);
}

#[test_log::test]
fn container_list_deref_mut() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Original" }
    };

    let mut list = ContainerList::new(containers);

    // Test DerefMut - modify through deref
    list.push(hyperchad_transformer::Container {
        element: hyperchad_transformer::Element::Raw {
            value: "Added".to_string(),
        },
        ..Default::default()
    });

    assert_eq!(list.len(), 2);
}

// ============================================================================
// calc module - viewport unit preservation tests
// ============================================================================

#[test_log::test]
fn calc_to_vw_number_already_vw() {
    // Passing an IntegerVw should return it unchanged
    let result = calc::to_vw_number(Number::IntegerVw(50));
    assert_eq!(result, Number::IntegerVw(50));

    // Passing a RealVw should return it unchanged
    let result = calc::to_vw_number(Number::RealVw(75.5));
    assert_eq!(result, Number::RealVw(75.5));
}

#[test_log::test]
fn calc_to_vh_number_already_vh() {
    // Passing an IntegerVh should return it unchanged
    let result = calc::to_vh_number(Number::IntegerVh(100));
    assert_eq!(result, Number::IntegerVh(100));

    // Passing a RealVh should return it unchanged
    let result = calc::to_vh_number(Number::RealVh(90.0));
    assert_eq!(result, Number::RealVh(90.0));
}

#[test_log::test]
fn calc_to_dvw_number_already_dvw() {
    // Passing an IntegerDvw should return it unchanged
    let result = calc::to_dvw_number(Number::IntegerDvw(80));
    assert_eq!(result, Number::IntegerDvw(80));

    // Passing a RealDvw should return it unchanged
    let result = calc::to_dvw_number(Number::RealDvw(65.5));
    assert_eq!(result, Number::RealDvw(65.5));
}

#[test_log::test]
fn calc_to_dvh_number_already_dvh() {
    // Passing an IntegerDvh should return it unchanged
    let result = calc::to_dvh_number(Number::IntegerDvh(70));
    assert_eq!(result, Number::IntegerDvh(70));

    // Passing a RealDvh should return it unchanged
    let result = calc::to_dvh_number(Number::RealDvh(55.5));
    assert_eq!(result, Number::RealDvh(55.5));
}

#[test_log::test]
fn calc_to_percent_number_already_percent() {
    // Passing a RealPercent should return it unchanged
    let result = calc::to_percent_number(Number::RealPercent(50.5));
    assert_eq!(result, Number::RealPercent(50.5));
}

#[test_log::test]
fn calc_to_vw_number_from_percent() {
    // Converting a percentage to vw should use calc fallback
    let result = calc::to_vw_number(Number::IntegerPercent(50));
    // The result depends on the calc implementation
    assert!(matches!(result, Number::RealVw(_)));
}

#[test_log::test]
fn calc_add_numbers_dvw_units() {
    // Test dvw addition
    let result = calc::add_numbers(&Number::IntegerDvw(50), &Number::IntegerDvw(30));
    assert_eq!(result, Number::IntegerDvw(80));

    let result = calc::add_numbers(&Number::RealDvw(25.5), &Number::RealDvw(14.5));
    assert_eq!(result, Number::RealDvw(40.0));

    // Mixed integer/real dvw
    let result = calc::add_numbers(&Number::IntegerDvw(50), &Number::RealDvw(25.5));
    assert_eq!(result, Number::RealDvw(75.5));
}

#[test_log::test]
fn calc_add_numbers_dvh_units() {
    // Test dvh addition
    let result = calc::add_numbers(&Number::IntegerDvh(60), &Number::IntegerDvh(20));
    assert_eq!(result, Number::IntegerDvh(80));

    let result = calc::add_numbers(&Number::RealDvh(35.5), &Number::RealDvh(24.5));
    assert_eq!(result, Number::RealDvh(60.0));

    // Mixed integer/real dvh
    let result = calc::add_numbers(&Number::IntegerDvh(40), &Number::RealDvh(15.5));
    assert_eq!(result, Number::RealDvh(55.5));
}

#[test_log::test]
fn calc_subtract_numbers_dvw_units() {
    let result = calc::subtract_numbers(&Number::IntegerDvw(100), &Number::IntegerDvw(30));
    assert_eq!(result, Number::IntegerDvw(70));

    let result = calc::subtract_numbers(&Number::RealDvw(80.5), &Number::RealDvw(30.5));
    assert_eq!(result, Number::RealDvw(50.0));
}

#[test_log::test]
fn calc_subtract_numbers_dvh_units() {
    let result = calc::subtract_numbers(&Number::IntegerDvh(90), &Number::IntegerDvh(40));
    assert_eq!(result, Number::IntegerDvh(50));

    let result = calc::subtract_numbers(&Number::RealDvh(75.5), &Number::RealDvh(25.5));
    assert_eq!(result, Number::RealDvh(50.0));
}

#[test_log::test]
fn calc_divide_by_zero_integer_percent() {
    // Test division by zero with percentage types
    let result = calc::divide_numbers(&Number::IntegerPercent(100), &Number::Integer(0));
    assert_eq!(result, Number::RealPercent(0.0));
}

#[test_log::test]
fn calc_operations_different_units_fallback() {
    // Test operations between incompatible units fall back to pixel calculation
    let result = calc::add_numbers(&Number::IntegerVw(50), &Number::IntegerVh(50));
    // Result should be Real (pixels) after conversion
    assert!(matches!(result, Number::Real(_)));

    let result = calc::subtract_numbers(&Number::IntegerPercent(100), &Number::IntegerVw(50));
    assert!(matches!(result, Number::Real(_)));
}

// ============================================================================
// unit_functions module - additional tests
// ============================================================================

#[test_log::test]
fn unit_functions_vw_with_percent_input() {
    // vw with a percent Number should convert via fallback
    let result = unit_functions::vw(Number::IntegerPercent(50));
    assert!(matches!(result, Number::RealVw(_)));
}

#[test_log::test]
fn unit_functions_vh_with_vw_input() {
    // vh with a vw Number should convert via fallback
    let result = unit_functions::vh(Number::IntegerVw(50));
    assert!(matches!(result, Number::RealVh(_)));
}

#[test_log::test]
fn unit_functions_dvw_with_vh_input() {
    // dvw with a vh Number should convert via fallback
    let result = unit_functions::dvw(Number::IntegerVh(50));
    assert!(matches!(result, Number::RealDvw(_)));
}

#[test_log::test]
fn unit_functions_dvh_with_dvw_input() {
    // dvh with a dvw Number should convert via fallback
    let result = unit_functions::dvh(Number::IntegerDvw(50));
    assert!(matches!(result, Number::RealDvh(_)));
}

// ============================================================================
// ToBool trait tests
// ============================================================================

#[test_log::test]
fn to_bool_for_bool() {
    use hyperchad_template::ToBool;

    assert!(true.to_bool());
    assert!(!false.to_bool());
}

// ============================================================================
// IntoActionEffect trait tests
// ============================================================================

#[test_log::test]
fn into_action_effect_from_action_type() {
    use hyperchad_actions::ActionType;
    use hyperchad_template::IntoActionEffect;

    let action_type = ActionType::NoOp;
    let effect = IntoActionEffect::into_action_effect(action_type);

    assert!(matches!(effect.action, ActionType::NoOp));
}

#[test_log::test]
fn into_action_effect_from_action_effect() {
    use hyperchad_actions::{ActionEffect, ActionType};
    use hyperchad_template::IntoActionEffect;

    let effect = ActionEffect {
        action: ActionType::NoOp,
        ..Default::default()
    };
    let converted = IntoActionEffect::into_action_effect(effect);

    assert!(matches!(converted.action, ActionType::NoOp));
}

#[test_log::test]
fn into_action_effect_from_vec_action_type() {
    use hyperchad_actions::ActionType;
    use hyperchad_template::IntoActionEffect;

    let actions = vec![ActionType::NoOp, ActionType::NoOp];
    let effect = actions.into_action_effect();

    // Should create a MultiEffect wrapping the actions
    match effect.action {
        ActionType::MultiEffect(effects) => assert_eq!(effects.len(), 2),
        _ => panic!("Expected MultiEffect"),
    }
}

#[test_log::test]
fn into_action_effect_from_vec_action_effect() {
    use hyperchad_actions::{ActionEffect, ActionType};
    use hyperchad_template::IntoActionEffect;

    let effects = vec![
        ActionEffect {
            action: ActionType::NoOp,
            ..Default::default()
        },
        ActionEffect {
            action: ActionType::NoOp,
            ..Default::default()
        },
    ];
    let converted = effects.into_action_effect();

    // Should create a MultiEffect
    assert!(matches!(converted.action, ActionType::MultiEffect(_)));
}

#[test_log::test]
fn into_action_effect_from_action() {
    use hyperchad_actions::{Action, ActionEffect, ActionTrigger, ActionType};
    use hyperchad_template::IntoActionEffect;

    let action = Action {
        trigger: ActionTrigger::Click,
        effect: ActionEffect {
            action: ActionType::NoOp,
            ..Default::default()
        },
    };
    let effect = action.into_action_effect();

    assert!(matches!(effect.action, ActionType::NoOp));
}

// ============================================================================
// RenderContainer for Vec<Container> test
// ============================================================================

#[test_log::test]
fn render_container_vec_container() {
    use hyperchad_template::container;

    let first = container! { div { "First" } };
    let mut containers = Vec::new();
    first.render_to(&mut containers).unwrap();

    let second = container! { div { "Second" } };
    second.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 2);
}

// ============================================================================
// IntoBorder with u16 type tests
// ============================================================================

#[test_log::test]
fn into_border_u16_color() {
    let border = (Color::from_hex("#AABBCC"), 3_u16).into_border();
    assert_eq!(border.0, Color::from_hex("#AABBCC"));
    assert_eq!(border.1, Number::Integer(3));
}

#[test_log::test]
fn into_border_color_u16_reversed() {
    let border = (5_u16, Color::from_hex("#112233")).into_border();
    assert_eq!(border.0, Color::from_hex("#112233"));
    assert_eq!(border.1, Number::Integer(5));
}

#[test_log::test]
fn into_border_u16_str() {
    let border = (2_u16, "#DDEEFF").into_border();
    assert_eq!(border.0, Color::from_hex("#DDEEFF"));
    assert_eq!(border.1, Number::Integer(2));
}

#[test_log::test]
fn into_border_str_u16() {
    let border = ("#445566", 4_u16).into_border();
    assert_eq!(border.0, Color::from_hex("#445566"));
    assert_eq!(border.1, Number::Integer(4));
}

#[test_log::test]
fn into_border_u16_string() {
    let border = (3_u16, String::from("#778899")).into_border();
    assert_eq!(border.0, Color::from_hex("#778899"));
    assert_eq!(border.1, Number::Integer(3));
}

#[test_log::test]
fn into_border_string_u16() {
    let border = (String::from("#AABBCC"), 2_u16).into_border();
    assert_eq!(border.0, Color::from_hex("#AABBCC"));
    assert_eq!(border.1, Number::Integer(2));
}

// ============================================================================
// IntoBorder with f64 type tests (cast truncation path)
// ============================================================================

#[test_log::test]
fn into_border_f64_color() {
    let border = (Color::from_hex("#123456"), 2.5_f64).into_border();
    assert_eq!(border.0, Color::from_hex("#123456"));
    assert_eq!(border.1, Number::Real(2.5_f32));
}

#[test_log::test]
fn into_border_color_f64_reversed() {
    let border = (3.5_f64, Color::from_hex("#654321")).into_border();
    assert_eq!(border.0, Color::from_hex("#654321"));
    assert_eq!(border.1, Number::Real(3.5_f32));
}

#[test_log::test]
fn into_border_f64_str() {
    let border = (1.5_f64, "#ABCDEF").into_border();
    assert_eq!(border.0, Color::from_hex("#ABCDEF"));
    assert_eq!(border.1, Number::Real(1.5_f32));
}

#[test_log::test]
fn into_border_str_f64() {
    let border = ("#FEDCBA", 2.5_f64).into_border();
    assert_eq!(border.0, Color::from_hex("#FEDCBA"));
    assert_eq!(border.1, Number::Real(2.5_f32));
}

#[test_log::test]
fn into_border_f64_string() {
    let border = (4.5_f64, String::from("#999999")).into_border();
    assert_eq!(border.0, Color::from_hex("#999999"));
    assert_eq!(border.1, Number::Real(4.5_f32));
}

#[test_log::test]
fn into_border_string_f64() {
    let border = (String::from("#888888"), 5.5_f64).into_border();
    assert_eq!(border.0, Color::from_hex("#888888"));
    assert_eq!(border.1, Number::Real(5.5_f32));
}

// ============================================================================
// color_functions module - additional tests
// ============================================================================

#[test_log::test]
fn color_alpha_value_from_owned_string() {
    use color_functions::AlphaValue;

    // Test From<String> implementation
    let alpha = AlphaValue::from(String::from("75%"));
    assert_eq!(alpha.to_u8(), 191);

    let alpha = AlphaValue::from(String::from("0.5"));
    assert_eq!(alpha.to_u8(), 128);
}

#[test_log::test]
fn color_alpha_value_from_f64() {
    use color_functions::AlphaValue;

    // Test From<f64> implementation (cast truncation)
    let alpha = AlphaValue::from(0.5_f64);
    assert_eq!(alpha.to_u8(), 128);

    let alpha = AlphaValue::from(1.0_f64);
    assert_eq!(alpha.to_u8(), 255);
}

#[test_log::test]
fn color_rgb_with_u16() {
    // Test ToRgbValue for u16
    let color = color_functions::rgb(200_u16, 150_u16, 100_u16);
    assert_eq!(
        color,
        Color {
            r: 200,
            g: 150,
            b: 100,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgb_with_u32() {
    // Test ToRgbValue for u32
    let color = color_functions::rgb(255_u32, 128_u32, 64_u32);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 128,
            b: 64,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgb_with_i64() {
    // Test ToRgbValue for i64
    let color = color_functions::rgb(100_i64, 200_i64, 50_i64);
    assert_eq!(
        color,
        Color {
            r: 100,
            g: 200,
            b: 50,
            a: None
        }
    );

    // Test clamping for i64
    let color = color_functions::rgb(-50_i64, 300_i64, 128_i64);
    assert_eq!(
        color,
        Color {
            r: 0,
            g: 255,
            b: 128,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgb_with_u16_clamping() {
    // Test clamping for u16 values over 255
    let color = color_functions::rgb(500_u16, 1000_u16, 100_u16);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 255,
            b: 100,
            a: None
        }
    );
}

#[test_log::test]
fn color_rgb_with_u32_clamping() {
    // Test clamping for u32 values over 255
    let color = color_functions::rgb(1000_u32, 500_u32, 100_u32);
    assert_eq!(
        color,
        Color {
            r: 255,
            g: 255,
            b: 100,
            a: None
        }
    );
}

// ============================================================================
// ToBool trait tests for IfExpression (logic feature)
// ============================================================================

#[test_log::test]
fn to_bool_if_expression_with_default_only() {
    use hyperchad_actions::logic::{IfExpression, Responsive};
    use hyperchad_template::ToBool;

    // When only default is set, it should use the default
    let expr = IfExpression {
        condition: Responsive::Target("mobile".to_string()),
        value: None,
        default: Some(true),
    };
    assert!(expr.to_bool());

    let expr = IfExpression {
        condition: Responsive::Target("desktop".to_string()),
        value: None,
        default: Some(false),
    };
    assert!(!expr.to_bool());
}

#[test_log::test]
fn to_bool_if_expression_with_value_only() {
    use hyperchad_actions::logic::{IfExpression, Responsive};
    use hyperchad_template::ToBool;

    // When only value is set (no default), it should use value
    let expr = IfExpression {
        condition: Responsive::Target("mobile".to_string()),
        value: Some(true),
        default: None,
    };
    assert!(expr.to_bool());

    let expr = IfExpression {
        condition: Responsive::Target("desktop".to_string()),
        value: Some(false),
        default: None,
    };
    assert!(!expr.to_bool());
}

#[test_log::test]
fn to_bool_if_expression_prefers_default_over_value() {
    use hyperchad_actions::logic::{IfExpression, Responsive};
    use hyperchad_template::ToBool;

    // When both default and value are set, default takes precedence
    let expr = IfExpression {
        condition: Responsive::Target("mobile".to_string()),
        value: Some(false),
        default: Some(true),
    };
    assert!(expr.to_bool()); // default wins

    let expr = IfExpression {
        condition: Responsive::Target("desktop".to_string()),
        value: Some(true),
        default: Some(false),
    };
    assert!(!expr.to_bool()); // default wins
}

#[test_log::test]
fn to_bool_if_expression_neither_set_uses_default_bool() {
    use hyperchad_actions::logic::{IfExpression, Responsive};
    use hyperchad_template::ToBool;

    // When neither default nor value is set, fall back to bool::default() = false
    let expr: IfExpression<bool, Responsive> = IfExpression {
        condition: Responsive::Target("tablet".to_string()),
        value: None,
        default: None,
    };
    assert!(!expr.to_bool());
}

#[test_log::test]
fn to_bool_if_expression_with_multiple_targets() {
    use hyperchad_actions::logic::{IfExpression, Responsive};
    use hyperchad_template::ToBool;

    // Works with Responsive::Targets as well
    let expr = IfExpression {
        condition: Responsive::Targets(vec!["mobile".to_string(), "tablet".to_string()]),
        value: Some(true),
        default: None,
    };
    assert!(expr.to_bool());
}

// ============================================================================
// IntoActionEffect trait tests for logic::If
// ============================================================================

#[test_log::test]
fn into_action_effect_from_logic_if() {
    use hyperchad_actions::ActionType;
    use hyperchad_actions::logic::{Condition, If, hidden, visible};
    use hyperchad_template::IntoActionEffect;

    let if_logic = If {
        condition: Condition::Eq(visible(), hidden()),
        actions: vec![],
        else_actions: vec![],
    };
    let effect = if_logic.into_action_effect();

    assert!(matches!(effect.action, ActionType::Logic(_)));
}

#[test_log::test]
fn into_action_effect_from_logic_if_with_actions() {
    use hyperchad_actions::logic::{Condition, If, hidden, visible};
    use hyperchad_actions::{ActionEffect, ActionType};
    use hyperchad_template::IntoActionEffect;

    let if_logic = If {
        condition: Condition::Eq(visible(), hidden()),
        actions: vec![ActionEffect {
            action: ActionType::NoOp,
            ..Default::default()
        }],
        else_actions: vec![ActionEffect {
            action: ActionType::NoOp,
            ..Default::default()
        }],
    };
    let effect = if_logic.into_action_effect();

    match effect.action {
        ActionType::Logic(if_action) => {
            assert_eq!(if_action.actions.len(), 1);
            assert_eq!(if_action.else_actions.len(), 1);
        }
        _ => panic!("Expected Logic action type"),
    }
}

// ============================================================================
// calc::to_number tests
// ============================================================================

#[test_log::test]
fn calc_to_number_from_integer() {
    let result = calc::to_number(42_i64);
    assert_eq!(result, Number::Integer(42));
}

#[test_log::test]
fn calc_to_number_from_float() {
    let result = calc::to_number(2.75_f32);
    assert_eq!(result, Number::Real(2.75));
}

#[test_log::test]
fn calc_to_number_from_number() {
    // Passing a Number should return it unchanged
    let result = calc::to_number(Number::IntegerVw(50));
    assert_eq!(result, Number::IntegerVw(50));
}

// ============================================================================
// calc::multiply_numbers additional tests
// ============================================================================

#[test_log::test]
fn calc_multiply_numbers_integer_real_mix() {
    let result = calc::multiply_numbers(&Number::Integer(5), &Number::Real(2.5));
    assert_eq!(result, Number::Real(12.5));

    let result = calc::multiply_numbers(&Number::Real(3.0), &Number::Integer(4));
    assert_eq!(result, Number::Real(12.0));
}

#[test_log::test]
fn calc_multiply_numbers_real_percent_scalar() {
    // RealPercent * Real should preserve percentage
    let result = calc::multiply_numbers(&Number::RealPercent(50.0), &Number::Real(2.0));
    assert_eq!(result, Number::RealPercent(100.0));

    // Real * RealPercent should preserve percentage
    let result = calc::multiply_numbers(&Number::Real(3.0), &Number::RealPercent(25.0));
    assert_eq!(result, Number::RealPercent(75.0));
}

#[test_log::test]
fn calc_multiply_numbers_viewport_units_fallback() {
    // Multiplying viewport units falls back to pixel conversion
    let result = calc::multiply_numbers(&Number::IntegerVw(50), &Number::IntegerVh(50));
    // Result should be Real after pixel conversion
    assert!(matches!(result, Number::Real(_)));
}

#[test_log::test]
fn calc_multiply_numbers_mixed_unit_types() {
    // Multiplying Vw by percent should fall back to pixel conversion
    let result = calc::multiply_numbers(&Number::IntegerVw(50), &Number::IntegerPercent(50));
    assert!(matches!(result, Number::Real(_)));

    // Multiplying different viewport units
    let result = calc::multiply_numbers(&Number::RealDvw(25.0), &Number::RealDvh(30.0));
    assert!(matches!(result, Number::Real(_)));
}

// ============================================================================
// calc::divide_numbers additional tests
// ============================================================================

#[test_log::test]
fn calc_divide_numbers_integer_real_mix() {
    let result = calc::divide_numbers(&Number::Integer(10), &Number::Real(2.5));
    assert_eq!(result, Number::Real(4.0));

    let result = calc::divide_numbers(&Number::Real(15.0), &Number::Integer(3));
    assert_eq!(result, Number::Real(5.0));
}

#[test_log::test]
fn calc_divide_numbers_integer_real_by_zero() {
    // Integer / Real(0.0) should return 0.0
    let result = calc::divide_numbers(&Number::Integer(10), &Number::Real(0.0));
    assert_eq!(result, Number::Real(0.0));

    // Real / Integer(0) should return 0.0
    let result = calc::divide_numbers(&Number::Real(10.0), &Number::Integer(0));
    assert_eq!(result, Number::Real(0.0));
}

#[test_log::test]
fn calc_divide_numbers_integer_percent_by_integer() {
    let result = calc::divide_numbers(&Number::IntegerPercent(100), &Number::Integer(4));
    assert_eq!(result, Number::RealPercent(25.0));
}

#[test_log::test]
fn calc_divide_numbers_real_percent_by_real_zero() {
    // Division by zero should return 0.0
    let result = calc::divide_numbers(&Number::RealPercent(100.0), &Number::Real(0.0));
    assert_eq!(result, Number::RealPercent(0.0));
}

#[test_log::test]
fn calc_divide_numbers_viewport_units_fallback() {
    // Dividing viewport units by other viewport units falls back to pixel conversion
    let result = calc::divide_numbers(&Number::IntegerVw(100), &Number::IntegerVh(50));
    assert!(matches!(result, Number::Real(_)));

    // Non-zero result check
    if let Number::Real(value) = result {
        assert!(value != 0.0, "Expected non-zero result for valid division");
    }
}

#[test_log::test]
fn calc_divide_numbers_fallback_by_zero() {
    // Fallback path division by zero (when units don't match specific patterns)
    // IntegerVw / IntegerVw(0) will evaluate to 0 via calc
    let result = calc::divide_numbers(&Number::IntegerVw(100), &Number::IntegerVw(0));
    // The calc function will return 0 for the second operand, triggering the zero check
    assert_eq!(result, Number::Real(0.0));
}

// ============================================================================
// calc::subtract_numbers additional tests
// ============================================================================

#[test_log::test]
fn calc_subtract_numbers_vw_mixed_integer_real() {
    let result = calc::subtract_numbers(&Number::IntegerVw(100), &Number::RealVw(25.5));
    assert_eq!(result, Number::RealVw(74.5));

    let result = calc::subtract_numbers(&Number::RealVw(100.0), &Number::IntegerVw(30));
    assert_eq!(result, Number::RealVw(70.0));
}

#[test_log::test]
fn calc_subtract_numbers_vh_mixed_integer_real() {
    let result = calc::subtract_numbers(&Number::IntegerVh(80), &Number::RealVh(20.5));
    assert_eq!(result, Number::RealVh(59.5));

    let result = calc::subtract_numbers(&Number::RealVh(90.0), &Number::IntegerVh(40));
    assert_eq!(result, Number::RealVh(50.0));
}

#[test_log::test]
fn calc_subtract_numbers_percent_mixed_integer_real() {
    let result = calc::subtract_numbers(&Number::IntegerPercent(100), &Number::RealPercent(33.3));
    // Expected: 100.0 - 33.3 = 66.7
    if let Number::RealPercent(val) = result {
        assert!((val - 66.7).abs() < 0.01);
    } else {
        panic!("Expected RealPercent, got {:?}", result);
    }

    let result = calc::subtract_numbers(&Number::RealPercent(75.0), &Number::IntegerPercent(25));
    assert_eq!(result, Number::RealPercent(50.0));
}

// ============================================================================
// fx function runtime tests
// ============================================================================

#[test_log::test]
fn fx_with_action_type() {
    use hyperchad_actions::ActionType;
    use hyperchad_template::fx;

    let effect = fx(ActionType::NoOp);
    assert!(matches!(effect.action, ActionType::NoOp));
}

#[test_log::test]
fn fx_with_action_effect() {
    use hyperchad_actions::{ActionEffect, ActionType};
    use hyperchad_template::fx;

    let input_effect = ActionEffect {
        action: ActionType::NoOp,
        ..Default::default()
    };
    let result = fx(input_effect);
    assert!(matches!(result.action, ActionType::NoOp));
}

#[test_log::test]
fn fx_with_vec_action_type() {
    use hyperchad_actions::ActionType;
    use hyperchad_template::fx;

    let actions = vec![ActionType::NoOp, ActionType::NoOp];
    let effect = fx(actions);

    match effect.action {
        ActionType::MultiEffect(effects) => {
            assert_eq!(effects.len(), 2);
        }
        _ => panic!("Expected MultiEffect"),
    }
}

// ============================================================================
// Container RenderContainer tests
// ============================================================================

#[test_log::test]
fn render_container_direct_container() {
    use hyperchad_transformer::{Container, Element};

    let container = Container {
        element: Element::Raw {
            value: "Direct Container".to_string(),
        },
        ..Default::default()
    };

    let mut containers = Vec::new();
    container.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(
        ContainerVecExt::display_to_string(&containers, false, false).unwrap(),
        "Direct Container"
    );
}

#[test_log::test]
fn render_container_container_with_element() {
    use hyperchad_transformer::{Container, Element};

    let container = Container {
        element: Element::Div,
        ..Default::default()
    };

    let mut containers = Vec::new();
    container.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::Div));
}

#[test_log::test]
fn render_container_container_with_children() {
    use hyperchad_transformer::{Container, Element};

    let child = Container {
        element: Element::Raw {
            value: "Child".to_string(),
        },
        ..Default::default()
    };

    let parent = Container {
        element: Element::Div,
        children: vec![child],
        ..Default::default()
    };

    let mut containers = Vec::new();
    parent.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 1);
}
