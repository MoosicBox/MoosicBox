use hyperchad_template_macros::container;
use hyperchad_transformer::{Element, Input};

#[test]
fn test_simple_input() {
    let result = container! {
        input type="text" name="test" value="static_value";
    };

    assert_eq!(result.len(), 1);

    if let Element::Input {
        input,
        name,
        autofocus,
    } = &result[0].element
    {
        assert_eq!(name, &Some("test".to_string()));
        assert_eq!(autofocus, &None);

        if let Input::Text { value, .. } = input {
            assert_eq!(value, &Some("static_value".to_string()));
        } else {
            panic!("Expected Input::Text, got: {input:?}");
        }
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_autofocus_input() {
    let result = container! {
        input type="text" name="test" value="static_value" autofocus;
    };

    assert_eq!(result.len(), 1);

    if let Element::Input { autofocus, .. } = &result[0].element {
        assert_eq!(autofocus, &Some(true));
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_input_types() {
    let result = container! {
        input type="checkbox" name="check" checked;
    };

    assert_eq!(result.len(), 1);

    if let Element::Input {
        input,
        name,
        autofocus,
    } = &result[0].element
    {
        assert_eq!(name, &Some("check".to_string()));
        assert_eq!(autofocus, &None);

        if let Input::Checkbox { checked } = input {
            assert_eq!(checked, &Some(true));
        } else {
            panic!("Expected Input::Checkbox, got: {input:?}");
        }
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}

#[test]
fn test_input_hidden() {
    let result = container! {
        input type="hidden" name="hidden_field" value="hidden_value";
    };

    assert_eq!(result.len(), 1);

    if let Element::Input {
        input,
        name,
        autofocus,
    } = &result[0].element
    {
        assert_eq!(name, &Some("hidden_field".to_string()));
        assert_eq!(autofocus, &None);

        if let Input::Hidden { value } = input {
            assert_eq!(value, &Some("hidden_value".to_string()));
        } else {
            panic!("Expected Input::Hidden, got: {input:?}");
        }
    } else {
        panic!("Expected Input element, got: {:?}", result[0].element);
    }
}
