use hyperchad_template2_macros::container;
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
    assert_eq!(result[0].children.len(), 1);

    if let Element::Raw { value } = &result[0].children[0].element {
        println!("Raw value: '{}'", value);
        assert_eq!(value, "Name: 123");
    } else {
        panic!(
            "Expected Raw element, got: {:?}",
            result[0].children[0].element
        );
    }
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
    assert_eq!(result[0].children.len(), 1);

    if let Element::Raw { value } = &result[0].children[0].element {
        println!("Raw value: '{}'", value);
        assert_eq!(value, "start: middle :end");
    } else {
        panic!(
            "Expected Raw element, got: {:?}",
            result[0].children[0].element
        );
    }
}
