use hyperchad_template::container;

struct Settings {
    auth_method: Option<String>,
}

#[test]
fn test_if_let_with_some_value() {
    let auth_method = Some("oauth");
    let settings = Settings {
        auth_method: auth_method.map(|s| s.to_string()),
    };

    let containers = container! {
        div {
            @if let Some(auth_method) = &settings.auth_method {
                "Auth method: " (auth_method)
            } @else {
                "No authentication"
            }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Auth method: oauth"),
        "Should contain auth method"
    );
    assert!(
        !html.contains("No authentication"),
        "Should not contain else branch"
    );
}

#[test]
fn test_if_let_with_none_value() {
    let settings = Settings { auth_method: None };

    let containers = container! {
        div {
            @if let Some(auth_method) = &settings.auth_method {
                "Auth method: " (auth_method)
            } @else {
                "No authentication"
            }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("No authentication"),
        "Should contain else branch"
    );
    assert!(
        !html.contains("Auth method:"),
        "Should not contain if branch"
    );
}

#[test]
fn test_complex_if_else_if_let_chain() {
    let settings = Settings {
        auth_method: Some("oauth".to_string()),
    };

    let containers = container! {
        div {
            @if settings.auth_method.is_none() {
                "No auth configured"
            } @else if let Some(auth_method) = &settings.auth_method {
                "Using auth: " (auth_method)
            } @else {
                "Unknown auth state"
            }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Using auth: oauth"),
        "Should contain else-if-let branch"
    );
    assert!(
        !html.contains("No auth configured"),
        "Should not contain first if branch"
    );
    assert!(
        !html.contains("Unknown auth state"),
        "Should not contain final else branch"
    );
}

#[test]
fn test_nested_if_let() {
    let outer_option = Some(Settings {
        auth_method: Some("jwt".to_string()),
    });

    let containers = container! {
        div {
            @if let Some(settings) = &outer_option {
                @if let Some(auth) = &settings.auth_method {
                    "Nested auth: " (auth)
                } @else {
                    "Settings exist but no auth"
                }
            } @else {
                "No settings"
            }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("Nested auth: jwt"),
        "Should contain nested if-let result"
    );
}
