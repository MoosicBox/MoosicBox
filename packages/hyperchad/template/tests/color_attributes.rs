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
