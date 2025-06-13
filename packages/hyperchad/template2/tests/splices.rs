use hyperchad_template2::container;

#[test]
fn literals() {
    let result = container! { ("<pinkie>") };
    assert_eq!(result.into_string(), "<pinkie>");
}

#[test]
fn raw_literals() {
    use hyperchad_template2::PreEscaped;
    let result = container! { (PreEscaped("<pinkie>")) };
    assert_eq!(result.into_string(), "<pinkie>");
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
    assert_eq!(result.into_string(), "3628800");
}

#[test]
fn attributes() {
    let alt = "Pinkie Pie";
    let result = container! { img src="pinkie.jpg" alt=(alt); };
    assert_eq!(
        result.into_string(),
        r#"<img src="pinkie.jpg" alt="Pinkie Pie">"#
    );
}

#[test]
fn class_shorthand() {
    let pinkie_class = "pinkie";
    let result = container! { p.(pinkie_class) { "Fun!" } };
    assert_eq!(result.into_string(), r#"<p class="pinkie">Fun!</p>"#);
}

#[test]
fn class_shorthand_block() {
    let class_prefix = "pinkie-";
    let result = container! { p.{ (class_prefix) "123" } { "Fun!" } };
    assert_eq!(result.into_string(), r#"<p class="pinkie-123">Fun!</p>"#);
}

#[test]
fn id_shorthand() {
    let pinkie_id = "pinkie";
    let result = container! { p #(pinkie_id) { "Fun!" } };
    assert_eq!(result.into_string(), r#"<p id="pinkie">Fun!</p>"#);
}

static BEST_PONY: &str = "Pinkie Pie";

#[test]
fn statics() {
    let result = container! { (BEST_PONY) };
    assert_eq!(result.into_string(), "Pinkie Pie");
}

#[test]
fn locals() {
    let best_pony = "Pinkie Pie";
    let result = container! { (best_pony) };
    assert_eq!(result.into_string(), "Pinkie Pie");
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
    assert_eq!(result.into_string(), "Name: Pinkie Pie. Rating: 1");
}

#[test]
fn tuple_accessors() {
    let a = ("ducks", "geese");
    let result = container! { (a.0) };
    assert_eq!(result.into_string(), "ducks");
}

#[test]
fn splice_with_path() {
    mod inner {
        pub fn name() -> &'static str {
            "Maud"
        }
    }
    let result = container! { (inner::name()) };
    assert_eq!(result.into_string(), "Maud");
}

#[test]
fn nested_macro_invocation() {
    let best_pony = "Pinkie Pie";
    let result = container! { (format!("{best_pony} is best pony")) };
    assert_eq!(result.into_string(), "Pinkie Pie is best pony");
}

#[test]
fn expression_grouping() {
    let result = container! { (1 + 1) };
    assert_eq!(result.into_string(), "2");
}
