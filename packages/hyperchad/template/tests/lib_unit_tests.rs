//! Unit tests for hyperchad_template library functions
//!
//! These tests focus on testing the library code directly rather than
//! the macro system, covering RenderContainer implementations, helper
//! functions, and trait implementations.

use hyperchad_color::Color;
use hyperchad_template::{
    calc, color_functions, unit_functions, ContainerList, ContainerVecExt, Containers, IntoBorder,
    RenderContainer,
};
use hyperchad_transformer::Number;
use pretty_assertions::assert_eq;
use std::sync::Arc;

// ============================================================================
// RenderContainer trait implementation tests
// ============================================================================

#[test]
fn render_container_str() {
    let text = "Hello, World!";
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(ContainerVecExt::to_string(&containers), "Hello, World!");
}

#[test]
fn render_container_str_empty() {
    let text = "";
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    // Empty strings should not add a container
    assert_eq!(containers.len(), 0);
}

#[test]
fn render_container_string() {
    let text = String::from("Test String");
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(ContainerVecExt::to_string(&containers), "Test String");
}

#[test]
fn render_container_string_empty() {
    let text = String::new();
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();

    // Empty strings should not add a container
    assert_eq!(containers.len(), 0);
}

#[test]
fn render_container_bool() {
    let mut containers_true = Vec::new();
    true.render_to(&mut containers_true).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_true), "true");

    let mut containers_false = Vec::new();
    false.render_to(&mut containers_false).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_false), "false");
}

#[test]
fn render_container_char() {
    let mut containers = Vec::new();
    'A'.render_to(&mut containers).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers), "A");

    let mut containers_unicode = Vec::new();
    '🦀'.render_to(&mut containers_unicode).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_unicode), "🦀");
}

#[test]
fn render_container_integers() {
    // Test various integer types
    let mut containers_i8 = Vec::new();
    42_i8.render_to(&mut containers_i8).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_i8), "42");

    let mut containers_i16 = Vec::new();
    (-1234_i16).render_to(&mut containers_i16).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_i16), "-1234");

    let mut containers_i32 = Vec::new();
    1_000_000_i32.render_to(&mut containers_i32).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_i32), "1000000");

    let mut containers_u8 = Vec::new();
    255_u8.render_to(&mut containers_u8).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_u8), "255");

    let mut containers_u64 = Vec::new();
    18_446_744_073_709_551_615_u64
        .render_to(&mut containers_u64)
        .unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_u64), "18446744073709551615");
}

#[test]
fn render_container_floats() {
    let mut containers_f32 = Vec::new();
    3.15_f32.render_to(&mut containers_f32).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_f32), "3.15");

    let mut containers_f64 = Vec::new();
    2.7_f64.render_to(&mut containers_f64).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_f64), "2.7");

    // Test zero
    let mut containers_zero = Vec::new();
    0.0_f32.render_to(&mut containers_zero).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_zero), "0.0");
}

#[test]
fn render_container_option_some() {
    let value = Some("Present");
    let mut containers = Vec::new();
    value.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(ContainerVecExt::to_string(&containers), "Present");
}

#[test]
fn render_container_option_none() {
    let value: Option<&str> = None;
    let mut containers = Vec::new();
    value.render_to(&mut containers).unwrap();

    // None should not add any containers
    assert_eq!(containers.len(), 0);
}

#[test]
fn render_container_reference_types() {
    // Test &T implementation
    let text = String::from("Referenced");
    let mut containers = Vec::new();
    text.render_to(&mut containers).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers), "Referenced");

    // Test &mut T implementation
    let number = 42;
    let mut containers_mut = Vec::new();
    number.render_to(&mut containers_mut).unwrap();
    assert_eq!(ContainerVecExt::to_string(&containers_mut), "42");
}

#[test]
fn render_container_box() {
    let boxed = Box::new("Boxed Value");
    let mut containers = Vec::new();
    boxed.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(ContainerVecExt::to_string(&containers), "Boxed Value");
}

#[test]
fn render_container_arc() {
    let arc = Arc::new("Arc Value");
    let mut containers = Vec::new();
    arc.render_to(&mut containers).unwrap();

    assert_eq!(containers.len(), 1);
    assert_eq!(ContainerVecExt::to_string(&containers), "Arc Value");
}

// ============================================================================
// ContainerVecMethods and ContainerVecExt tests
// ============================================================================

#[test]
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

#[test]
fn container_list_new_and_into_inner() {
    use hyperchad_template::container;

    let containers = container! {
        div { "Content" }
    };

    let list = ContainerList::new(containers.clone());
    let inner = list.into_inner();

    assert_eq!(inner, containers);
}

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
fn calc_subtract_numbers_same_unit() {
    let result = calc::subtract_numbers(&Number::Integer(30), &Number::Integer(10));
    assert_eq!(result, Number::Integer(20));

    let result = calc::subtract_numbers(&Number::RealPercent(75.5), &Number::RealPercent(25.5));
    assert_eq!(result, Number::RealPercent(50.0));
}

#[test]
fn calc_subtract_numbers_negative_result() {
    let result = calc::subtract_numbers(&Number::Integer(10), &Number::Integer(20));
    assert_eq!(result, Number::Integer(-10));
}

#[test]
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

