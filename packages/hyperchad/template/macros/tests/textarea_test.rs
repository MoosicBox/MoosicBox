#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template_macros::container;
use hyperchad_transformer::{Element, Number};

#[test]
fn test_static_textarea() {
    let result = container! {
        textarea name="comment" placeholder="Enter comment" {}
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea {
        name,
        placeholder,
        value,
        rows,
        cols,
    } = &result[0].element
    {
        assert_eq!(name, &Some("comment".to_string()));
        assert_eq!(placeholder, &Some("Enter comment".to_string()));
        assert_eq!(value, "");
        assert_eq!(rows, &None);
        assert_eq!(cols, &None);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_with_value() {
    let result = container! {
        textarea name="message" {
            "Hello world"
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea { name, value, .. } = &result[0].element {
        assert_eq!(name, &Some("message".to_string()));
        assert_eq!(value, "Hello world");
        assert_eq!(result[0].children.len(), 0);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_with_rows_cols() {
    let result = container! {
        textarea rows="10" cols="50" {}
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea { rows, cols, .. } = &result[0].element {
        assert_eq!(rows, &Some(Number::Integer(10)));
        assert_eq!(cols, &Some(Number::Integer(50)));
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_with_children() {
    let result = container! {
        textarea name="test" {
            "Default text content"
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea { name, value, .. } = &result[0].element {
        assert_eq!(name, &Some("test".to_string()));
        assert_eq!(value, "Default text content");
        assert_eq!(result[0].children.len(), 0);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_with_dynamic_value() {
    let initial_value = "Dynamic content";
    let result = container! {
        textarea name="dynamic" {
            (initial_value)
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea { name, value, .. } = &result[0].element {
        assert_eq!(name, &Some("dynamic".to_string()));
        assert_eq!(value, "Dynamic content");
        assert_eq!(result[0].children.len(), 0);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_with_dynamic_placeholder() {
    let placeholder_text = "Type something...";
    let result = container! {
        textarea placeholder=(placeholder_text) {}
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea { placeholder, .. } = &result[0].element {
        assert_eq!(placeholder, &Some("Type something...".to_string()));
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_all_attributes() {
    let result = container! {
        textarea name="full" placeholder="Placeholder text" rows="5" cols="40" {
            "Initial value"
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea {
        name,
        placeholder,
        value,
        rows,
        cols,
    } = &result[0].element
    {
        assert_eq!(name, &Some("full".to_string()));
        assert_eq!(placeholder, &Some("Placeholder text".to_string()));
        assert_eq!(value, "Initial value");
        assert_eq!(rows, &Some(Number::Integer(5)));
        assert_eq!(cols, &Some(Number::Integer(40)));
        assert_eq!(result[0].children.len(), 0);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_with_mixed_attributes() {
    let dynamic_value = "Dynamic value";
    let result = container! {
        textarea name="mixed" placeholder="Static placeholder" rows="3" {
            (dynamic_value)
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea {
        name,
        value,
        placeholder,
        rows,
        ..
    } = &result[0].element
    {
        assert_eq!(name, &Some("mixed".to_string()));
        assert_eq!(value, "Dynamic value");
        assert_eq!(placeholder, &Some("Static placeholder".to_string()));
        assert_eq!(rows, &Some(Number::Integer(3)));
        assert_eq!(result[0].children.len(), 0);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_textarea_empty() {
    let result = container! {
        textarea {}
    };

    assert_eq!(result.len(), 1);

    if let Element::Textarea {
        name,
        placeholder,
        value,
        rows,
        cols,
    } = &result[0].element
    {
        assert_eq!(name, &None);
        assert_eq!(placeholder, &None);
        assert_eq!(value, "");
        assert_eq!(rows, &None);
        assert_eq!(cols, &None);
    } else {
        panic!("Expected Textarea element, got: {:?}", result[0].element);
    }
}
