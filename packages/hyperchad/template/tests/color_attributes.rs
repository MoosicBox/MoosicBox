use hyperchad_color::Color;
use hyperchad_template::*;

#[test]
fn raw_color_names() {
    let result = container! {
        div color=white background=black {
            "White text on black background"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::WHITE));
    assert_eq!(container.background, Some(Color::BLACK));
}

#[test]
fn raw_color_names_various() {
    let result = container! {
        div color=red background=blue {
            "Red text on blue background"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#FF0000")));
    assert_eq!(container.background, Some(Color::from_hex("#0000FF")));
}

#[test]
fn raw_color_names_extended() {
    let result = container! {
        div color=green background=yellow {
            "Green text on yellow background"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#00FF00")));
    assert_eq!(container.background, Some(Color::from_hex("#FFFF00")));
}

#[test]
fn raw_hex_colors_3_digit() {
    let result = container! {
        div color=#fff background=#000 {
            "3-digit hex colors"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#fff")));
    assert_eq!(container.background, Some(Color::from_hex("#000")));
}

#[test]
fn raw_hex_colors_6_digit() {
    let result = container! {
        div color=#ffffff background=#000000 {
            "6-digit hex colors"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#ffffff")));
    assert_eq!(container.background, Some(Color::from_hex("#000000")));
}

#[test]
fn raw_hex_colors_numeric() {
    let result = container! {
        div color=#123 background=#456 {
            "Numeric hex colors"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#123")));
    assert_eq!(container.background, Some(Color::from_hex("#456")));
}

#[test]
fn raw_hex_colors_all_zeros() {
    let result = container! {
        div color=#000 background=#000000 {
            "All zeros hex colors"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#000")));
    assert_eq!(container.background, Some(Color::from_hex("#000000")));
}

#[test]
fn raw_hex_colors_mixed_letters_numbers() {
    let result = container! {
        div color=#a1b2c3 background=#d4e5f6 {
            "Mixed letters and numbers"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#a1b2c3")));
    assert_eq!(container.background, Some(Color::from_hex("#d4e5f6")));
}

#[test]
fn raw_hex_colors_all_letters() {
    let result = container! {
        div color=#abc background=#def {
            "All letters hex colors"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#abc")));
    assert_eq!(container.background, Some(Color::from_hex("#def")));
}

#[test]
fn mixed_color_types() {
    let result = container! {
        div color=white background=#000000 {
            "Mixed color name and hex"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::WHITE));
    assert_eq!(container.background, Some(Color::from_hex("#000000")));
}

#[test]
fn mixed_color_types_reverse() {
    let result = container! {
        div color=#ffffff background=black {
            "Mixed hex and color name"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#ffffff")));
    assert_eq!(container.background, Some(Color::BLACK));
}

#[test]
fn quoted_colors_still_work() {
    let result = container! {
        div color="white" background="black" {
            "Quoted color names"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::WHITE));
    assert_eq!(container.background, Some(Color::BLACK));
}

#[test]
fn quoted_hex_colors_still_work() {
    let result = container! {
        div color="#ffffff" background="#000000" {
            "Quoted hex colors"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#ffffff")));
    assert_eq!(container.background, Some(Color::from_hex("#000000")));
}

#[test]
fn mixed_quoted_and_unquoted() {
    let result = container! {
        div color="white" background=#000000 {
            "Mixed quoted and unquoted"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::WHITE));
    assert_eq!(container.background, Some(Color::from_hex("#000000")));
}

#[test]
fn multiple_elements_with_colors() {
    let result = container! {
        div {
            div color=red { "Red text" }
            div color=blue { "Blue text" }
            div color=green { "Green text" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 3);

    assert_eq!(parent.children[0].color, Some(Color::from_hex("#FF0000")));
    assert_eq!(parent.children[1].color, Some(Color::from_hex("#0000FF")));
    assert_eq!(parent.children[2].color, Some(Color::from_hex("#00FF00")));
}

#[test]
fn nested_elements_with_colors() {
    let result = container! {
        div color=white background=black {
            div color=red {
                span color=yellow { "Nested colors" }
            }
        }
    };

    assert_eq!(result.len(), 1);
    let root = &result[0];
    assert_eq!(root.color, Some(Color::WHITE));
    assert_eq!(root.background, Some(Color::BLACK));

    assert_eq!(root.children.len(), 1);
    let child = &root.children[0];
    assert_eq!(child.color, Some(Color::from_hex("#FF0000")));

    assert_eq!(child.children.len(), 1);
    let grandchild = &child.children[0];
    assert_eq!(grandchild.color, Some(Color::from_hex("#FFFF00")));
}

#[test]
fn color_with_other_attributes() {
    let result = container! {
        div color=white background=black width="100" height="50" {
            "Color with other attributes"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::WHITE));
    assert_eq!(container.background, Some(Color::BLACK));
    // Also verify other attributes work
    assert!(container.width.is_some());
    assert!(container.height.is_some());
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

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 13);

    let expected_colors = [
        Color::BLACK,
        Color::WHITE,
        Color::from_hex("#FF0000"), // red
        Color::from_hex("#00FF00"), // green
        Color::from_hex("#0000FF"), // blue
        Color::from_hex("#808080"), // gray
        Color::from_hex("#FFFF00"), // yellow
        Color::from_hex("#00FFFF"), // cyan
        Color::from_hex("#FF00FF"), // magenta
        Color::from_hex("#FFA500"), // orange
        Color::from_hex("#800080"), // purple
        Color::from_hex("#FFC0CB"), // pink
        Color::from_hex("#A52A2A"), // brown
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn hex_color_edge_cases() {
    let result = container! {
        div {
            // Mixed case letters
            div color=#FfF { "FfF" }
            div color=#AaA { "AaA" }
            // All numbers
            div color=#000 { "000" }
            div color=#123 { "123" }
            div color=#999 { "999" }
            // 6-digit variations
            div color=#000000 { "000000" }
            div color=#AbC123 { "AbC123" }
            div color=#123abc { "123abc" }
            div color=#abc123 { "abc123" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 9);

    let expected_colors = [
        Color::from_hex("#FfF"),
        Color::from_hex("#AaA"),
        Color::from_hex("#000"),
        Color::from_hex("#123"),
        Color::from_hex("#999"),
        Color::from_hex("#000000"),
        Color::from_hex("#AbC123"),
        Color::from_hex("#123abc"),
        Color::from_hex("#abc123"),
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn raw_hex_colors_8_digit_rgba() {
    let result = container! {
        div color=#445566ff background=#12345678 {
            "8-digit hex colors with alpha channel"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#445566ff")));
    assert_eq!(container.background, Some(Color::from_hex("#12345678")));
}

#[test]
fn raw_hex_colors_8_digit_mixed() {
    let result = container! {
        div color=#abcdef80 background=#ff00ff33 {
            "8-digit hex colors with various alpha values"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#abcdef80")));
    assert_eq!(container.background, Some(Color::from_hex("#ff00ff33")));
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

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 4);

    let expected_colors = [
        Color::from_hex("#000000ff"),
        Color::from_hex("#ffffff00"),
        Color::from_hex("#ff000080"),
        Color::from_hex("#AbCdEf12"),
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn rgb_function_basic() {
    let result = container! {
        div color=rgb(255, 0, 0) background=rgb(0, 255, 0) {
            "RGB color functions"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(
        container.color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: None
        })
    );
    assert_eq!(
        container.background,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: None
        })
    );
}

#[test]
fn rgb_function_with_alpha() {
    let result = container! {
        div color=rgb(255, 0, 0, 0.5) background=rgb(0, 255, 0, 128) {
            "RGB color functions with alpha"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    // 0.5 alpha should convert to 128 (0.5 * 255)
    assert_eq!(
        container.color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        })
    );
    // 128 integer alpha should stay as 128
    assert_eq!(
        container.background,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(128)
        })
    );
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

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(
        container.color,
        Some(Color {
            r: 255,
            g: 100,
            b: 50,
            a: None
        })
    );
}

#[test]
fn rgb_function_with_alpha_variables() {
    let red = 255;
    let green = 100;
    let blue = 50;
    let alpha = 0.75;
    let result = container! {
        div color=rgb(red, green, blue, alpha) {
            "RGB with alpha variables"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    // 0.75 alpha should convert to 191 (0.75 * 255)
    assert_eq!(
        container.color,
        Some(Color {
            r: 255,
            g: 100,
            b: 50,
            a: Some(191)
        })
    );
}

#[test]
fn rgb_function_mixed_usage() {
    let result = container! {
        div {
            // RGB function (3 arguments)
            div color=rgb(255, 0, 0) { "Pure red" }

            // RGB with float alpha (4 arguments)
            div color=rgb(0, 255, 0, 0.5) { "Semi-transparent green" }

            // RGB with integer alpha (4 arguments)
            div color=rgb(0, 0, 255, 128) { "Semi-transparent blue" }

            // Mixed with hex colors
            div color=rgb(128, 128, 128) background=#ffffff { "Gray on white" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 4);

    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: None
        })
    );
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(128)
        })
    );
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(128)
        })
    );
    assert_eq!(
        parent.children[3].color,
        Some(Color {
            r: 128,
            g: 128,
            b: 128,
            a: None
        })
    );
    assert_eq!(
        parent.children[3].background,
        Some(Color::from_hex("#ffffff"))
    );
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
                background=rgb(255, 255, 255, alpha_val) {
                "All color formats combined"
            }

            // Edge cases
            div color=rgb(0, 0, 0) background=rgb(255, 255, 255, 1.0) {
                "Black text on white background"
            }

            div color=rgb(255, 255, 255) background=rgb(0, 0, 0, 0.9) {
                "White text on mostly opaque black"
            }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 3);

    // First child - variables
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 200,
            g: 100,
            b: 50,
            a: None
        })
    );
    assert_eq!(
        parent.children[0].background,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: Some(204)
        })
    ); // 0.8 * 255 = 204

    // Second child - edge case 1
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: None
        })
    );
    assert_eq!(
        parent.children[1].background,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: Some(255)
        })
    ); // 1.0 * 255 = 255

    // Third child - edge case 2
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: None
        })
    );
    assert_eq!(
        parent.children[2].background,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: Some(230)
        })
    ); // 0.9 * 255 = 229.5 -> 230
}

