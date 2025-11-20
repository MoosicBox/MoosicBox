use hyperchad_template::container;
use hyperchad_transformer_models::UserSelect;

#[test_log::test]
fn test_user_select_with_quoted_string_literals() {
    let containers = container! {
        div user-select="auto" { "Auto text" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::Auto));

    let containers = container! {
        div user-select="none" { "None text" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::None));

    let containers = container! {
        div user-select="text" { "Text selectable" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::Text));

    let containers = container! {
        div user-select="all" { "All selectable" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::All));
}

#[test_log::test]
fn test_user_select_with_unquoted_identifiers() {
    let containers = container! {
        div user-select=auto { "Auto text" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::Auto));

    let containers = container! {
        div user-select=none { "None text" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::None));

    let containers = container! {
        div user-select=text { "Text selectable" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::Text));

    let containers = container! {
        div user-select=all { "All selectable" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::All));
}

#[test_log::test]
fn test_user_select_with_expressions() {
    let containers = container! {
        div user-select=(UserSelect::Auto) { "Auto with expression" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::Auto));

    let containers = container! {
        div user-select=(UserSelect::None) { "None with expression" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::None));

    let containers = container! {
        div user-select=(UserSelect::Text) { "Text with expression" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::Text));

    let containers = container! {
        div user-select=(UserSelect::All) { "All with expression" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::All));
}

#[test_log::test]
fn test_user_select_with_function_calls() {
    fn get_user_select() -> UserSelect {
        UserSelect::None
    }

    let containers = container! {
        div user-select=(get_user_select()) { "Function-determined user-select" }
    };
    assert_eq!(containers[0].user_select, Some(UserSelect::None));
}

#[test_log::test]
fn test_user_select_html_output() {
    let containers = container! {
        div user-select=auto { "Auto" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("user-select"));
    assert!(html.contains("auto"));

    let containers = container! {
        div user-select=none { "None" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("user-select"));
    assert!(html.contains("none"));

    let containers = container! {
        div user-select=text { "Text" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("user-select"));
    assert!(html.contains("text"));

    let containers = container! {
        div user-select=all { "All" }
    };
    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();
    assert!(html.contains("user-select"));
    assert!(html.contains("all"));
}

#[test_log::test]
fn test_user_select_combined_with_other_attributes() {
    let containers = container! {
        div
            user-select=none
            cursor="pointer"
            padding=20
            background="blue"
        {
            "Combined attributes"
        }
    };

    let container = &containers[0];
    assert_eq!(container.user_select, Some(UserSelect::None));
    assert!(container.cursor.is_some());
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
}

#[test_log::test]
fn test_user_select_default_is_none() {
    let containers = container! {
        div { "No user-select specified" }
    };
    assert_eq!(containers[0].user_select, None);
}

#[test_log::test]
fn test_user_select_nested_elements() {
    let containers = container! {
        div user-select=auto {
            div user-select=none {
                "Inner not selectable"
            }
            "Outer auto"
        }
    };

    assert_eq!(containers[0].user_select, Some(UserSelect::Auto));
    assert_eq!(
        containers[0].children[0].user_select,
        Some(UserSelect::None)
    );
}

#[test_log::test]
fn test_user_select_with_conditional() {
    let prevent_select = true;

    let containers = container! {
        div user-select=(if prevent_select { UserSelect::None } else { UserSelect::Auto }) {
            "Conditional user-select"
        }
    };

    assert_eq!(containers[0].user_select, Some(UserSelect::None));
}
