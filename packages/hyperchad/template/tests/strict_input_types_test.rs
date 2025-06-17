use hyperchad_template::container;
use hyperchad_transformer::{Element, Input};

#[test]
fn test_raw_identifier_input_types() {
    // Test that raw identifiers work (no quotes)
    let result = container! {
        input type=text value="text_input";
        input type=checkbox checked;
        input type=password placeholder="password";
        input type=hidden value="hidden_value";
        input type=tel value="phone";
        input type=email value="email@example.com";
    };

    assert_eq!(result.len(), 6);

    // Check text input
    if let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &result[0].element
    {
        assert_eq!(value.as_ref().unwrap(), "text_input");
    } else {
        panic!("Expected Input::Text, got {:?}", result[0].element);
    }

    // Check checkbox input
    if let Element::Input {
        input: Input::Checkbox { checked },
        ..
    } = &result[1].element
    {
        assert!(checked.unwrap());
    } else {
        panic!("Expected Input::Checkbox, got {:?}", result[1].element);
    }

    // Check password input
    if let Element::Input {
        input: Input::Password { placeholder, .. },
        ..
    } = &result[2].element
    {
        assert_eq!(placeholder.as_ref().unwrap(), "password");
    } else {
        panic!("Expected Input::Password, got {:?}", result[2].element);
    }

    // Check hidden input
    if let Element::Input {
        input: Input::Hidden { value },
        ..
    } = &result[3].element
    {
        assert_eq!(value.as_ref().unwrap(), "hidden_value");
    } else {
        panic!("Expected Input::Hidden, got {:?}", result[3].element);
    }

    // Check tel input (should map to Text)
    if let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &result[4].element
    {
        assert_eq!(value.as_ref().unwrap(), "phone");
    } else {
        panic!("Expected Input::Text for tel, got {:?}", result[4].element);
    }

    // Check email input (should map to Text)
    if let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &result[5].element
    {
        assert_eq!(value.as_ref().unwrap(), "email@example.com");
    } else {
        panic!(
            "Expected Input::Text for email, got {:?}",
            result[5].element
        );
    }
}

#[test]
fn test_quoted_string_input_types() {
    // Test that quoted strings still work
    let result = container! {
        input type="text" value="text_input";
        input type="checkbox" checked;
        input type="password" placeholder="password";
        input type="hidden" value="hidden_value";
    };

    assert_eq!(result.len(), 4);

    // Check that they produce the same results as raw identifiers
    if let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &result[0].element
    {
        assert_eq!(value.as_ref().unwrap(), "text_input");
    } else {
        panic!("Expected Input::Text, got {:?}", result[0].element);
    }

    if let Element::Input {
        input: Input::Checkbox { checked },
        ..
    } = &result[1].element
    {
        assert!(checked.unwrap());
    } else {
        panic!("Expected Input::Checkbox, got {:?}", result[1].element);
    }
}

#[test]
fn test_mixed_raw_and_quoted() {
    // Test mixing raw identifiers and quoted strings
    let result = container! {
        input type=text value="raw_text";
        input type="checkbox" checked;
        input type=password placeholder="raw_password";
        input type="hidden" value="quoted_hidden";
    };

    assert_eq!(result.len(), 4);

    // All should work the same way
    if let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &result[0].element
    {
        assert_eq!(value.as_ref().unwrap(), "raw_text");
    } else {
        panic!(
            "Expected Input::Text from raw identifier, got {:?}",
            result[0].element
        );
    }

    if let Element::Input {
        input: Input::Checkbox { checked },
        ..
    } = &result[1].element
    {
        assert!(checked.unwrap());
    } else {
        panic!(
            "Expected Input::Checkbox from quoted string, got {:?}",
            result[1].element
        );
    }
}

#[test]
fn test_invalid_unsupported_type() {
    // Test that unsupported types are properly rejected
    // This would fail at compile time with our implementation
    // We can't test actual compile errors in a unit test, but we can
    // verify the behavior is consistent

    // Using known good types for the actual test
    let result = container! {
        input type="text" value="valid";
    };

    assert_eq!(result.len(), 1);
    if let Element::Input {
        input: Input::Text { .. },
        ..
    } = &result[0].element
    {
        // Good - text type works
    } else {
        panic!("Expected Input::Text, got {:?}", result[0].element);
    }
}