#[test]
fn rgb_function_css_spec_compatibility() {
    // Test that rgb() function works like CSS - supports both 3 and 4 arguments
    let result = container! {
        div {
            // 3-argument RGB (standard RGB)
            div color=rgb(255, 0, 0) { "Pure red (3 args)" }
            div color=rgb(0, 255, 0) { "Pure green (3 args)" }
            div color=rgb(0, 0, 255) { "Pure blue (3 args)" }

            // 4-argument RGB (RGBA functionality)
            div color=rgb(255, 0, 0, 0.5) { "Semi-transparent red (4 args)" }
            div color=rgb(0, 255, 0, 0.75) { "Semi-transparent green (4 args)" }
            div color=rgb(0, 0, 255, 128) { "Semi-transparent blue (4 args)" }

            // Mixed usage in same container
            div background=rgb(255, 255, 255) color=rgb(0, 0, 0, 0.8) {
                "White background with semi-transparent black text"
            }

            // Edge cases
            div color=rgb(0, 0, 0) { "Black (3 args)" }
            div color=rgb(255, 255, 255, 1.0) { "White with full opacity (4 args)" }
            div color=rgb(128, 128, 128, 0.0) { "Gray with full transparency (4 args)" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 10);

    // 3-argument RGB tests
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: None
        })
    );
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: None
        })
    );
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: None
        })
    );

    // 4-argument RGB tests
    assert_eq!(
        parent.children[3].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        })
    ); // 0.5 * 255 = 127.5 -> 128
    assert_eq!(
        parent.children[4].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(191)
        })
    ); // 0.75 * 255 = 191.25 -> 191
    assert_eq!(
        parent.children[5].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(128)
        })
    ); // integer 128

    // Mixed usage
    assert_eq!(
        parent.children[6].background,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: None
        })
    );
    assert_eq!(
        parent.children[6].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: Some(204)
        })
    ); // 0.8 * 255 = 204

    // Edge cases
    assert_eq!(
        parent.children[7].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: None
        })
    );
    assert_eq!(
        parent.children[8].color,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: Some(255)
        })
    ); // 1.0 * 255 = 255
    assert_eq!(
        parent.children[9].color,
        Some(Color {
            r: 128,
            g: 128,
            b: 128,
            a: Some(0)
        })
    ); // 0.0 * 255 = 0
}

