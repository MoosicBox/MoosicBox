use hyperchad_template::container;
use hyperchad_transformer_models::WhiteSpace;

#[test]
fn test_white_space_with_quoted_string_literals() {
    let containers = container! {
        div white-space="normal" { "Normal text" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Normal));

    let containers = container! {
        div white-space="preserve" { "Preserved text" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Preserve));

    let containers = container! {
        div white-space="preserve-wrap" { "Preserve-wrap text" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::PreserveWrap));
}

#[test]
fn test_white_space_with_unquoted_identifiers() {
    let containers = container! {
        div white-space=normal { "Normal text" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Normal));

    let containers = container! {
        div white-space=preserve { "Preserved text" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Preserve));

    let containers = container! {
        div white-space=preserve-wrap { "Preserve-wrap text" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::PreserveWrap));
}

#[test]
fn test_white_space_with_expressions() {
    let containers = container! {
        div white-space=(WhiteSpace::Normal) { "Normal with expression" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Normal));

    let containers = container! {
        div white-space=(WhiteSpace::Preserve) { "Preserve with expression" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Preserve));

    let containers = container! {
        div white-space=(WhiteSpace::PreserveWrap) { "PreserveWrap with expression" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::PreserveWrap));
}

#[test]
fn test_white_space_with_function_calls() {
    fn get_white_space() -> WhiteSpace {
        WhiteSpace::Preserve
    }

    let containers = container! {
        div white-space=(get_white_space()) { "Function-determined white-space" }
    };
    assert_eq!(containers[0].white_space, Some(WhiteSpace::Preserve));
}

#[test]
fn test_white_space_html_output() {
    let containers = container! {
        div white-space=normal { "Normal" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("white-space"));
    assert!(html.contains("normal"));

    let containers = container! {
        div white-space=preserve { "Preserved" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("white-space"));
    assert!(html.contains("pre"));

    let containers = container! {
        div white-space=preserve-wrap { "PreserveWrap" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("white-space"));
    assert!(html.contains("preserve-wrap"));
}

#[test]
fn test_white_space_combined_with_other_attributes() {
    let containers = container! {
        div
            white-space=preserve
            text-align="center"
            padding=20
            background="blue"
        {
            "Combined attributes"
        }
    };

    let container = &containers[0];
    assert_eq!(container.white_space, Some(WhiteSpace::Preserve));
    assert!(container.text_align.is_some());
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
}

#[test]
fn test_white_space_default_is_none() {
    let containers = container! {
        div { "No white-space specified" }
    };
    assert_eq!(containers[0].white_space, None);
}

#[test]
fn test_white_space_nested_elements() {
    let containers = container! {
        div white-space=normal {
            div white-space=preserve {
                "Inner preserved"
            }
            "Outer normal"
        }
    };

    assert_eq!(containers[0].white_space, Some(WhiteSpace::Normal));
    assert_eq!(
        containers[0].children[0].white_space,
        Some(WhiteSpace::Preserve)
    );
}

#[test]
fn test_white_space_with_conditional() {
    let preserve_whitespace = true;

    let containers = container! {
        div white-space=(if preserve_whitespace { WhiteSpace::Preserve } else { WhiteSpace::Normal }) {
            "Conditional white-space"
        }
    };

    assert_eq!(containers[0].white_space, Some(WhiteSpace::Preserve));
}
