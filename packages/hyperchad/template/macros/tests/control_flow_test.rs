#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template::container;
use hyperchad_transformer::Element;

// ============================================================================
// @if / @else / @else if Tests
// ============================================================================

#[test_log::test]
fn test_if_true_branch() {
    let show_content = true;

    let containers = container! {
        @if show_content {
            div { "Visible" }
        }
    };

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::Div));
    assert_eq!(containers[0].children.len(), 1);

    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Visible");
}

#[test_log::test]
fn test_if_false_branch() {
    let show_content = false;

    let containers = container! {
        @if show_content {
            div { "Visible" }
        }
    };

    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn test_if_else() {
    let use_primary = false;

    let containers = container! {
        @if use_primary {
            div { "Primary" }
        } @else {
            div { "Secondary" }
        }
    };

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::Div));

    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Secondary");
}

#[test_log::test]
fn test_if_else_if_else_chain() {
    let mode = 2;

    let containers = container! {
        @if mode == 1 {
            div { "Mode 1" }
        } @else if mode == 2 {
            div { "Mode 2" }
        } @else {
            div { "Other" }
        }
    };

    assert_eq!(containers.len(), 1);
    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Mode 2");
}

#[test_log::test]
fn test_if_let_some() {
    let maybe_name: Option<&str> = Some("Alice");

    let containers = container! {
        @if let Some(name) = maybe_name {
            div { "Hello, " (name) }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 2);

    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Hello, ");

    let Element::Raw { value } = &containers[0].children[1].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Alice");
}

#[test_log::test]
fn test_if_let_none() {
    let maybe_name: Option<&str> = None;

    let containers = container! {
        @if let Some(name) = maybe_name {
            div { "Hello, " (name) }
        }
    };

    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn test_if_let_with_else() {
    let maybe_value: Option<i32> = None;

    let containers = container! {
        @if let Some(val) = maybe_value {
            div { "Value: " (val) }
        } @else {
            div { "No value" }
        }
    };

    assert_eq!(containers.len(), 1);
    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "No value");
}

// ============================================================================
// @for Loop Tests
// ============================================================================

#[test_log::test]
fn test_for_loop_basic() {
    let items = vec!["Apple", "Banana", "Cherry"];

    let containers = container! {
        @for item in &items {
            div { (item) }
        }
    };

    assert_eq!(containers.len(), 3);

    for (i, container) in containers.iter().enumerate() {
        assert!(matches!(container.element, Element::Div));
        let Element::Raw { value } = &container.children[0].element else {
            panic!("Expected Raw element");
        };
        assert_eq!(value, items[i]);
    }
}

#[test_log::test]
fn test_for_loop_with_index() {
    let items = ["A", "B", "C"];

    let containers = container! {
        @for (idx, item) in items.iter().enumerate() {
            div { (idx) ": " (item) }
        }
    };

    assert_eq!(containers.len(), 3);

    // Check first item
    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "0");

    let Element::Raw { value } = &containers[0].children[1].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, ": ");

    let Element::Raw { value } = &containers[0].children[2].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "A");
}

#[test_log::test]
fn test_for_loop_empty() {
    let items: Vec<&str> = vec![];

    let containers = container! {
        @for item in items {
            div { (item) }
        }
    };

    assert_eq!(containers.len(), 0);
}

#[test_log::test]
fn test_for_loop_range() {
    let containers = container! {
        @for i in 0..3 {
            span { (i) }
        }
    };

    assert_eq!(containers.len(), 3);

    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "0");

    let Element::Raw { value } = &containers[1].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "1");

    let Element::Raw { value } = &containers[2].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "2");
}

// ============================================================================
// @let Binding Tests
// ============================================================================

#[test_log::test]
fn test_let_binding_basic() {
    let containers = container! {
        {
            @let x = 42;
            div { (x) }
        }
    };

    assert_eq!(containers.len(), 1);
    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "42");
}

#[test_log::test]
fn test_let_binding_computed() {
    let a = 10;
    let b = 20;

    let containers = container! {
        {
            @let sum = a + b;
            div { "Sum: " (sum) }
        }
    };

    assert_eq!(containers.len(), 1);
    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Sum: ");

    let Element::Raw { value } = &containers[0].children[1].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "30");
}

#[test_log::test]
fn test_let_binding_multiple() {
    let containers = container! {
        {
            @let first = "Hello";
            @let second = "World";
            div { (first) " " (second) }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 3);

    let Element::Raw { value } = &containers[0].children[0].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "Hello");

    let Element::Raw { value } = &containers[0].children[1].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, " ");

    let Element::Raw { value } = &containers[0].children[2].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "World");
}

// ============================================================================
// Nested Control Flow Tests
// ============================================================================

#[test_log::test]
fn test_nested_if_in_for() {
    let items = vec![1, 2, 3, 4, 5];

    let containers = container! {
        @for item in &items {
            @if *item % 2 == 0 {
                div { "Even: " (item) }
            }
        }
    };

    // Only even numbers should produce containers (2 and 4)
    assert_eq!(containers.len(), 2);

    let Element::Raw { value } = &containers[0].children[1].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "2");

    let Element::Raw { value } = &containers[1].children[1].element else {
        panic!("Expected Raw element");
    };
    assert_eq!(value, "4");
}

#[test_log::test]
fn test_for_in_if() {
    let show_list = true;
    let items = vec!["X", "Y", "Z"];

    let containers = container! {
        @if show_list {
            ul {
                @for item in &items {
                    li { (item) }
                }
            }
        }
    };

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::UnorderedList));
    assert_eq!(containers[0].children.len(), 3);
}