#[test]
fn rgb_function_percentage_alpha() {
    let result = container! {
        div {
            // Test various percentage values
            div color=rgb(255, 0, 0, "0%") { "0% alpha" }
            div color=rgb(0, 255, 0, "25%") { "25% alpha" }
            div color=rgb(0, 0, 255, "50%") { "50% alpha" }
            div color=rgb(255, 255, 0, "75%") { "75% alpha" }
            div color=rgb(255, 0, 255, "100%") { "100% alpha" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 5);

    // Test percentage alpha conversions
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(0)
        })
    ); // 0% = 0
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(64)
        })
    ); // 25% = 63.75 -> 64
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(128)
        })
    ); // 50% = 127.5 -> 128
    assert_eq!(
        parent.children[3].color,
        Some(Color {
            r: 255,
            g: 255,
            b: 0,
            a: Some(191)
        })
    ); // 75% = 191.25 -> 191
    assert_eq!(
        parent.children[4].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 255,
            a: Some(255)
        })
    ); // 100% = 255
}

#[test]
fn rgb_function_percentage_alpha_edge_cases() {
    let result = container! {
        div {
            // Test edge cases and decimal percentages
            div color=rgb(255, 0, 0, "0.5%") { "0.5% alpha" }
            div color=rgb(0, 255, 0, "33.33%") { "33.33% alpha" }
            div color=rgb(0, 0, 255, "66.67%") { "66.67% alpha" }
            div color=rgb(128, 128, 128, "99.9%") { "99.9% alpha" }

            // Test out-of-range values (should be clamped)
            div color=rgb(255, 255, 255, "-10%") { "Negative percentage" }
            div color=rgb(0, 0, 0, "150%") { "Over 100%" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 6);

    // Test decimal percentage conversions
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(1)
        })
    ); // 0.5% = 1.275 -> 1
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(85)
        })
    ); // 33.33% = 84.9915 -> 85
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(170)
        })
    ); // 66.67% = 170.0085 -> 170
    assert_eq!(
        parent.children[3].color,
        Some(Color {
            r: 128,
            g: 128,
            b: 128,
            a: Some(255)
        })
    ); // 99.9% = 254.745 -> 255

    // Test clamping
    assert_eq!(
        parent.children[4].color,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: Some(0)
        })
    ); // -10% clamped to 0%
    assert_eq!(
        parent.children[5].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: Some(255)
        })
    ); // 150% clamped to 100%
}

