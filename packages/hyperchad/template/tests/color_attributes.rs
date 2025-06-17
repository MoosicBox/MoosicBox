use hyperchad_template::{ContainerVecExt, container};

#[test]
fn raw_color_names() {
    let result = container! {
        div color=white background=black {
            "White text on black background"
        }
    };
    // Note: Color attributes are handled by the color processing system
    // and won't appear directly in the HTML output, but the test ensures parsing works
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_color_names_various() {
    let result = container! {
        div color=red background=blue {
            "Red text on blue background"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_color_names_extended() {
    let result = container! {
        div color=gray background=purple {
            "Gray text on purple background"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_3_digit() {
    let result = container! {
        div color=#fff background=#000 {
            "3-digit hex colors"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_6_digit() {
    let result = container! {
        div color=#ffffff background=#000000 {
            "6-digit hex colors"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_numeric() {
    let result = container! {
        div color=#123 background=#456 {
            "Numeric hex colors"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_all_zeros() {
    let result = container! {
        div color=#000 background=#fff {
            "All zeros hex color"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_mixed_letters_numbers() {
    let result = container! {
        div color=#a1b background=#c2d {
            "Mixed letters and numbers hex"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_all_letters() {
    let result = container! {
        div color=#abc background=#def {
            "All letters hex colors"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn mixed_color_types() {
    let result = container! {
        div color=red background=#ffffff {
            "Color name with hex background"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn mixed_color_types_reverse() {
    let result = container! {
        div color=#ff0000 background=white {
            "Hex color with color name background"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn quoted_colors_still_work() {
    let result = container! {
        div color="white" background="black" {
            "Quoted color names"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn quoted_hex_colors_still_work() {
    let result = container! {
        div color="#ff0000" background="#0000ff" {
            "Quoted hex colors"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn mixed_quoted_and_unquoted() {
    let result = container! {
        div color=red background="#00ff00" {
            "Mixed quoted and unquoted"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn multiple_elements_with_colors() {
    let result = container! {
        div color=white background=black { "First" }
        div color=#fff background=#000 { "Second" }
        div color="red" background="blue" { "Third" }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn nested_elements_with_colors() {
    let result = container! {
        div color=white background=black {
            span color=red { "Red text" }
            div color=#0000ff background=#ffffff {
                "Blue text on white"
            }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn color_with_other_attributes() {
    let result = container! {
        div color=white background=black width="100" height="50" .my-class #my-id {
            "Colors with other attributes"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn all_supported_color_names() {
    let result = container! {
        div {
            div color=black { "black" }
            div color=white { "white" }
            div color=red { "red" }
            div color=green { "green" }
            div color=blue { "blue" }
            div color=gray { "gray" }
            div color=yellow { "yellow" }
            div color=cyan { "cyan" }
            div color=magenta { "magenta" }
            div color=orange { "orange" }
            div color=purple { "purple" }
            div color=pink { "pink" }
            div color=brown { "brown" }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn hex_color_edge_cases() {
    let result = container! {
        div {
            // Minimum valid 3-digit hex
            div color=#000 { "000" }
            div color=#fff { "fff" }
            // Maximum valid 6-digit hex
            div color=#ffffff { "ffffff" }
            div color=#000000 { "000000" }
            // Mixed case scenarios
            div color=#AbC { "AbC" }
            div color=#123abc { "123abc" }
            div color=#abc123 { "abc123" }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn color_attributes_with_responsive() {
    let result = container! {
        div color=white background=black width="100" height="50" {
            "Responsive with colors"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn colors_in_complex_layout() {
    let result = container! {
        div color=white background=black {
            header color=#ffffff background=#333333 {
                h1 color=yellow { "Title" }
            }
            main color=#000000 background=#f0f0f0 {
                div color=red { "Content" }
                div color=#0066cc { "More content" }
            }
            footer color=gray background=white {
                span color=#666 { "Footer text" }
            }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_8_digit_rgba() {
    let result = container! {
        div color=#445566ff background=#12345678 {
            "8-digit hex colors with alpha channel"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_8_digit_mixed() {
    let result = container! {
        div color=#abcdef80 background=#ff00ff33 {
            "8-digit hex colors with various alpha values"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn raw_hex_colors_8_digit_edge_cases() {
    let result = container! {
        div {
            // Fully opaque (ff alpha)
            div color=#000000ff { "Fully opaque black" }
            // Fully transparent (00 alpha)
            div color=#ffffff00 { "Fully transparent white" }
            // Semi-transparent (80 alpha)
            div color=#ff000080 { "Semi-transparent red" }
            // Mixed case
            div color=#AbCdEf12 { "Mixed case 8-digit" }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn rgb_function_basic() {
    let result = container! {
        div color=rgb(255, 0, 0) background=rgb(0, 255, 0) {
            "RGB color functions"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn rgba_function_basic() {
    let result = container! {
        div color=rgba(255, 0, 0, 0.5) background=rgba(0, 255, 0, 128) {
            "RGBA color functions"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn rgb_function_with_variables() {
    let red = 255;
    let green = 100;
    let blue = 50;
    let result = container! {
        div color=rgb(red, green, blue) {
            "RGB with variables"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn rgba_function_with_variables() {
    let red = 255;
    let green = 100;
    let blue = 50;
    let alpha = 0.75;
    let result = container! {
        div color=rgba(red, green, blue, alpha) {
            "RGBA with variables"
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn rgb_rgba_mixed_usage() {
    let result = container! {
        div {
            // RGB function
            div color=rgb(255, 0, 0) { "Pure red" }

            // RGBA with float alpha
            div color=rgba(0, 255, 0, 0.5) { "Semi-transparent green" }

            // RGBA with integer alpha
            div color=rgba(0, 0, 255, 128) { "Semi-transparent blue" }

            // Mixed with hex colors
            div color=rgb(128, 128, 128) background=#ffffff { "Gray on white" }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}

#[test]
fn color_functions_comprehensive() {
    let red_val = 200;
    let green_val = 100;
    let blue_val = 50;
    let alpha_val = 0.8;

    let result = container! {
        div {
            // All color formats in one container
            div color=rgb(red_val, green_val, blue_val)
                background=rgba(255, 255, 255, alpha_val) {
                "All color formats combined"
            }

            // Edge cases
            div color=rgb(0, 0, 0) background=rgba(255, 255, 255, 1.0) {
                "Black text on white background"
            }

            div color=rgb(255, 255, 255) background=rgba(0, 0, 0, 0.9) {
                "White text on mostly opaque black"
            }
        }
    };
    assert!(result.display_to_string(false, false).is_ok());
}
