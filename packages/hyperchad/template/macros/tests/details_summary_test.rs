#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template::container;
use hyperchad_transformer::Element;

#[test_log::test]
fn test_details_with_summary() {
    let containers = container! {
        details {
            summary { "Click to expand" }
            div { "Hidden content" }
        }
    };

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::Details { .. }));

    // Check children
    assert_eq!(containers[0].children.len(), 2);

    // First child should be summary
    assert!(matches!(
        containers[0].children[0].element,
        Element::Summary
    ));

    // Second child should be div
    assert!(matches!(containers[0].children[1].element, Element::Div));
}

#[test_log::test]
fn test_details_open() {
    let containers = container! {
        details open {
            summary { "Already expanded" }
            div { "Visible content" }
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Details { open } = &containers[0].element {
        assert_eq!(*open, Some(true));
    } else {
        panic!("Expected Details element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_details_closed() {
    let containers = container! {
        details {
            summary { "Collapsed" }
            div { "Hidden" }
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Details { open } = &containers[0].element {
        // Should be None (default closed state)
        assert_eq!(*open, None);
    } else {
        panic!("Expected Details element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_details_dynamic_open() {
    let is_expanded = true;

    let containers = container! {
        details open=(is_expanded) {
            summary { "Dynamic state" }
            div { "Content" }
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Details { open } = &containers[0].element {
        assert_eq!(*open, Some(true));
    } else {
        panic!("Expected Details element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_summary_with_multiple_children() {
    let containers = container! {
        details {
            summary {
                span { "Title: " }
                "More info"
            }
            div { "Detailed content" }
        }
    };

    assert_eq!(containers.len(), 1);

    // Check summary has multiple children
    let summary = &containers[0].children[0];
    assert!(matches!(summary.element, Element::Summary));
    assert_eq!(summary.children.len(), 2);
}

#[test_log::test]
fn test_nested_details() {
    let containers = container! {
        details {
            summary { "Outer" }
            div {
                details {
                    summary { "Inner" }
                    div { "Nested content" }
                }
            }
        }
    };

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::Details { .. }));

    // Find the nested details within the div
    let outer_div = &containers[0].children[1];
    assert!(matches!(outer_div.element, Element::Div));

    let inner_details = &outer_div.children[0];
    assert!(matches!(inner_details.element, Element::Details { .. }));
}