#[test]
fn rgb_function_mixed_alpha_types() {
    let result = container! {
        div {
            // Mix percentage, float, and integer alpha values
            div color=rgb(255, 0, 0, "50%") background=rgb(0, 0, 0, 0.5) { "Percentage vs Float" }
            div color=rgb(0, 255, 0, "25%") background=rgb(0, 0, 0, 64) { "Percentage vs Integer" }
            div color=rgb(0, 0, 255, 0.75) background=rgb(255, 255, 255, "75%") { "Float vs Percentage" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 3);

    // Test that different alpha formats produce equivalent results
    // 50% should equal 0.5 float
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        })
    );
    assert_eq!(
        parent.children[0].background,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: Some(128)
        })
    );

    // 25% should equal 64 integer
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(64)
        })
    );
    assert_eq!(
        parent.children[1].background,
        Some(Color {
            r: 0,
            g: 0,
            b: 0,
            a: Some(64)
        })
    );

    // 0.75 float should equal 75%
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(191)
        })
    );
    assert_eq!(
        parent.children[2].background,
        Some(Color {
            r: 255,
            g: 255,
            b: 255,
            a: Some(191)
        })
    );
}

#[test]
fn rgb_function_percentage_alpha_with_variables() {
    let alpha_percent = "60%";
    let result = container! {
        div color=rgb(200, 100, 50, alpha_percent) {
            "RGB with percentage alpha variable"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    // 60% = 0.6 * 255 = 153
    assert_eq!(
        container.color,
        Some(Color {
            r: 200,
            g: 100,
            b: 50,
            a: Some(153)
        })
    );
}

#[test]
fn simple_percentage_alpha_test() {
    let result = container! {
        div color=rgb(255, 0, 0, 50%) { "Test" }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    // 50% should convert to 127 or 128 (50% of 255)
    assert_eq!(
        container.color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        })
    );
}

#[test]
fn raw_percentage_alpha_comprehensive() {
    let result = container! {
        div {
            // Test various raw percentage values
            div color=rgb(255, 0, 0, 0%) { "0% alpha" }
            div color=rgb(0, 255, 0, 25%) { "25% alpha" }
            div color=rgb(0, 0, 255, 50%) { "50% alpha" }
            div color=rgb(255, 255, 0, 75%) { "75% alpha" }
            div color=rgb(255, 0, 255, 100%) { "100% alpha" }

            // Test decimal percentages
            div color=rgb(128, 128, 128, 12.5%) { "12.5% alpha" }
            div color=rgb(64, 64, 64, 87.5%) { "87.5% alpha" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 7);

    // Test percentage alpha conversions (percentage * 255 / 100)
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(0)
        })
    ); // 0%
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(64)
        })
    ); // 25% = 63.75 -> 64
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(128)
        })
    ); // 50% = 127.5 -> 128
    assert_eq!(
        parent.children[3].color,
        Some(Color {
            r: 255,
            g: 255,
            b: 0,
            a: Some(191)
        })
    ); // 75% = 191.25 -> 191
    assert_eq!(
        parent.children[4].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 255,
            a: Some(255)
        })
    ); // 100%

    // Test decimal percentages
    assert_eq!(
        parent.children[5].color,
        Some(Color {
            r: 128,
            g: 128,
            b: 128,
            a: Some(32)
        })
    ); // 12.5% = 31.875 -> 32
    assert_eq!(
        parent.children[6].color,
        Some(Color {
            r: 64,
            g: 64,
            b: 64,
            a: Some(223)
        })
    ); // 87.5% = 223.125 -> 223
}

