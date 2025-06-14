use hyperchad_template2_macros::container;
use hyperchad_transformer::Element;

#[test]
fn test_sequential_literal_splice() {
    fn connection_input() -> String {
        "test_input".to_string()
    }

    let result = container! {
        div { "Name: " (connection_input()) }
    };

    assert_eq!(result.len(), 1);
    let div = &result[0];

    // Should have one child - the concatenated raw element
    assert_eq!(div.children.len(), 1);

    if let Element::Raw { value } = &div.children[0].element {
        println!("Sequential concatenation result: '{}'", value);
        assert_eq!(value, "Name: test_input");
    } else {
        panic!("Expected Raw element, got: {:?}", div.children[0].element);
    }
}

#[test]
fn test_multiple_sequential_concatenations() {
    fn get_first() -> String {
        "first".to_string()
    }

    fn get_second() -> String {
        "second".to_string()
    }

    let result = container! {
        div {
            "Before: " (get_first()) " Middle: " (get_second()) " End"
        }
    };

    assert_eq!(result.len(), 1);
    let div = &result[0];

    // Should have one child - the concatenated raw element
    assert_eq!(div.children.len(), 1);

    if let Element::Raw { value } = &div.children[0].element {
        println!("Multiple concatenation result: '{}'", value);
        assert_eq!(value, "Before: first Middle: second End");
    } else {
        panic!("Expected Raw element, got: {:?}", div.children[0].element);
    }
}

#[test]
fn test_mixed_sequential_and_separate() {
    fn get_value() -> String {
        "value".to_string()
    }

    let result = container! {
        div {
            "Label: " (get_value())
            Span { "Separate element" }
            "Another: " (get_value())
        }
    };

    assert_eq!(result.len(), 1);
    let div = &result[0];

    // Should have three children: concatenated raw, span, concatenated raw
    assert_eq!(div.children.len(), 3);

    // First child: concatenated raw
    if let Element::Raw { value } = &div.children[0].element {
        assert_eq!(value, "Label: value");
    } else {
        panic!(
            "Expected first child to be Raw element, got: {:?}",
            div.children[0].element
        );
    }

    // Second child: span element
    if let Element::Span = &div.children[1].element {
        // Correct
    } else {
        panic!(
            "Expected second child to be Span element, got: {:?}",
            div.children[1].element
        );
    }

    // Third child: concatenated raw
    if let Element::Raw { value } = &div.children[2].element {
        assert_eq!(value, "Another: value");
    } else {
        panic!(
            "Expected third child to be Raw element, got: {:?}",
            div.children[2].element
        );
    }
}

#[test]
fn test_unwrapped_sequential_concatenation() {
    fn get_name() -> String {
        "John".to_string()
    }

    fn get_age() -> u32 {
        25
    }

    // Test raw sequential concatenation at the top level (unwrapped)
    let result = container! {
        "Name: " (get_name()) ", Age: " (get_age()) " years old"
    };

    // Should create a single raw container with concatenated content
    assert_eq!(result.len(), 1);

    if let Element::Raw { value } = &result[0].element {
        println!("Unwrapped concatenation result: '{}'", value);
        assert_eq!(value, "Name: John, Age: 25 years old");
    } else {
        panic!("Expected Raw element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_unwrapped_simple_literal_splice() {
    fn connection_input() -> String {
        "test_value".to_string()
    }

    // Test the original case: simple literal + splice at top level
    let result = container! {
        "Label: " (connection_input())
    };

    assert_eq!(result.len(), 1);

    if let Element::Raw { value } = &result[0].element {
        println!("Simple unwrapped result: '{}'", value);
        assert_eq!(value, "Label: test_value");
    } else {
        panic!("Expected Raw element, got: {:?}", result[0].element);
    }
}
