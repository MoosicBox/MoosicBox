#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template::container;
use hyperchad_transformer::Element;

#[test_log::test]
fn test_data_attribute_simple() {
    let containers = container! {
        div data-id="item-123" {
            "Item content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert!(matches!(containers[0].element, Element::Div));

    // Check that the data attribute is present
    let data = &containers[0].data;
    assert_eq!(data.get("id"), Some(&"item-123".to_string()));
}

#[test_log::test]
fn test_data_attribute_multiple() {
    let containers = container! {
        div data-id="123" data-type="product" data-category="electronics" {
            "Product"
        }
    };

    assert_eq!(containers.len(), 1);

    let data = &containers[0].data;
    assert_eq!(data.get("id"), Some(&"123".to_string()));
    assert_eq!(data.get("type"), Some(&"product".to_string()));
    assert_eq!(data.get("category"), Some(&"electronics".to_string()));
}

#[test_log::test]
fn test_data_attribute_hyphenated_name() {
    let containers = container! {
        div data-user-name="John" data-last-updated="2024-01-01" {
            "User info"
        }
    };

    assert_eq!(containers.len(), 1);

    let data = &containers[0].data;
    assert_eq!(data.get("user-name"), Some(&"John".to_string()));
    assert_eq!(data.get("last-updated"), Some(&"2024-01-01".to_string()));
}

#[test_log::test]
fn test_data_attribute_dynamic_value() {
    let user_id = "user-456";

    let containers = container! {
        div data-user-id=(user_id) {
            "Dynamic data"
        }
    };

    assert_eq!(containers.len(), 1);

    let data = &containers[0].data;
    assert_eq!(data.get("user-id"), Some(&"user-456".to_string()));
}

#[test_log::test]
fn test_data_attribute_numeric_value() {
    let containers = container! {
        div data-count="42" data-price="19.99" {
            "Numbers as strings"
        }
    };

    assert_eq!(containers.len(), 1);

    let data = &containers[0].data;
    assert_eq!(data.get("count"), Some(&"42".to_string()));
    assert_eq!(data.get("price"), Some(&"19.99".to_string()));
}

#[test_log::test]
fn test_data_attribute_empty_value() {
    let containers = container! {
        div data-flag {
            "Empty data attribute"
        }
    };

    assert_eq!(containers.len(), 1);

    let data = &containers[0].data;
    assert_eq!(data.get("flag"), Some(&String::new()));
}

#[test_log::test]
fn test_data_attribute_with_other_attributes() {
    let containers = container! {
        div
            class="item-card"
            data-item-id="abc123"
            padding=10
            background="white"
        {
            "Mixed attributes"
        }
    };

    assert_eq!(containers.len(), 1);

    // Check data attribute
    let data = &containers[0].data;
    assert_eq!(data.get("item-id"), Some(&"abc123".to_string()));

    // Check other attributes still work
    assert!(containers[0].padding_top.is_some());
    assert!(containers[0].background.is_some());
}

#[test_log::test]
fn test_data_attribute_on_different_elements() {
    let containers = container! {
        span data-tooltip="Hover for info" {
            "Tooltip trigger"
        }
        button data-action="submit" {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 2);

    // Check span's data attribute
    assert!(matches!(containers[0].element, Element::Span));
    assert_eq!(
        containers[0].data.get("tooltip"),
        Some(&"Hover for info".to_string())
    );

    // Check button's data attribute
    assert!(matches!(containers[1].element, Element::Button { .. }));
    assert_eq!(
        containers[1].data.get("action"),
        Some(&"submit".to_string())
    );
}
