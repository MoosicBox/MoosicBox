use hyperchad_template::{ContainerVecExt, container};

#[test]
fn htmx_attributes() {
    let result = container! {
        div hx-get="/api/data" hx-trigger="click" hx-swap="outerHTML" {
            "Click me for HTMX magic"
        }
    };

    let container = &result[0];
    assert!(container.route.is_some());
    // Check for HTMX attributes in the rendered output
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("hx-get=\"/api/data\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("hx-trigger=\"click\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Click me for HTMX magic")
    );
}

#[test]
fn complex_styling() {
    let result = container! {
        div
            width="100%"
            height="200px"
            padding="16px"
            margin="8px"
            background="blue"
            color="white"
            border="1px solid black"
            border-radius="8px"
        {
            "Styled content"
        }
    };

    let container = &result[0];
    assert!(container.width.is_some());
    assert!(container.height.is_some());
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
    assert!(container.color.is_some());
    assert!(container.border_top.is_some());
    assert!(container.border_top_left_radius.is_some());
}

#[test]
fn flexbox_layout() {
    let result = container! {
        div
            direction="row"
            justify-content="space-between"
            align-items="center"
            flex="1"
            gap="16px"
        {
            div flex-grow="1" { "Item 1" }
            div flex-grow="2" { "Item 2" }
            div flex-shrink="0" { "Item 3" }
        }
    };

    let container = &result[0];
    assert_eq!(
        container.direction,
        hyperchad_transformer_models::LayoutDirection::Row
    );
    assert!(container.justify_content.is_some());
    assert!(container.align_items.is_some());
    assert!(container.flex.is_some());
    assert!(container.column_gap.is_some());
}

#[test]
fn responsive_design() {
    let is_mobile = true;
    let result = container! {
        div
            width={if is_mobile { "100%" } else { "800px" }}
            padding={if is_mobile { "8px" } else { "16px" }}
        {
            "Responsive content"
        }
    };

    let container = &result[0];
    assert!(container.width.is_some());
    assert!(container.padding_top.is_some());
}

#[test]
fn form_validation() {
    let result = container! {
        form {
            div {
                input
                    type="email"
                    name="email"
                    placeholder="Enter your email"
                    required;
            }
            div {
                input
                    type="password"
                    name="password"
                    placeholder="Enter your password"
                    required;
            }
            button type="submit" { "Submit" }
        }
    };

    // Check for key form elements and attributes
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("<form")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("type=\"text\"")
            || result
                .display_to_string(false, false)
                .unwrap()
                .contains("type=\"email\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("type=\"password\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("name=\"email\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("name=\"password\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("type=\"submit\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("placeholder=\"Enter your email\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("placeholder=\"Enter your password\"")
    );
}

#[test]
fn accessibility_attributes() {
    let result = container! {
        div {
            button
                data-testid="submit-button"
                data-cy="submit-btn"
            {
                "Submit"
            }
            image
                src="logo.png"
                alt="Company Logo"
                loading="lazy";
        }
    };

    let _containers = &result;
    // Check if data attributes are present in the rendered HTML
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("data-testid=\"submit-button\"")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("data-cy=\"submit-btn\"")
    );
}

#[test]
fn conditional_rendering() {
    let show_alert = true;
    let user_name = Some("Alice");

    let result = container! {
        div {
            @if show_alert {
                div.alert { "Welcome!" }
            }

            @if let Some(name) = user_name {
                div { "Hello, " (name) "!" }
            } @else {
                div { "Please log in" }
            }
        }
    };

    // Check that the structure is correct, ignoring debug attributes
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains(r#"class="alert"#)
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Welcome!")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Hello, Alice!")
    );
}

#[test]
fn dynamic_classes() {
    let _is_active = true;
    let _is_disabled = false;
    let _theme = "dark";

    let result = container! {
        button
            .base-button
            .active
            .theme-dark
        {
            "Dynamic button"
        }
    };

    let container = &result[0];
    assert!(container.classes.contains(&"base-button".to_string()));
    assert!(container.classes.contains(&"active".to_string()));
    assert!(container.classes.contains(&"theme-dark".to_string()));
}

#[test]
fn nested_components() {
    fn card(title: &str, content: &str) -> Vec<hyperchad_transformer::Container> {
        container! {
            div.card {
                div.card-header {
                    h3 { (title) }
                }
                div.card-body {
                    (content)
                }
            }
        }
    }

    let result = container! {
        div.container {
            (card("Welcome", "This is a card component"))
            (card("About", "This is another card"))
        }
    };

    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("card")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Welcome")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("About")
    );
}

#[test]
fn event_handlers() {
    let result = container! {
        div
            fx-click="alert('Clicked!')"
            fx-hover="console.log('Hovered')"
            fx-resize="handleResize()"
        {
            "Interactive element"
        }
    };

    let container = &result[0];
    assert!(!container.actions.is_empty());
}

#[test]
fn table_with_data() {
    let users = vec![
        ("Alice", 30, "Engineer"),
        ("Bob", 25, "Designer"),
        ("Charlie", 35, "Manager"),
    ];

    let result = container! {
        table {
            thead {
                tr {
                    th { "Name" }
                    th { "Age" }
                    th { "Role" }
                }
            }
            tbody {
                @for (name, age, role) in &users {
                    tr {
                        td { (name) }
                        td { (age) }
                        td { (role) }
                    }
                }
            }
        }
    };

    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Alice")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("30")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Engineer")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Bob")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Charlie")
    );
}

#[test]
fn media_elements() {
    let result = container! {
        div {
            image
                src="hero.jpg"
                alt="Hero image"
                srcset="hero-small.jpg 480w, hero-large.jpg 800w"
                sizes="(max-width: 600px) 480px, 800px"
                loading="lazy"
                fit="cover";

            div { "Canvas placeholder" }
        }
    };

    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("hero.jpg")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("srcset")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("Canvas placeholder")
    );
}

#[test]
fn complex_form() {
    let result = container! {
        form {
            div.form-group {
                input type="text" name="firstName" placeholder="First Name";
                input type="text" name="lastName" placeholder="Last Name";
            }

            div.form-group {
                input type="email" name="email" placeholder="Email";
                input type="tel" name="phone" placeholder="Phone";
            }

            div.form-group {
                input type="checkbox" name="newsletter" checked;
                " Subscribe to newsletter"
            }

            div.form-actions {
                button type="submit" { "Submit" }
                button type="reset" { "Reset" }
            }
        }
    };

    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("form-group")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("firstName")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("newsletter")
    );
    assert!(
        result
            .display_to_string(false, false)
            .unwrap()
            .contains("checked")
    );
}

#[test]
fn layout_positioning() {
    let result = container! {
        div position="relative" width="100%" height="200px" {
            div
                position="absolute"
                top="10px"
                left="10px"
                background="red"
                width="50px"
                height="50px"
            {
                "Positioned element"
            }
        }
    };

    let container = &result[0];
    assert!(matches!(
        container.position,
        Some(hyperchad_transformer_models::Position::Relative)
    ));
    assert!(container.width.is_some());
    assert!(container.height.is_some());

    let child = &result[0].children[0];
    assert!(matches!(
        child.position,
        Some(hyperchad_transformer_models::Position::Absolute)
    ));
    assert!(child.top.is_some());
    assert!(child.left.is_some());
}
