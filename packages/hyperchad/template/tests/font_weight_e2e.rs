use hyperchad_template::container;
use hyperchad_transformer_models::FontWeight;

// Helper function to strip ANSI color codes from HTML output
fn strip_ansi_codes(input: &str) -> String {
    // Simple regex-free approach to strip ANSI escape sequences
    let mut result = String::new();
    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip the escape sequence
            if chars.next() == Some('[') {
                // Skip until we find a letter (end of ANSI sequence)
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[test]
fn test_font_weight_end_to_end_template_to_html() {
    // Test that font-weight works from template macro → Container → HTML output
    let containers = container! {
        div font-weight=bold {
            "Bold text"
        }
        div font-weight=700 {
            "Numeric weight"
        }
        div font-weight=semi-bold {
            "Semi-bold text"
        }
        div font-weight=normal {
            "Normal text"
        }
        div font-weight=lighter {
            "Lighter text"
        }
    };

    // Verify we have 5 containers
    assert_eq!(containers.len(), 5);

    // Verify each container has the correct font-weight
    assert_eq!(containers[0].font_weight, Some(FontWeight::Bold));
    assert_eq!(containers[1].font_weight, Some(FontWeight::Weight700));
    assert_eq!(containers[2].font_weight, Some(FontWeight::SemiBold));
    assert_eq!(containers[3].font_weight, Some(FontWeight::Normal));
    assert_eq!(containers[4].font_weight, Some(FontWeight::Lighter));

    // Test HTML generation
    let html_output = hyperchad_template::to_html(&containers);

    // Strip ANSI color codes for reliable string matching
    let clean_html = strip_ansi_codes(&html_output);

    // Verify that the HTML contains the correct sx-font-weight attributes
    assert!(clean_html.contains(r#"sx-font-weight="bold""#));
    assert!(clean_html.contains(r#"sx-font-weight="700""#));
    assert!(clean_html.contains(r#"sx-font-weight="semi-bold""#));
    assert!(clean_html.contains(r#"sx-font-weight="normal""#));
    assert!(clean_html.contains(r#"sx-font-weight="lighter""#));
}

#[test]
fn test_font_weight_with_other_attributes() {
    // Test font-weight combined with other styling attributes
    let containers = container! {
        div
            font-weight=bold
            font-size="16px"
            color="red"
            class="text-content"
        {
            "Styled text"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Verify font-weight is set correctly
    assert_eq!(container.font_weight, Some(FontWeight::Bold));

    // Verify other attributes are also set
    assert!(container.font_size.is_some());
    assert!(container.color.is_some());
    assert!(!container.classes.is_empty());

    // Test HTML generation includes all attributes
    let html_output = hyperchad_template::to_html(&containers);
    let clean_html = strip_ansi_codes(&html_output);
    assert!(clean_html.contains(r#"sx-font-weight="bold""#));
    assert!(clean_html.contains(r#"sx-font-size="16""#));
    assert!(clean_html.contains("sx-color=\"#FF0000\""));
    assert!(clean_html.contains(r#"class="text-content""#));
}

#[test]
fn test_font_weight_expression_syntax() {
    // Test using expressions with FontWeight enum values
    let containers = container! {
        div font-weight=(FontWeight::ExtraBold) {
            "Expression-based font weight"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight, Some(FontWeight::ExtraBold));

    // Test HTML generation
    let html_output = hyperchad_template::to_html(&containers);
    let clean_html = strip_ansi_codes(&html_output);
    assert!(clean_html.contains(r#"sx-font-weight="extra-bold""#));
}

#[test]
fn test_font_weight_all_variants_html_output() {
    // Test that all FontWeight variants generate correct HTML
    let containers = container! {
        div font-weight=thin { "Thin" }
        div font-weight=extra-light { "Extra Light" }
        div font-weight=light { "Light" }
        div font-weight=normal { "Normal" }
        div font-weight=medium { "Medium" }
        div font-weight=semi-bold { "Semi Bold" }
        div font-weight=bold { "Bold" }
        div font-weight=extra-bold { "Extra Bold" }
        div font-weight=black { "Black" }
        div font-weight=lighter { "Lighter" }
        div font-weight=bolder { "Bolder" }
        div font-weight=100 { "100" }
        div font-weight=200 { "200" }
        div font-weight=300 { "300" }
        div font-weight=400 { "400" }
        div font-weight=500 { "500" }
        div font-weight=600 { "600" }
        div font-weight=700 { "700" }
        div font-weight=800 { "800" }
        div font-weight=900 { "900" }
    };

    assert_eq!(containers.len(), 20);

    // Verify all containers have font-weight set
    for container in &containers {
        assert!(container.font_weight.is_some());
    }

    // Test HTML generation
    let html_output = hyperchad_template::to_html(&containers);
    let clean_html = strip_ansi_codes(&html_output);

    // Verify HTML contains all expected font-weight values
    let expected_values = [
        "thin",
        "extra-light",
        "light",
        "normal",
        "medium",
        "semi-bold",
        "bold",
        "extra-bold",
        "black",
        "lighter",
        "bolder",
        "100",
        "200",
        "300",
        "400",
        "500",
        "600",
        "700",
        "800",
        "900",
    ];

    for expected in expected_values {
        assert!(
            clean_html.contains(&format!(r#"sx-font-weight="{expected}""#)),
            "HTML output missing sx-font-weight=\"{expected}\""
        );
    }
}

#[test]
fn test_font_weight_conditional_rendering() {
    // Test font-weight with conditional rendering
    let use_bold = true;
    let containers = container! {
        div font-weight=(if use_bold { FontWeight::Bold } else { FontWeight::Normal }) {
            "Conditional font weight"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight, Some(FontWeight::Bold));

    let html_output = hyperchad_template::to_html(&containers);
    let clean_html = strip_ansi_codes(&html_output);
    assert!(clean_html.contains(r#"sx-font-weight="bold""#));
}
