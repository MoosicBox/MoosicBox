use hyperchad_template::{Container, ContainerVecExt, RenderContainer, container};

#[test]
fn issue_13_ownership() {
    let owned = String::from("yay");
    let _ = container! { (owned) };
    // Make sure the `container!` call didn't move it
    let _owned = owned;
}

#[test]
fn macro_interaction() {
    macro_rules! greet {
        () => {{
            let name = "Pinkie Pie";
            container! {
                Div { "Hello, " (name) "!" }
            }
        }};
    }

    assert_eq!(
        greet!().display_to_string(false, false).unwrap(),
        "<div>Hello, Pinkie Pie!</div>"
    );
}

#[test]
fn macro_with_parameters() {
    macro_rules! greet {
        ($name:expr) => {{
            container! {
                Div { "Hello, " ($name) "!" }
            }
        }};
    }

    assert_eq!(
        greet!("Pinkie Pie")
            .display_to_string(false, false)
            .unwrap(),
        "<div>Hello, Pinkie Pie!</div>"
    );
}

#[test]
fn nested_macro_wrapper() {
    macro_rules! wrapper {
        ($($x:tt)*) => {{
            container! { $($x)* }
        }}
    }

    let name = "Lyra";
    let result = wrapper!(Div { "Hi, " (name) "!" });
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div>Hi, Lyra!</div>"
    );
}

#[test]
fn render_impl() {
    struct R(&'static str);
    impl RenderContainer for R {
        type Error = String; // Use String instead of core::fmt::Error

        fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
            containers.push(Container {
                element: hyperchad_transformer::Element::Raw {
                    value: self.0.to_string(),
                },
                ..Default::default()
            });
            Ok(())
        }
    }

    let r = R("pinkie");
    let result_a = container! { (r) };
    let result_b = container! { (r) };
    assert_eq!(result_a.display_to_string(false, false).unwrap(), "pinkie");
    assert_eq!(result_b.display_to_string(false, false).unwrap(), "pinkie");
}

#[test]
fn display_implementation() {
    use std::fmt::Display;

    struct DisplayOnly;
    impl Display for DisplayOnly {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "<hello>")
        }
    }

    impl RenderContainer for DisplayOnly {
        type Error = String;

        fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
            containers.push(Container {
                element: hyperchad_transformer::Element::Raw {
                    value: format!("{}", self),
                },
                ..Default::default()
            });
            Ok(())
        }
    }

    assert_eq!(
        container! { (DisplayOnly) }
            .display_to_string(false, false)
            .unwrap(),
        "<hello>"
    );
}

#[test]
fn smart_pointers() {
    use std::rc::Rc;
    use std::sync::Arc;

    let arc = Arc::new("foo");
    let rc = Rc::new("bar");

    assert_eq!(
        container! { (arc) }
            .display_to_string(false, false)
            .unwrap(),
        "foo"
    );
    assert_eq!(
        container! { (rc) }.display_to_string(false, false).unwrap(),
        "bar"
    );
}

#[test]
fn option_handling() {
    let some_value = Some("exists");
    let none_value: Option<&str> = None;

    let result = container! {
        (some_value.unwrap_or("default")) " "
        (none_value.unwrap_or("default"))
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "exists default"
    );
}

#[test]
fn numeric_types() {
    let int8: i8 = 127;
    let int16: i16 = 32767;
    let int32: i32 = 2147483647;
    let int64: i64 = 9223372036854775807;
    let uint8: u8 = 255;
    #[allow(clippy::approx_constant)]
    let float32: f32 = 3.14159;
    #[allow(clippy::approx_constant)]
    let float64: f64 = 2.718281828459045;

    let result = container! {
        (int8) " " (int16) " " (int32) " " (int64) " "
        (uint8) " " (float32) " " (float64)
    };

    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "127 32767 2147483647 9223372036854775807 255 3.14159 2.718281828459045"
    );
}

#[test]
fn character_escaping() {
    let special_chars = "<>&\"'";
    let result = container! { (special_chars) };
    assert_eq!(result.display_to_string(false, false).unwrap(), "<>&\"'");
}

#[test]
fn unicode_support() {
    let unicode = "Hello 世界 🦀 Καλημέρα";
    let result = container! { (unicode) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Hello 世界 🦀 Καλημέρα"
    );
}

#[test]
fn empty_containers() {
    let result = container! {
        Div {}
        Span {}
        Section {}
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div></div><span></span><section></section>"
    );
}

#[ignore]
#[test]
fn whitespace_handling() {
    let result = container! {
        Div { "   " }
        Div { "\n\t" }
        Div { " hello world " }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div>   </div><div>\n\t</div><div> hello world </div>"
    );
}

#[test]
fn nested_structures() {
    struct Person {
        name: String,
        age: u32,
        address: Address,
    }

    struct Address {
        street: String,
        city: String,
    }

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        address: Address {
            street: "123 Main St".to_string(),
            city: "Springfield".to_string(),
        },
    };

    let result = container! {
        Div {
            "Name: " (person.name) ", Age: " (person.age)
            ", Address: " (person.address.street) ", " (person.address.city)
        }
    };

    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div>Name: Alice, Age: 30, Address: 123 Main St, Springfield</div>"
    );
}

#[test]
fn collection_iteration() {
    let vec_data = vec!["apple", "banana", "cherry"];
    let array_data = ["one", "two", "three"];

    let result = container! {
        Div {
            "Vec: "
            @for item in &vec_data {
                (item) " "
            }
        }
        Div {
            "Array: "
            @for item in &array_data {
                (item) " "
            }
        }
    };

    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div>Vec: apple banana cherry </div><div>Array: one two three </div>"
    );
}

#[test]
fn complex_expressions() {
    let x: i32 = 5;
    let y: i32 = 10;
    let result = container! {
        "Sum: " (x + y) ", "
        "Product: " (x * y) ", "
        "Average: " ((x + y) as f32 / 2.0) ", "
        "Power: " (x.pow(2))
    };

    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Sum: 15, Product: 50, Average: 7.5, Power: 25"
    );
}

#[test]
fn method_chaining() {
    let text = "  Hello World  ";
    let result = container! {
        (text.trim().to_uppercase().replace("HELLO", "HI"))
    };
    assert_eq!(result.display_to_string(false, false).unwrap(), "HI WORLD");
}

#[test]
fn closure_usage() {
    let transform = |x: i32| x * 2;
    let numbers = vec![1, 2, 3, 4, 5];

    let result = container! {
        "Transformed: "
        @for num in &numbers {
            (transform(*num)) " "
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Transformed: 2 4 6 8 10 "
    );
}

#[test]
fn default_test() {
    // Test that String::default() works
    assert_eq!(String::default(), "");
}
