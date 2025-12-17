use hyperchad_template_macros::container;
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

    assert_eq!(div.children.len(), 2);

    let Element::Text { value } = &div.children[0].element else {
        panic!("Expected Text element, got: {:?}", div.children[0].element);
    };

    println!("Sequential concatenation result: '{value}'");
    assert_eq!(value, "Name: ");

    let Element::Text { value } = &div.children[1].element else {
        panic!("Expected Text element, got: {:?}", div.children[1].element);
    };

    println!("Sequential concatenation result: '{value}'");
    assert_eq!(value, "test_input");
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

    assert_eq!(div.children.len(), 5);

    let Element::Text { value } = &div.children[0].element else {
        panic!("Expected Text element, got: {:?}", div.children[0].element);
    };

    println!("Multiple concatenation result: '{value}'");
    assert_eq!(value, "Before: ");

    let Element::Text { value } = &div.children[1].element else {
        panic!("Expected Text element, got: {:?}", div.children[1].element);
    };

    println!("Multiple concatenation result: '{value}'");
    assert_eq!(value, "first");

    let Element::Text { value } = &div.children[2].element else {
        panic!("Expected Text element, got: {:?}", div.children[2].element);
    };

    println!("Multiple concatenation result: '{value}'");
    assert_eq!(value, " Middle: ");

    let Element::Text { value } = &div.children[3].element else {
        panic!("Expected Text element, got: {:?}", div.children[3].element);
    };

    println!("Multiple concatenation result: '{value}'");
    assert_eq!(value, "second");

    let Element::Text { value } = &div.children[4].element else {
        panic!("Expected Text element, got: {:?}", div.children[4].element);
    };

    println!("Multiple concatenation result: '{value}'");
    assert_eq!(value, " End");
}

#[test]
fn test_mixed_sequential_and_separate() {
    fn get_value() -> String {
        "value".to_string()
    }

    let result = container! {
        div {
            "Label: " (get_value())
            span { "Separate element" }
            "Another: " (get_value())
        }
    };

    assert_eq!(result.len(), 1);
    let div = &result[0];

    assert_eq!(div.children.len(), 5);

    let Element::Text { value } = &div.children[0].element else {
        panic!(
            "Expected first child to be Text element, got: {:?}",
            div.children[0].element
        );
    };

    assert_eq!(value, "Label: ");

    let Element::Text { value } = &div.children[1].element else {
        panic!(
            "Expected first child to be Text element, got: {:?}",
            div.children[1].element
        );
    };

    assert_eq!(value, "value");

    let Element::Span = &div.children[2].element else {
        panic!(
            "Expected second child to be Span element, got: {:?}",
            div.children[2].element
        );
    };

    let Element::Text { value } = &div.children[3].element else {
        panic!(
            "Expected third child to be Text element, got: {:?}",
            div.children[3].element
        );
    };

    assert_eq!(value, "Another: ");

    let Element::Text { value } = &div.children[4].element else {
        panic!(
            "Expected third child to be Text element, got: {:?}",
            div.children[4].element
        );
    };

    assert_eq!(value, "value");
}

#[test]
fn test_unwrapped_sequential_concatenation() {
    fn get_name() -> String {
        "John".to_string()
    }

    fn get_age() -> u32 {
        25
    }

    // Test text sequential concatenation at the top level (unwrapped)
    let result = container! {
        "Name: " (get_name()) ", Age: " (get_age()) " years old"
    };

    assert_eq!(result.len(), 5);

    let Element::Text { value } = &result[0].element else {
        panic!("Expected Text element, got: {:?}", result[0].element);
    };

    println!("Unwrapped concatenation result: '{value}'");
    assert_eq!(value, "Name: ");

    let Element::Text { value } = &result[1].element else {
        panic!("Expected Text element, got: {:?}", result[1].element);
    };

    println!("Unwrapped concatenation result: '{value}'");
    assert_eq!(value, "John");

    let Element::Text { value } = &result[2].element else {
        panic!("Expected Text element, got: {:?}", result[2].element);
    };

    println!("Unwrapped concatenation result: '{value}'");
    assert_eq!(value, ", Age: ");

    let Element::Text { value } = &result[3].element else {
        panic!("Expected Text element, got: {:?}", result[3].element);
    };

    println!("Unwrapped concatenation result: '{value}'");
    assert_eq!(value, "25");

    let Element::Text { value } = &result[4].element else {
        panic!("Expected Text element, got: {:?}", result[4].element);
    };

    println!("Unwrapped concatenation result: '{value}'");
    assert_eq!(value, " years old");
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

    assert_eq!(result.len(), 2);

    let Element::Text { value } = &result[0].element else {
        panic!("Expected Text element, got: {:?}", result[0].element);
    };

    println!("Simple unwrapped result: '{value}'");
    assert_eq!(value, "Label: ");

    let Element::Text { value } = &result[1].element else {
        panic!("Expected Text element, got: {:?}", result[1].element);
    };

    println!("Simple unwrapped result: '{value}'");
    assert_eq!(value, "test_value");
}
