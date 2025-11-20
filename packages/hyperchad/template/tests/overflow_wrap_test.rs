use hyperchad_template::container;
use hyperchad_transformer_models::OverflowWrap;

#[test]
fn test_overflow_wrap_with_quoted_string_literals() {
    let containers = container! {
        div overflow-wrap="normal" { "Normal wrapping" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Normal));

    let containers = container! {
        div overflow-wrap="break-word" { "Break word" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::BreakWord));

    let containers = container! {
        div overflow-wrap="anywhere" { "Break anywhere" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Anywhere));
}

#[test]
fn test_overflow_wrap_with_unquoted_identifiers() {
    let containers = container! {
        div overflow-wrap=normal { "Normal wrapping" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Normal));

    let containers = container! {
        div overflow-wrap=break-word { "Break word" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::BreakWord));

    let containers = container! {
        div overflow-wrap=anywhere { "Break anywhere" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Anywhere));
}

#[test]
fn test_overflow_wrap_with_expressions() {
    let containers = container! {
        div overflow-wrap=(OverflowWrap::Normal) { "Normal with expression" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Normal));

    let containers = container! {
        div overflow-wrap=(OverflowWrap::BreakWord) { "BreakWord with expression" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::BreakWord));

    let containers = container! {
        div overflow-wrap=(OverflowWrap::Anywhere) { "Anywhere with expression" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Anywhere));
}

#[test]
fn test_overflow_wrap_with_function_calls() {
    fn get_overflow_wrap() -> OverflowWrap {
        OverflowWrap::BreakWord
    }

    let containers = container! {
        div overflow-wrap=(get_overflow_wrap()) { "Function-determined overflow-wrap" }
    };
    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::BreakWord));
}

#[test]
fn test_overflow_wrap_html_output() {
    let containers = container! {
        div overflow-wrap=normal { "Normal" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("overflow-wrap"));
    assert!(html.contains("normal"));

    let containers = container! {
        div overflow-wrap=break-word { "BreakWord" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("overflow-wrap"));
    assert!(html.contains("break-word"));

    let containers = container! {
        div overflow-wrap=anywhere { "Anywhere" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("overflow-wrap"));
    assert!(html.contains("anywhere"));
}

#[test]
fn test_overflow_wrap_combined_with_other_attributes() {
    let containers = container! {
        div
            overflow-wrap=break-word
            white-space="preserve-wrap"
            padding=20
            background="blue"
        {
            "Combined attributes"
        }
    };

    let container = &containers[0];
    assert_eq!(container.overflow_wrap, Some(OverflowWrap::BreakWord));
    assert!(container.white_space.is_some());
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
}

#[test]
fn test_overflow_wrap_default_is_none() {
    let containers = container! {
        div { "No overflow-wrap specified" }
    };
    assert_eq!(containers[0].overflow_wrap, None);
}

#[test]
fn test_overflow_wrap_nested_elements() {
    let containers = container! {
        div overflow-wrap=normal {
            div overflow-wrap=break-word {
                "Inner break-word"
            }
            "Outer normal"
        }
    };

    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::Normal));
    assert_eq!(
        containers[0].children[0].overflow_wrap,
        Some(OverflowWrap::BreakWord)
    );
}

#[test]
fn test_overflow_wrap_with_conditional() {
    let allow_breaking = true;

    let containers = container! {
        div overflow-wrap=(if allow_breaking { OverflowWrap::BreakWord } else { OverflowWrap::Normal }) {
            "Conditional overflow-wrap"
        }
    };

    assert_eq!(containers[0].overflow_wrap, Some(OverflowWrap::BreakWord));
}
