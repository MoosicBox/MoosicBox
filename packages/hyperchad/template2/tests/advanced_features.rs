use hyperchad_template2::{ContainerVecExt, container};

#[test]
fn htmx_attributes() {
    let result = container! {
        Div hx-get="/api/data" hx-trigger="click" hx-swap="outerHTML" {
            "Click me for HTMX magic"
        }
    };

    let container = &result[0];
    assert!(container.route.is_some());
    // Check for HTMX attributes in the rendered output
    assert!(result.to_string().contains("hx-get=\"/api/data\""));
    assert!(result.to_string().contains("hx-trigger=\"click\""));
    assert!(result.to_string().contains("Click me for HTMX magic"));
}

#[test]
fn complex_styling() {
    let result = container! {
        Div
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
        Div
            direction="row"
            justify-content="space-between"
            align-items="center"
            flex="1"
            gap="16px"
        {
            Div flex-grow="1" { "Item 1" }
            Div flex-grow="2" { "Item 2" }
            Div flex-shrink="0" { "Item 3" }
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
        Div
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
        Form {
            Div {
                Input
                    type="email"
                    name="email"
                    placeholder="Enter your email"
                    required;
            }
            Div {
                Input
                    type="password"
                    name="password"
                    placeholder="Enter your password"
                    required;
            }
            Button type="submit" { "Submit" }
        }
    };

    // Check for key form elements and attributes
    assert!(result.to_string().contains("<form"));
    assert!(
        result.to_string().contains("type=\"text\"")
            || result.to_string().contains("type=\"email\"")
    );
    assert!(result.to_string().contains("type=\"password\""));
    assert!(result.to_string().contains("name=\"email\""));
    assert!(result.to_string().contains("name=\"password\""));
    assert!(result.to_string().contains("type=\"submit\""));
    assert!(
        result
            .to_string()
            .contains("placeholder=\"Enter your email\"")
    );
    assert!(
        result
            .to_string()
            .contains("placeholder=\"Enter your password\"")
    );
}

#[test]
fn accessibility_attributes() {
    let result = container! {
        Div {
            Button
                data-testid="submit-button"
                data-cy="submit-btn"
            {
                "Submit"
            }
            Image
                src="logo.png"
                alt="Company Logo"
                loading="lazy";
        }
    };

    let _containers = &result;
    // Check if data attributes are present in the rendered HTML
    assert!(result.to_string().contains("data-testid=\"submit-button\""));
    assert!(result.to_string().contains("data-cy=\"submit-btn\""));
}

#[test]
fn conditional_rendering() {
    let show_alert = true;
    let user_name = Some("Alice");

    let result = container! {
        Div {
            @if show_alert {
                Div.alert { "Welcome!" }
            }

            @if let Some(name) = user_name {
                Div { "Hello, " (name) "!" }
            } @else {
                Div { "Please log in" }
            }
        }
    };

    // Check that the structure is correct, ignoring debug attributes
    assert!(result.to_string().contains(r#"class="alert"#));
    assert!(result.to_string().contains("Welcome!"));
    assert!(result.to_string().contains("Hello, Alice!"));
}

#[test]
fn dynamic_classes() {
    let _is_active = true;
    let _is_disabled = false;
    let _theme = "dark";

    let result = container! {
        Button
            .base-button
            .active
            .theme-dark
        {
            "Dynamic Button"
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
            Div.card {
                Div.card-header {
                    H3 { (title) }
                }
                Div.card-body {
                    (content)
                }
            }
        }
    }

    let result = container! {
        Div.container {
            (card("Welcome", "This is a card component"))
            (card("About", "This is another card"))
        }
    };

    assert!(result.to_string().contains("card"));
    assert!(result.to_string().contains("Welcome"));
    assert!(result.to_string().contains("About"));
}

#[test]
fn event_handlers() {
    let result = container! {
        Div
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
        Table {
            THead {
                TR {
                    TH { "Name" }
                    TH { "Age" }
                    TH { "Role" }
                }
            }
            TBody {
                @for (name, age, role) in &users {
                    TR {
                        TD { (name) }
                        TD { (age) }
                        TD { (role) }
                    }
                }
            }
        }
    };

    assert!(result.to_string().contains("Alice"));
    assert!(result.to_string().contains("30"));
    assert!(result.to_string().contains("Engineer"));
    assert!(result.to_string().contains("Bob"));
    assert!(result.to_string().contains("Charlie"));
}

#[test]
fn media_elements() {
    let result = container! {
        Div {
            Image
                src="hero.jpg"
                alt="Hero image"
                srcset="hero-small.jpg 480w, hero-large.jpg 800w"
                sizes="(max-width: 600px) 480px, 800px"
                loading="lazy"
                fit="cover";

            Div { "Canvas placeholder" }
        }
    };

    assert!(result.to_string().contains("hero.jpg"));
    assert!(result.to_string().contains("srcset"));
    assert!(result.to_string().contains("Canvas placeholder"));
}

#[test]
fn complex_form() {
    let result = container! {
        Form {
            Div.form-group {
                Input type="text" name="firstName" placeholder="First Name";
                Input type="text" name="lastName" placeholder="Last Name";
            }

            Div.form-group {
                Input type="email" name="email" placeholder="Email";
                Input type="tel" name="phone" placeholder="Phone";
            }

            Div.form-group {
                Input type="checkbox" name="newsletter" checked;
                " Subscribe to newsletter"
            }

            Div.form-actions {
                Button type="submit" { "Submit" }
                Button type="reset" { "Reset" }
            }
        }
    };

    assert!(result.to_string().contains("form-group"));
    assert!(result.to_string().contains("firstName"));
    assert!(result.to_string().contains("newsletter"));
    assert!(result.to_string().contains("checked"));
}

#[test]
fn layout_positioning() {
    let result = container! {
        Div position="relative" width="100%" height="200px" {
            Div
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
