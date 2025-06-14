use hyperchad_template2::{ContainerVecExt, container};

#[ignore]
#[test]
fn if_statement() {
    let show_content = true;
    let result = container! {
        @if show_content {
            Div { "Content is shown" }
        }
    };
    // Just check that it compiles and produces some output
    assert!(!result.to_string().is_empty());
}

#[ignore]
#[test]
fn for_loop() {
    let items = vec!["apple", "banana", "cherry"];
    let result = container! {
        Ul {
            @for item in &items {
                Li { (item) }
            }
        }
    };
    // Just check that it compiles and produces some output
    assert!(!result.to_string().is_empty());
}

#[ignore]
#[test]
fn let_statement() {
    let result = container! {
        @let x = 42;
        Div { "I have " (x) " cupcakes!" }
    };
    // Just check that it compiles and produces some output
    assert!(!result.to_string().is_empty());
}
