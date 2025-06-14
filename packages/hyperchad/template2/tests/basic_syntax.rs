use hyperchad_template2::{ContainerVecExt, container};

#[ignore]
#[test]
fn literals() {
    let result = container! { "Hello, world!" };
    assert_eq!(result.to_string(), "Hello, world!");
}

#[ignore]
#[test]
fn simple_div() {
    let result = container! { Div { "Hello World" } };
    assert_eq!(result.to_string(), "<div>Hello World</div>");
}

#[ignore]
#[test]
fn nested_elements() {
    let result = container! { Div { Span { "pickle" } "barrel" Span { "kumquat" } } };
    assert_eq!(
        result.to_string(),
        "<div><span>pickle</span>barrel<span>kumquat</span></div>"
    );
}

#[ignore]
#[test]
fn multiple_elements() {
    let result = container! {
        Div { "First" }
        Div { "Second" }
    };
    assert_eq!(result.to_string(), "<div>First</div><div>Second</div>");
}

#[ignore]
#[test]
fn with_styling() {
    let result = container! { Div width="100" height="50" { "Styled div" } };
    assert_eq!(result.to_string(), "<div>Styled div</div>");
}

#[ignore]
#[test]
fn with_classes() {
    let result = container! { Div.my-class.another-class { "With classes" } };
    assert_eq!(
        result.to_string(),
        r#"<div class="my-class another-class">With classes</div>"#
    );
}

#[ignore]
#[test]
fn with_id() {
    let result = container! { Div #my-id { "With ID" } };
    assert_eq!(result.to_string(), r#"<div id="my-id">With ID</div>"#);
}

#[ignore]
#[test]
fn heading_elements() {
    let result = container! {
        H1 { "Heading 1" }
        H2 { "Heading 2" }
        H3 { "Heading 3" }
    };
    assert_eq!(
        result.to_string(),
        "<h1>Heading 1</h1><h2>Heading 2</h2><h3>Heading 3</h3>"
    );
}

#[ignore]
#[test]
fn list_elements() {
    let result = container! {
        Ul {
            Li { "Item 1" }
            Li { "Item 2" }
        }
    };
    assert_eq!(
        result.to_string(),
        "<ul><li>Item 1</li><li>Item 2</li></ul>"
    );
}

#[ignore]
#[test]
fn input_element() {
    let result = container! { Input; };
    assert_eq!(result.to_string(), "<input>");
}

#[ignore]
#[test]
fn button_element() {
    let result = container! { Button { "Click me" } };
    assert_eq!(result.to_string(), "<button>Click me</button>");
}

#[test]
fn flex_attributes() {
    // Test shorthand flex attribute
    let result = container! { Div flex="1 0 auto" { "Flex container" } };
    let container = &result[0];
    assert!(container.flex.is_some());

    // Test individual flex attributes
    let result =
        container! { Div flex-grow="2" flex-shrink="0" flex-basis="100px" { "Individual flex" } };
    let container = &result[0];
    assert!(container.flex.is_some());
}

#[test]
fn text_decoration_attributes() {
    // Test basic text-decoration attribute
    let result = container! { Div text-decoration="underline" { "Decorated text" } };
    let container = &result[0];
    assert!(container.text_decoration.is_some());

    // Test text-decoration none
    let result = container! { Div text-decoration="none" { "No decoration" } };
    let container = &result[0];
    assert!(container.text_decoration.is_some());
}
