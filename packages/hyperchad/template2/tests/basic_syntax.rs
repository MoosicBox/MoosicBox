use hyperchad_template2::{ContainerVecExt, container};

#[test]
fn literals() {
    let result = container! { "du\tcks" "-23" "3.14\n" "geese" };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "du\tcks-23geese"
    );
}

#[test]
fn semicolons() {
    let result = container! {
        "one";
        "two";
        "three";
        ;;;;;;;;;;;;;;;;;;;;;;;;
        "four";
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "onetwothreefour"
    );
}

#[test]
fn blocks() {
    let result = container! {
        "hello"
        {
            " ducks" " geese"
        }
        " swans"
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "hello ducks geese swans"
    );
}

#[test]
fn simple_elements() {
    let result = container! { Div { Span { "pickle" } "barrel" Span { "kumquat" } } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div><span>pickle</span>barrel<span>kumquat</span></div>"#
    );
}

#[test]
fn simple_div() {
    let result = container! { Div { "Hello World" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div>Hello World</div>"#
    );
}

#[test]
fn nested_elements() {
    let result = container! { Div { Span { "pickle" } "barrel" Span { "kumquat" } } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div><span>pickle</span>barrel<span>kumquat</span></div>"#
    );
}

#[test]
fn multiple_elements() {
    let result = container! {
        Div { "First" }
        Div { "Second" }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div>First</div><div>Second</div>"#
    );
}

#[test]
fn simple_attributes() {
    let result = container! {
        Anchor href="https://example.com" {
            "Click here"
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<a href="https://example.com">Click here</a>"#
    );
}

#[test]
fn with_styling() {
    let result = container! { Div width="100" height="50" { "Styled div" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div sx-height="50" sx-width="100">Styled div</div>"#
    );
}

#[test]
fn with_classes() {
    let result = container! { Div.my-class.another-class { "With classes" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div class="my-class another-class">With classes</div>"#
    );
}

#[test]
fn with_id() {
    let result = container! { Div #my-id { "With ID" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div id="my-id">With ID</div>"#
    );
}

#[test]
fn class_shorthand() {
    let result = container! { Div.hotpink { "Hello!" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div class="hotpink">Hello!</div>"#
    );
}

#[test]
fn multiple_classes() {
    let result = container! { Div.first.second.third { "Multiple classes" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div class="first second third">Multiple classes</div>"#
    );
}

#[test]
fn id_shorthand() {
    let result = container! { Div #midriff { "With ID" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div id="midriff">With ID</div>"#
    );
}

#[test]
fn classes_attrs_ids_mixed_up() {
    let result = container! { Div.class1 #my-id width="100" .class2 { "Mixed attributes" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div class="class1 class2" id="my-id" sx-width="100">Mixed attributes</div>"#
    );
}

#[test]
fn heading_elements() {
    let result = container! {
        H1 { "Heading 1" }
        H2 { "Heading 2" }
        H3 { "Heading 3" }
        H4 { "Heading 4" }
        H5 { "Heading 5" }
        H6 { "Heading 6" }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<h1>Heading 1</h1><h2>Heading 2</h2><h3>Heading 3</h3><h4>Heading 4</h4><h5>Heading 5</h5><h6>Heading 6</h6>"#
    );
}

#[test]
fn list_elements() {
    let result = container! {
        Ul {
            Li { "Item 1" }
            Li { "Item 2" }
            Li { "Item 3" }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul>"#
    );
}

#[test]
fn ordered_list_elements() {
    let result = container! {
        Ol {
            Li { "First item" }
            Li { "Second item" }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<ol><li>First item</li><li>Second item</li></ol>"#
    );
}

#[test]
fn input_element() {
    let result = container! { Input type=text; };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<input type="text" />"#
    );
}

#[test]
fn input_with_attributes() {
    let result = container! { Input type="text" placeholder="Enter name" value="default"; };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<input type="text" value="default" placeholder="Enter name" />"#
    );
}

#[test]
fn button_element() {
    let result = container! { Button { "Click me" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<button>Click me</button>"#
    );
}

#[test]
fn button_with_type() {
    let result = container! { Button type="submit" { "Submit" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<button type="submit">Submit</button>"#
    );
}

#[test]
fn image_element() {
    let result = container! { Image src="image.jpg" alt="An image"; };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<img src="image.jpg" alt="An image" />"#
    );
}

#[test]
fn table_elements() {
    let result = container! {
        Table {
            THead {
                TR {
                    TH { "Header 1" }
                    TH { "Header 2" }
                }
            }
            TBody {
                TR {
                    TD { "Cell 1" }
                    TD { "Cell 2" }
                }
                TR {
                    TD { "Cell 3" }
                    TD { "Cell 4" }
                }
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<table><thead><tr sx-dir="col"><th>Header 1</th><th>Header 2</th></tr></thead><tbody><tr sx-dir="col"><td>Cell 1</td><td>Cell 2</td></tr><tr sx-dir="col"><td>Cell 3</td><td>Cell 4</td></tr></tbody></table>"#
    );
}

#[test]
fn form_elements() {
    let result = container! {
        Form {
            Input type="text" placeholder="Name";
            Button type="submit" { "Submit" }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<form><input type="text" placeholder="Name" /><button type="submit">Submit</button></form>"#
    );
}

#[test]
fn semantic_elements() {
    let result = container! {
        Main {
            Header {
                H1 { "Site Title" }
            }
            Section {
                H2 { "Content Title" }
                Div { "Content goes here" }
            }
            Aside {
                Div { "Sidebar content" }
            }
            Footer {
                Div { "Footer content" }
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<main><header><h1>Site Title</h1></header><section><h2>Content Title</h2><div>Content goes here</div></section><aside><div>Sidebar content</div></aside><footer><div>Footer content</div></footer></main>"#
    );
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

#[test]
fn mixed_content() {
    let result = container! {
        Div {
            "Text before element "
            Span { "inside span" }
            " text after element"
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div>Text before element <span>inside span</span> text after element</div>"
    );
}

#[test]
fn data_attributes() {
    let result = container! { Div data-id="123" data-name="test" { "With data attributes" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div data-id="123" data-name="test">With data attributes</div>"#
    );
}

#[test]
fn input_variations() {
    let result = container! {
        Input type="checkbox" checked;
        Input type="hidden" value="secret";
    };

    let value = result.display_to_string(false, false).unwrap();

    assert!(value.contains("checkbox"));
    assert!(value.contains("checked"));
    assert!(value.contains("hidden"));
}
