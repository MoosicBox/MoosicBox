use hyperchad_template2::container;

#[test]
fn test_simple_container_creation() {
    let containers = container! {
        Div {
            "Hello World"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Hello World"),
        "Should contain the text content"
    );
    assert!(html.contains("<div"), "Should generate a div element");
}

#[test]
fn test_empty_container() {
    let containers = container! {
        Div {}
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("<div"), "Should generate a div element");
}

#[test]
fn test_multiple_containers() {
    let containers = container! {
        Div { "First" }
        Div { "Second" }
        Span { "Third" }
    };

    assert_eq!(containers.len(), 3, "Should generate three containers");

    let html: String = containers.iter().map(|c| c.to_string()).collect();
    assert!(html.contains("First"), "Should contain first element");
    assert!(html.contains("Second"), "Should contain second element");
    assert!(html.contains("Third"), "Should contain third element");
}
