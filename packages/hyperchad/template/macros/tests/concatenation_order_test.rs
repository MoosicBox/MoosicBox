use hyperchad_template_macros::container;
use hyperchad_transformer::Element;

#[test]
fn test_concatenation_order() {
    let item_id = "123";

    let result = container! {
        div {
            {"Name: " (item_id)}
        }
    };

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].children.len(), 2);

    let Element::Text { value } = &result[0].children[0].element else {
        panic!(
            "Expected Text element, got: {:?}",
            result[0].children[0].element
        );
    };

    println!("Text value: '{value}'");
    assert_eq!(value, "Name: ");

    let Element::Text { value } = &result[0].children[1].element else {
        panic!(
            "Expected Text element, got: {:?}",
            result[0].children[1].element
        );
    };

    println!("Text value: '{value}'");
    assert_eq!(value, "123");
}

#[test]
fn test_multiple_concatenation_order() {
    let prefix = "start";
    let suffix = "end";

    let result = container! {
        div {
            {(prefix) ": middle :" (suffix)}
        }
    };

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].children.len(), 3);

    let Element::Text { value } = &result[0].children[0].element else {
        panic!(
            "Expected Text element, got: {:?}",
            result[0].children[0].element
        );
    };

    println!("Text value: '{value}'");
    assert_eq!(value, "start");

    let Element::Text { value } = &result[0].children[1].element else {
        panic!(
            "Expected Text element, got: {:?}",
            result[0].children[1].element
        );
    };

    println!("Text value: '{value}'");
    assert_eq!(value, ": middle :");

    let Element::Text { value } = &result[0].children[2].element else {
        panic!(
            "Expected Text element, got: {:?}",
            result[0].children[2].element
        );
    };

    println!("Text value: '{value}'");
    assert_eq!(value, "end");
}