#[test]
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

#[test]
fn calc_divide_by_zero() {
    // Division by zero should return 0.0 to avoid panics
    let result = calc::divide_numbers(&Number::Integer(10), &Number::Integer(0));
    assert_eq!(result, Number::Real(0.0));

    let result = calc::divide_numbers(&Number::Real(10.0), &Number::Real(0.0));
    assert_eq!(result, Number::Real(0.0));
}

#[test]
fn calc_to_percent_number() {
    let result = calc::to_percent_number(50);
    assert_eq!(result, Number::IntegerPercent(50));

    let result = calc::to_percent_number(75.5);
    assert_eq!(result, Number::RealPercent(75.5));

    // Already a percent type should remain unchanged
    let result = calc::to_percent_number(Number::IntegerPercent(100));
    assert_eq!(result, Number::IntegerPercent(100));
}

#[test]
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

#[test]
fn unit_functions_vw() {
    assert_eq!(unit_functions::vw(50), Number::IntegerVw(50));
    assert_eq!(unit_functions::vw(75.5), Number::RealVw(75.5));
}

#[test]
fn unit_functions_vh() {
    assert_eq!(unit_functions::vh(100), Number::IntegerVh(100));
    assert_eq!(unit_functions::vh(50.5), Number::RealVh(50.5));
}

#[test]
fn unit_functions_dvw() {
    assert_eq!(unit_functions::dvw(80), Number::IntegerDvw(80));
    assert_eq!(unit_functions::dvw(90.5), Number::RealDvw(90.5));
}

#[test]
fn unit_functions_dvh() {
    assert_eq!(unit_functions::dvh(60), Number::IntegerDvh(60));
    assert_eq!(unit_functions::dvh(70.5), Number::RealDvh(70.5));
}

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
fn color_alpha_value_clamping() {
    use color_functions::AlphaValue;

    // Float clamping
    assert_eq!(AlphaValue::Float(-0.5).to_u8(), 0);
    assert_eq!(AlphaValue::Float(1.5).to_u8(), 255);

    // Percentage clamping
    assert_eq!(AlphaValue::Percentage(-10.0).to_u8(), 0);
    assert_eq!(AlphaValue::Percentage(150.0).to_u8(), 255);
}

#[test]
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

#[test]
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

#[test]
fn into_border_color_number() {
    let border = (Color::from_hex("#FF0000"), Number::Integer(2)).into_border();
    assert_eq!(border.0, Color::from_hex("#FF0000"));
    assert_eq!(border.1, Number::Integer(2));
}

#[test]
fn into_border_color_i32() {
    let border = (Color::from_hex("#00FF00"), 4_i32).into_border();
    assert_eq!(border.0, Color::from_hex("#00FF00"));
    assert_eq!(border.1, Number::Integer(4));
}

#[test]
fn into_border_color_f32() {
    let border = (Color::from_hex("#0000FF"), 2.5_f32).into_border();
    assert_eq!(border.0, Color::from_hex("#0000FF"));
    assert_eq!(border.1, Number::Real(2.5));
}

#[test]
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

#[test]
fn into_border_hex_string() {
    let border = ("#FF0000", 2_i32).into_border();
    assert_eq!(border.0, Color::from_hex("#FF0000"));
    assert_eq!(border.1, Number::Integer(2));

    let border = (2_i32, "#00FF00").into_border();
    assert_eq!(border.0, Color::from_hex("#00FF00"));
    assert_eq!(border.1, Number::Integer(2));
}

#[test]
fn into_border_owned_string() {
    let color_str = String::from("#0000FF");
    let border = (color_str, 3_i32).into_border();
    assert_eq!(border.0, Color::from_hex("#0000FF"));
    assert_eq!(border.1, Number::Integer(3));
}

// ============================================================================
// Helper function tests
// ============================================================================

#[test]
fn to_html_helper() {
    use hyperchad_template::{container, to_html};

    let containers = container! {
        div { "Test Content" }
    };

    let html = to_html(&containers);
    assert!(html.contains("Test Content"));
}

#[test]
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

#[test]
fn render_to_string_empty_containers() {
    let containers: Containers = Vec::new();
    let html = ContainerVecExt::to_string(&containers);
    assert_eq!(html, "");
}

#[test]
fn render_multiple_primitives() {
    let mut containers = Vec::new();

    // Add multiple primitive values
    42.render_to(&mut containers).unwrap();
    " - ".render_to(&mut containers).unwrap();
    true.render_to(&mut containers).unwrap();
    " - ".render_to(&mut containers).unwrap();
    3.25_f32.render_to(&mut containers).unwrap();

    let result = ContainerVecExt::to_string(&containers);
    assert_eq!(result, "42 - true - 3.25");
}

#[test]
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

#[test]
fn calc_operations_preserve_units() {
    // Adding two vh values should preserve vh unit
    let result = calc::add_numbers(&Number::IntegerVh(50), &Number::IntegerVh(30));
    assert_eq!(result, Number::IntegerVh(80));

    // Subtracting dvw values should preserve dvw unit
    let result = calc::subtract_numbers(&Number::RealDvw(100.0), &Number::RealDvw(25.5));
    assert_eq!(result, Number::RealDvw(74.5));
}

#[test]
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

#[test]
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
