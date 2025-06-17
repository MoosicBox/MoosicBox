use hyperchad_template_macros::container;
use hyperchad_transformer::{Element, Input};

#[test]
fn test_input_value_expression() {
    let connection_name = "test_connection";

    let result = container! {
        input type="text" name="connection" value=(connection_name);
    };

    // Check that we have one container with an Input element
    assert_eq!(result.len(), 1);

    if let Element::Input { input, name } = &result[0].element {
        // Check that the name field is set correctly
        assert_eq!(name, &Some("connection".to_string()));

        // Check that the input is a Text variant with the correct value
        if let Input::Text {
            value,
            placeholder: _,
        } = input
        {
            assert_eq!(value, &Some("test_connection".to_string()));
        } else {
            panic!("Expected Input::Text, got: {:?}", input);
        }
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_input_placeholder_expression() {
    let placeholder_text = "Enter your name";

    let result = container! {
        input type="text" placeholder=(placeholder_text);
    };

    assert_eq!(result.len(), 1);

    if let Element::Input { input, .. } = &result[0].element {
        if let Input::Text {
            value: _,
            placeholder,
        } = input
        {
            assert_eq!(placeholder, &Some("Enter your name".to_string()));
        } else {
            panic!("Expected Input::Text, got: {:?}", input);
        }
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_anchor_href_expression() {
    let url = "https://example.com";

    let result = container! {
        anchor href=(url) {
            "Click here"
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Anchor { href, target: _ } = &result[0].element {
        assert_eq!(href, &Some("https://example.com".to_string()));
    } else {
        panic!("Expected anchor element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_button_type_expression() {
    let button_type = "submit";

    let result = container! {
        button type=(button_type) {
            "Submit"
        }
    };

    assert_eq!(result.len(), 1);

    if let Element::Button { r#type } = &result[0].element {
        assert_eq!(r#type, &Some("submit".to_string()));
    } else {
        panic!("Expected Button element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_mixed_expressions_and_literals() {
    let dynamic_value = "dynamic";

    let result = container! {
        input type="text" name="mixed" value=(dynamic_value) placeholder="Static placeholder";
    };

    assert_eq!(result.len(), 1);

    if let Element::Input { input, name } = &result[0].element {
        assert_eq!(name, &Some("mixed".to_string()));

        if let Input::Text { value, placeholder } = input {
            assert_eq!(value, &Some("dynamic".to_string()));
            assert_eq!(placeholder, &Some("Static placeholder".to_string()));
        } else {
            panic!("Expected Input::Text, got: {:?}", input);
        }
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}
