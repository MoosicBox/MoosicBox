use hyperchad_template2::{ContainerVecExt, container};

#[ignore]
#[test]
fn simple_splice() {
    let name = "world";
    let result = container! { "Hello, " (name) "!" };
    assert_eq!(result.to_string(), "Hello, world!");
}

#[ignore]
#[test]
fn expression_splice() {
    let x = 5;
    let y = 3;
    let result = container! { "The sum is " (x + y) };
    assert_eq!(result.to_string(), "The sum is 8");
}

#[ignore]
#[test]
fn raw_literals() {
    let result = container! { "<pinkie>" };
    assert_eq!(result.to_string(), "<pinkie>");
}

#[ignore]
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
    assert_eq!(result.to_string(), "3628800");
}

static BEST_PONY: &str = "Pinkie Pie";

#[ignore]
#[test]
fn statics() {
    let result = container! { (BEST_PONY) };
    assert_eq!(result.to_string(), "Pinkie Pie");
}

#[ignore]
#[test]
fn locals() {
    let best_pony = "Pinkie Pie";
    let result = container! { (best_pony) };
    assert_eq!(result.to_string(), "Pinkie Pie");
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

#[ignore]
#[test]
fn structs() {
    let pinkie = Creature {
        name: "Pinkie Pie",
        adorableness: 9,
    };
    let result = container! {
        "Name: " (pinkie.name) ". Rating: " (pinkie.repugnance())
    };
    assert_eq!(result.to_string(), "Name: Pinkie Pie. Rating: 1");
}

#[ignore]
#[test]
fn tuple_accessors() {
    let a = ("ducks", "geese");
    let result = container! { (a.0) };
    assert_eq!(result.to_string(), "ducks");
}

#[ignore]
#[test]
fn splice_with_path() {
    mod inner {
        pub fn name() -> &'static str {
            "Maud"
        }
    }
    let result = container! { (inner::name()) };
    assert_eq!(result.to_string(), "Maud");
}

#[ignore]
#[test]
fn nested_macro_invocation() {
    let best_pony = "Pinkie Pie";
    let result = container! { (format!("{best_pony} is best pony")) };
    assert_eq!(result.to_string(), "Pinkie Pie is best pony");
}

#[ignore]
#[test]
fn expression_grouping() {
    let result = container! { (1 + 1) };
    assert_eq!(result.to_string(), "2");
}