#[test]
fn raw_percentage_mixed_with_other_types() {
    let result = container! {
        div {
            // Mix raw percentages with other alpha types
            div color=rgb(255, 0, 0, 50%) background=rgb(0, 255, 0, 0.5) { "Percentage vs float" }
            div color=rgb(0, 0, 255, 75%) background=rgb(255, 255, 0, 128) { "Percentage vs integer" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 2);

    // First child: 50% vs 0.5 (should be equivalent)
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(128)
        })
    ); // 50%
    assert_eq!(
        parent.children[0].background,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(128)
        })
    ); // 0.5

    // Second child: 75% vs 128
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(191)
        })
    ); // 75%
    assert_eq!(
        parent.children[1].background,
        Some(Color {
            r: 255,
            g: 255,
            b: 0,
            a: Some(128)
        })
    ); // 128
}

#[test]
fn raw_percentage_edge_cases() {
    let result = container! {
        div {
            // Test edge cases
            div color=rgb(255, 0, 0, 0.1%) { "Very small percentage" }
            div color=rgb(0, 255, 0, 99.9%) { "Very large percentage" }
            div color=rgb(0, 0, 255, 150%) { "Over 100% (should clamp)" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 3);

    // Test edge cases
    assert_eq!(
        parent.children[0].color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(0)
        })
    ); // 0.1% = 0.255 -> 0
    assert_eq!(
        parent.children[1].color,
        Some(Color {
            r: 0,
            g: 255,
            b: 0,
            a: Some(255)
        })
    ); // 99.9% = 254.745 -> 255
    assert_eq!(
        parent.children[2].color,
        Some(Color {
            r: 0,
            g: 0,
            b: 255,
            a: Some(255)
        })
    ); // 150% clamped to 100% = 255
}

