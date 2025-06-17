use hyperchad_template2::{ContainerVecExt, container};

#[test]
fn literals() {
    let result = container! { ("<pinkie>") };
    assert_eq!(result.display_to_string(false, false).unwrap(), "<pinkie>");
}

#[test]
fn blocks() {
    let result = container! {
        ({
            let mut result = 1i32;
            for i in 2..11 {
                result *= i;
            }
            result
        })
    };
    assert_eq!(result.display_to_string(false, false).unwrap(), "3628800");
}

#[test]
fn string_interpolation() {
    let name = "Pinkie Pie";
    let result = container! { "Hello, " (name) "!" };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Hello, Pinkie Pie!"
    );
}

#[test]
fn numeric_interpolation() {
    let number = 42;
    let result = container! { "The answer is " (number) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "The answer is 42"
    );
}

#[test]
fn attributes() {
    let alt = "Pinkie Pie";
    let result = container! { Image src="pinkie.jpg" alt=(alt); };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<img src="pinkie.jpg" alt="Pinkie Pie" />"#
    );
}

#[test]
fn class_shorthand() {
    let _pinkie_class = "pinkie";
    let result = container! { Div.pinkie { "Fun!" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div class="pinkie">Fun!</div>"#
    );
}

#[test]
fn class_shorthand_block() {
    let _class_prefix = "pinkie-";
    let result = container! { Div.pinkie-123 { "Fun!" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div class="pinkie-123">Fun!</div>"#
    );
}

#[test]
fn id_shorthand() {
    let pinkie_id = "pinkie";
    let result = container! { Div #(pinkie_id) { "Fun!" } };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        r#"<div id="pinkie">Fun!</div>"#
    );
}

static BEST_PONY: &str = "Pinkie Pie";

#[test]
fn statics() {
    let result = container! { (BEST_PONY) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Pinkie Pie"
    );
}

#[test]
fn locals() {
    let best_pony = "Pinkie Pie";
    let result = container! { (best_pony) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Pinkie Pie"
    );
}

/// An example struct, for testing purposes only
struct Creature {
    name: &'static str,
    /// Rating out of 10, where:
    /// * 0 is a naked mole rat with dysentery
    /// * 10 is Sweetie Belle in a milkshake
    adorableness: u32,
}

impl Creature {
    fn repugnance(&self) -> u32 {
        10 - self.adorableness
    }
}

#[test]
fn structs() {
    let pinkie = Creature {
        name: "Pinkie Pie",
        adorableness: 9,
    };
    let result = container! {
        "Name: " (pinkie.name) ". Rating: " (pinkie.repugnance())
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Name: Pinkie Pie. Rating: 1"
    );
}

#[test]
fn tuple_accessors() {
    let a = ("ducks", "geese");
    let result = container! { (a.0) };
    assert_eq!(result.display_to_string(false, false).unwrap(), "ducks");
}

#[test]
fn splice_with_path() {
    mod inner {
        pub fn name() -> &'static str {
            "Maud"
        }
    }
    let result = container! { (inner::name()) };
    assert_eq!(result.display_to_string(false, false).unwrap(), "Maud");
}

#[test]
fn nested_macro_invocation() {
    let best_pony = "Pinkie Pie";
    let result = container! { (format!("{best_pony} is best pony")) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Pinkie Pie is best pony"
    );
}

#[test]
fn expression_grouping() {
    let result = container! { (1 + 1) };
    assert_eq!(result.display_to_string(false, false).unwrap(), "2");
}

#[test]
fn multiple_expressions() {
    let x = 5;
    let y = 10;
    let result = container! { (x) " + " (y) " = " (x + y) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "5 + 10 = 15"
    );
}

#[test]
fn boolean_expressions() {
    let is_true = true;
    let is_false = false;
    let result = container! { (is_true) " and " (is_false) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "true and false"
    );
}

#[test]
fn array_access() {
    let items = ["apple", "banana", "cherry"];
    let result = container! { (items[0]) " and " (items[2]) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "apple and cherry"
    );
}

#[test]
fn method_calls() {
    let text = "Hello World";
    let result = container! { (text.to_lowercase()) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "hello world"
    );
}

#[test]
fn chained_method_calls() {
    let text = "  Hello World  ";
    let result = container! { (text.trim().to_uppercase()) };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "HELLO WORLD"
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
fn vector_operations() {
    let numbers = [1, 2, 3, 4, 5];
    let result = container! {
        "Length: " (numbers.len()) ", Sum: " (numbers.iter().sum::<i32>())
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Length: 5, Sum: 15"
    );
}
