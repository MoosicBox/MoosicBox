use hyperchad_template2::container;

#[test]
fn test_flex_with_single_value() {
    let containers = container! {
        Div flex=(1) {
            "Test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("Test"), "Should contain the text content");
    // Note: The exact flex CSS output depends on the transformer implementation
}

#[test]
fn test_flex_with_string_value() {
    let containers = container! {
        Div flex="1 0 auto" {
            "Flex string test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Flex string test"),
        "Should contain the text content"
    );
}

#[test]
fn test_flex_with_individual_properties() {
    let containers = container! {
        Div flex-grow=1 flex-shrink=0 flex-basis=200 {
            "Individual flex properties"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Individual flex properties"),
        "Should contain the text content"
    );
}

#[test]
fn test_flex_with_expression() {
    let grow_value = 2;
    let containers = container! {
        Div flex=(grow_value) {
            "Dynamic flex"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Dynamic flex"),
        "Should contain the text content"
    );
}

#[test]
fn test_multiple_flex_containers() {
    let containers = container! {
        Div flex=1 {
            "First flex item"
        }
        Div flex=2 {
            "Second flex item"
        }
        Div flex="0 1 auto" {
            "Third flex item"
        }
    };

    assert_eq!(containers.len(), 3, "Should generate three containers");

    let html: String = containers.iter().map(|c| c.to_string()).collect();
    assert!(
        html.contains("First flex item"),
        "Should contain first item"
    );
    assert!(
        html.contains("Second flex item"),
        "Should contain second item"
    );
    assert!(
        html.contains("Third flex item"),
        "Should contain third item"
    );
}
