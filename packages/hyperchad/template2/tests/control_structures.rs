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
    assert!(!result.display_to_string(false, false).unwrap().is_empty());
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
    assert!(!result.display_to_string(false, false).unwrap().is_empty());
}

#[ignore]
#[test]
fn let_statement() {
    let result = container! {
        @let x = 42;
        Div { "I have " (x) " cupcakes!" }
    };
    // Just check that it compiles and produces some output
    assert!(!result.display_to_string(false, false).unwrap().is_empty());
}

#[test]
fn if_expr() {
    for (number, &name) in (1..4).zip(["one", "two", "three"].iter()) {
        let result = container! {
            @if number == 1 {
                "one"
            } @else if number == 2 {
                "two"
            } @else if number == 3 {
                "three"
            } @else {
                "oh noes"
            }
        };
        assert_eq!(result.display_to_string(false, false).unwrap(), name);
    }
}

#[test]
fn if_let() {
    for &(input, output) in &[(Some("yay"), "yay"), (None, "oh noes")] {
        let result = container! {
            @if let Some(value) = input {
                (value)
            } @else {
                "oh noes"
            }
        };
        assert_eq!(result.display_to_string(false, false).unwrap(), output);
    }
}

#[test]
fn while_expr() {
    let mut numbers = (0..3).peekable();
    let result = container! {
        Ul {
            @while numbers.peek().is_some() {
                Li { (numbers.next().unwrap()) }
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<ul><li>0</li><li>1</li><li>2</li></ul>"
    );
}

#[test]
fn while_simple_condition() {
    let mut count = 0;
    #[allow(clippy::let_unit_value)]
    let result = container! {
        Ul {
            @while count < 3 {
                @let current = count;
                Li { "Item " (current) }
                @let _ = { count += 1; };
            }
        }
    };

    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<ul><li>Item 0</li><li>Item 1</li><li>Item 2</li></ul>"
    );
}

#[test]
fn for_expr() {
    let ponies = ["Apple Bloom", "Scootaloo", "Sweetie Belle"];
    let result = container! {
        Ul {
            @for pony in &ponies {
                Li { (pony) }
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        concat!(
            "<ul>",
            "<li>Apple Bloom</li>",
            "<li>Scootaloo</li>",
            "<li>Sweetie Belle</li>",
            "</ul>"
        )
    );
}

#[test]
fn match_expr() {
    for &(input, output) in &[(Some("yay"), "<div>yay</div>"), (None, "oh noes")] {
        let result = container! {
            @match input {
                Some(value) => {
                    Div { (value) }
                },
                None => {
                    "oh noes"
                },
            }
        };
        assert_eq!(result.display_to_string(false, false).unwrap(), output);
    }
}

#[test]
fn match_expr_without_delims() {
    for &(input, output) in &[(Some("yay"), "yay"), (None, "<span>oh noes</span>")] {
        let result = container! {
            @match input {
                Some(value) => (value),
                None => Span { "oh noes" },
            }
        };
        assert_eq!(result.display_to_string(false, false).unwrap(), output);
    }
}

#[test]
fn match_no_trailing_comma() {
    for &(input, output) in &[(Some("yay"), "yay"), (None, "<span>oh noes</span>")] {
        let result = container! {
            @match input {
                Some(value) => { (value) }
                None => Span { "oh noes" }
            }
        };
        assert_eq!(result.display_to_string(false, false).unwrap(), output);
    }
}

#[test]
fn match_expr_with_guards() {
    for &(input, output) in &[(Some(1), "one"), (None, "none"), (Some(2), "2")] {
        let result = container! {
            @match input {
                Some(value) if value % 3 == 1 => "one",
                Some(value) => (value),
                None => "none",
            }
        };
        assert_eq!(result.display_to_string(false, false).unwrap(), output);
    }
}

#[test]
fn let_expr() {
    let result = container! {
        @let x = 42;
        "I have " (x) " cupcakes!"
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "I have 42 cupcakes!"
    );
}

#[test]
fn let_lexical_scope() {
    let x = 42;
    let result = container! {
        {
            @let x = 99;
            "Twilight thought I had " (x) " cupcakes, "
        }
        "but I only had " (x) "."
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Twilight thought I had 99 cupcakes, but I only had 42."
    );
}

#[test]
fn let_type_ascription() {
    let result = container! {
        @let x: i32 = 42;
        "Value: " (x)
    };
    assert_eq!(result.display_to_string(false, false).unwrap(), "Value: 42");
}

#[test]
fn nested_control_flow() {
    let items = vec![Some(1), None, Some(2), Some(3)];
    let result = container! {
        Ul {
            @for item in &items {
                @match item {
                    Some(value) => {
                        Li { "Value: " (value) }
                    }
                    None => {
                        Li { "No value" }
                    }
                }
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<ul><li>Value: 1</li><li>No value</li><li>Value: 2</li><li>Value: 3</li></ul>"
    );
}

#[test]
fn complex_if_conditions() {
    let a = 5;
    let b = 10;
    let result = container! {
        @if a > 0 && b > 0 {
            "Both positive"
        } @else if a > 0 || b > 0 {
            "At least one positive"
        } @else {
            "Neither positive"
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "Both positive"
    );
}

#[test]
fn for_with_enumerate() {
    let items = ["apple", "banana", "cherry"];
    let result = container! {
        Ol {
            @for (i, item) in items.iter().enumerate() {
                Li { (i + 1) ": " (item) }
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<ol><li>1: apple</li><li>2: banana</li><li>3: cherry</li></ol>"
    );
}

#[test]
fn match_with_complex_patterns() {
    let data = (Some("hello"), 42);
    let result = container! {
        @match data {
            (Some(text), num) if num > 40 => {
                Div { (text) " - " (num) }
            }
            (Some(text), num) => {
                Span { (text) " - " (num) }
            }
            (None, num) => {
                "No text, number: " (num)
            }
        }
    };
    assert_eq!(
        result.display_to_string(false, false).unwrap(),
        "<div>hello - 42</div>"
    );
}
