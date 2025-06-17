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

    let expected_colors = vec![
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

    let expected_colors = vec![
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

    let expected_colors = vec![
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
