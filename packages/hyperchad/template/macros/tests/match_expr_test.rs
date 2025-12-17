use hyperchad_template::container;

#[test]
fn test_basic_match() {
    #[allow(dead_code)]
    enum Demo {
        A,
        B,
    }

    let val = Demo::A;

    let containers = container! {
        @match val {
            Demo::A => {
                "A"
            }
            Demo::B => {
                "B"
            }
        }
    };

    // Verify that the srcset was concatenated correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    let hyperchad_transformer::Element::Text { value } = &container.element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "A");
}

#[test]
fn test_basic_match_with_div_child() {
    #[allow(dead_code)]
    enum Demo {
        A,
        B,
    }

    let val = Demo::A;

    let containers = container! {
        @match val {
            Demo::A => {
                div { "A" }
            }
            Demo::B => {
                div { "B" }
            }
        }
    };

    // Verify that the srcset was concatenated correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];
    assert_eq!(container.children.len(), 1);
    let container = &container.children[0];

    let hyperchad_transformer::Element::Text { value } = &container.element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "A");
}

#[test]
fn test_basic_match_with_struct_enums() {
    #[allow(dead_code)]
    enum Demo {
        A { value: String },
        B { value: String },
    }

    let val = Demo::A {
        value: "Bob".to_string(),
    };

    let containers = container! {
        @match val {
            Demo::A { value } => {
                "A: " (value)
            }
            Demo::B { value } => {
                "B: " (value)
            }
        }
    };

    // Verify that the srcset was concatenated correctly
    assert_eq!(containers.len(), 2);

    let hyperchad_transformer::Element::Text { value } = &containers[0].element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "A: ");

    let hyperchad_transformer::Element::Text { value } = &containers[1].element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "Bob");
}

#[test]
fn test_basic_match_with_struct_enums_with_ellipses() {
    #[allow(dead_code)]
    enum Demo {
        A { value: String, num: u32 },
        B { value: String, num: u32 },
    }

    let val = Demo::A {
        value: "Bob".to_string(),
        num: 100,
    };

    let containers = container! {
        @match val {
            Demo::A { value, .. } => {
                "A: " (value)
            }
            Demo::B { value, .. } => {
                "B: " (value)
            }
        }
    };

    // Verify that the srcset was concatenated correctly
    assert_eq!(containers.len(), 2);

    let hyperchad_transformer::Element::Text { value } = &containers[0].element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "A: ");

    let hyperchad_transformer::Element::Text { value } = &containers[1].element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "Bob");
}

#[test]
fn test_basic_match_with_direct_div() {
    #[allow(dead_code)]
    enum Demo {
        A,
        B,
    }

    let val = Demo::A;

    let containers = container! {
        @match val {
            Demo::A => div { "A" }
            Demo::B => div { "B" }
        }
    };

    // Verify that the direct element works correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];
    assert_eq!(container.children.len(), 1);
    let container = &container.children[0];

    let hyperchad_transformer::Element::Text { value } = &container.element else {
        panic!("Expected Text element");
    };

    assert_eq!(value, "A");
}

#[test]
fn test_basic_match_with_empty_arms() {
    #[allow(dead_code)]
    enum Demo {
        A,
        B,
    }

    let val = Demo::A;

    let containers = container! {
        @match val {
            Demo::A => {}
            Demo::B => {}
        }
    };

    // Verify that empty arms produce no containers
    assert_eq!(containers.len(), 0);
}

#[test]
fn test_empty_braces_in_elements() {
    let containers = container! {
        div {}
    };

    // Verify that empty element blocks work correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];
    assert_eq!(container.children.len(), 0); // Empty div should have no children

    // Verify it's a div element
    assert!(matches!(
        container.element,
        hyperchad_transformer::Element::Div
    ));
}
