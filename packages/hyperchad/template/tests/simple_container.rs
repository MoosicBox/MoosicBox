use hyperchad_template::container;

#[test]
fn test_simple_container_creation() {
    let containers = container! {
        div {
            "Hello World"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(
        html.contains("Hello World"),
        "Should contain the text content"
    );
    assert!(html.contains("<div"), "Should generate a div element");
}

#[test]
fn test_empty_container() {
    let containers = container! {
        div {}
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("<div"), "Should generate a div element");
}

#[test]
fn test_multiple_containers() {
    let containers = container! {
        div { "First" }
        div { "Second" }
        span { "Third" }
    };

    assert_eq!(containers.len(), 3, "Should generate three containers");

    let html: String = containers
        .iter()
        .map(|c| c.display_to_string_default(false, false).unwrap())
        .collect();
    assert!(html.contains("First"), "Should contain first element");
    assert!(html.contains("Second"), "Should contain second element");
    assert!(html.contains("Third"), "Should contain third element");
}
