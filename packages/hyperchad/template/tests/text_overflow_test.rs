use hyperchad_template::container;
use hyperchad_transformer_models::TextOverflow;

#[test]
fn test_text_overflow_with_quoted_string_literals() {
    let containers = container! {
        div text-overflow="clip" { "Clipped text" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Clip));

    let containers = container! {
        div text-overflow="ellipsis" { "Ellipsis text" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn test_text_overflow_with_unquoted_identifiers() {
    let containers = container! {
        div text-overflow=clip { "Clipped text" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Clip));

    let containers = container! {
        div text-overflow=ellipsis { "Ellipsis text" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn test_text_overflow_with_expressions() {
    let containers = container! {
        div text-overflow=(TextOverflow::Clip) { "Clip with expression" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Clip));

    let containers = container! {
        div text-overflow=(TextOverflow::Ellipsis) { "Ellipsis with expression" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn test_text_overflow_with_function_calls() {
    fn get_text_overflow() -> TextOverflow {
        TextOverflow::Ellipsis
    }

    let containers = container! {
        div text-overflow=(get_text_overflow()) { "Function-determined text-overflow" }
    };
    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn test_text_overflow_html_output() {
    let containers = container! {
        div text-overflow=clip { "Clip" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("text-overflow"));
    assert!(html.contains("clip"));

    let containers = container! {
        div text-overflow=ellipsis { "Ellipsis" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("text-overflow"));
    assert!(html.contains("ellipsis"));
}

#[test]
fn test_text_overflow_combined_with_other_attributes() {
    let containers = container! {
        div
            text-overflow=ellipsis
            overflow-wrap="break-word"
            padding=20
            background="blue"
        {
            "Combined attributes"
        }
    };

    let container = &containers[0];
    assert_eq!(container.text_overflow, Some(TextOverflow::Ellipsis));
    assert!(container.overflow_wrap.is_some());
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
}

#[test]
fn test_text_overflow_default_is_none() {
    let containers = container! {
        div { "No text-overflow specified" }
    };
    assert_eq!(containers[0].text_overflow, None);
}

#[test]
fn test_text_overflow_nested_elements() {
    let containers = container! {
        div text-overflow=clip {
            div text-overflow=ellipsis {
                "Inner ellipsis"
            }
            "Outer clip"
        }
    };

    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Clip));
    assert_eq!(
        containers[0].children[0].text_overflow,
        Some(TextOverflow::Ellipsis)
    );
}

#[test]
fn test_text_overflow_with_conditional() {
    let use_ellipsis = true;

    let containers = container! {
        div text-overflow=(if use_ellipsis { TextOverflow::Ellipsis } else { TextOverflow::Clip }) {
            "Conditional text-overflow"
        }
    };

    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Ellipsis));
}

#[test]
fn test_text_overflow_with_width_constraint() {
    let containers = container! {
        div
            text-overflow=ellipsis
            width=200
        {
            "This is a very long text that should be truncated with ellipsis when it exceeds the width"
        }
    };

    assert_eq!(containers[0].text_overflow, Some(TextOverflow::Ellipsis));
    assert!(containers[0].width.is_some());
}