#[test]
fn hex_colors_starting_with_digit_containing_letters() {
    let result = container! {
        div {
            div color=#1a2b3c { "Starts with 1, contains a,b,c" }
            div color=#2fade3 { "Starts with 2, contains f,a,d,e" }
            div color=#3ff { "Starts with 3, contains f" }
            div color=#4d4d4d { "Starts with 4, contains d" }
            div color=#5abcde { "Starts with 5, contains a,b,c,d,e" }
            div color=#6f6f6f { "Starts with 6, contains f" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 6);

    let expected_colors = [
        Color::from_hex("#1a2b3c"),
        Color::from_hex("#2fade3"),
        Color::from_hex("#3ff"),
        Color::from_hex("#4d4d4d"),
        Color::from_hex("#5abcde"),
        Color::from_hex("#6f6f6f"),
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn hex_colors_8_digit_starting_with_digit_containing_letters() {
    let result = container! {
        div color=#1a2b3c4d background=#5f6a7b8c {
            "8-digit hex colors starting with digit and containing letters"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#1a2b3c4d")));
    assert_eq!(container.background, Some(Color::from_hex("#5f6a7b8c")));
}

#[test]
fn hex_colors_scientific_notation_3_digit() {
    let result = container! {
        div {
            div color=#1e2 { "1e2 - tokenizes as LitFloat(1e2)" }
            div color=#3e8 { "3e8 - tokenizes as LitFloat(3e8)" }
            div color=#9e1 { "9e1 - tokenizes as LitFloat(9e1)" }
            div color=#5e0 { "5e0 - tokenizes as LitFloat(5e0)" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 4);

    let expected_colors = [
        Color::from_hex("#1e2"),
        Color::from_hex("#3e8"),
        Color::from_hex("#9e1"),
        Color::from_hex("#5e0"),
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn hex_colors_scientific_notation_6_digit() {
    let result = container! {
        div {
            div color=#1e293b { "1e293b - tokenizes as LitFloat(1e293) + Ident(b)" }
            div color=#3e8acc { "3e8acc - tokenizes as LitFloat(3e8) + Ident(acc)" }
            div color=#2e5a3d { "2e5a3d - tokenizes as LitFloat(2e5) + Ident(a3d)" }
            div color=#7e1f8a { "7e1f8a - tokenizes as LitFloat(7e1) + Ident(f8a)" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 4);

    let expected_colors = [
        Color::from_hex("#1e293b"),
        Color::from_hex("#3e8acc"),
        Color::from_hex("#2e5a3d"),
        Color::from_hex("#7e1f8a"),
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn hex_colors_scientific_notation_8_digit() {
    let result = container! {
        div {
            div color=#1e293bff { "1e293bff with alpha" }
            div color=#3e8accaa { "3e8accaa with alpha" }
            div color=#2e5a3d80 { "2e5a3d80 with alpha" }
        }
    };

    assert_eq!(result.len(), 1);
    let parent = &result[0];
    assert_eq!(parent.children.len(), 3);

    let expected_colors = [
        Color::from_hex("#1e293bff"),
        Color::from_hex("#3e8accaa"),
        Color::from_hex("#2e5a3d80"),
    ];

    for (i, expected_color) in expected_colors.iter().enumerate() {
        assert_eq!(parent.children[i].color, Some(*expected_color));
    }
}

#[test]
fn hex_colors_scientific_notation_mixed() {
    let result = container! {
        div color=#1e2 background=#3e8acc {
            "Mix 3-digit and 6-digit scientific notation patterns"
        }
    };

    assert_eq!(result.len(), 1);
    let container = &result[0];
    assert_eq!(container.color, Some(Color::from_hex("#1e2")));
    assert_eq!(container.background, Some(Color::from_hex("#3e8acc")));
}
