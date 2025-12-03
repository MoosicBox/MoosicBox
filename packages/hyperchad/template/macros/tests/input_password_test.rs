#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template::container;
use hyperchad_transformer::{Element, Input};

#[test_log::test]
fn test_password_input_basic() {
    let containers = container! {
        input type="password" name="user_password";
    };

    assert_eq!(containers.len(), 1);

    if let Element::Input { input, name, .. } = &containers[0].element {
        assert_eq!(name.as_deref(), Some("user_password"));

        if let Input::Password { value, placeholder } = input {
            assert_eq!(*value, None);
            assert_eq!(*placeholder, None);
        } else {
            panic!("Expected Input::Password, got: {input:?}");
        }
    } else {
        panic!("Expected Input element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_password_input_with_placeholder() {
    let containers = container! {
        input type="password" name="pwd" placeholder="Enter your password";
    };

    assert_eq!(containers.len(), 1);

    if let Element::Input { input, .. } = &containers[0].element {
        if let Input::Password { placeholder, .. } = input {
            assert_eq!(placeholder.as_deref(), Some("Enter your password"));
        } else {
            panic!("Expected Input::Password, got: {input:?}");
        }
    } else {
        panic!("Expected Input element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_password_input_with_dynamic_placeholder() {
    let placeholder_text = "Dynamic placeholder";

    let containers = container! {
        input type="password" name="pwd" placeholder=(placeholder_text);
    };

    assert_eq!(containers.len(), 1);

    if let Element::Input { input, .. } = &containers[0].element {
        if let Input::Password { placeholder, .. } = input {
            assert_eq!(placeholder.as_deref(), Some("Dynamic placeholder"));
        } else {
            panic!("Expected Input::Password, got: {input:?}");
        }
    } else {
        panic!("Expected Input element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_password_input_with_autofocus() {
    let containers = container! {
        input type="password" name="pwd" autofocus;
    };

    assert_eq!(containers.len(), 1);

    if let Element::Input {
        input, autofocus, ..
    } = &containers[0].element
    {
        assert_eq!(*autofocus, Some(true));
        assert!(matches!(input, Input::Password { .. }));
    } else {
        panic!("Expected Input element, got: {:?}", containers[0].element);
    }
}
