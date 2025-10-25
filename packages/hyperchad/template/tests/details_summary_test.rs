use hyperchad_template::container;

#[test]
fn test_details_without_summary() {
    let _result = container! {
        details {
            div { "Just content" }
        }
    };
}

#[test]
fn test_details_with_summary_first() {
    let _result = container! {
        details {
            summary { "Click me" }
            div { "Hidden content" }
        }
    };
}

#[test]
fn test_details_with_open_attribute() {
    let _result = container! {
        details open="true" {
            summary { "Already open" }
            div { "Visible content" }
        }
    };
}

#[test]
fn test_details_renders_to_html() {
    let result = container! {
        details {
            summary { "Click me" }
            div { "Hidden content" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();

    assert!(html.contains("<details"));
    assert!(html.contains("<summary"));
    assert!(html.contains("Click me"));
    assert!(html.contains("Hidden content"));
}

#[test]
fn test_details_with_open_renders_open_attribute() {
    let result = container! {
        details open="true" {
            summary { "Test" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("open"));
}

#[test]
fn test_details_open_boolean_literal() {
    let result = container! {
        details open=true {
            summary { "Test" }
            div { "Content" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("<details"));
    assert!(html.contains("open"));
    assert!(html.contains("Test"));
}

#[test]
fn test_details_open_boolean_false() {
    let result = container! {
        details open=false {
            summary { "Test" }
            div { "Content" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("<details"));
    assert!(!html.contains("open"));
}

#[test]
fn test_details_open_variable() {
    let is_open = true;
    let result = container! {
        details open=(is_open) {
            summary { "Test" }
            div { "Content" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("<details"));
    assert!(html.contains("open"));
}

#[test]
fn test_details_open_variable_false() {
    let is_open = false;
    let result = container! {
        details open=(is_open) {
            summary { "Test" }
            div { "Content" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("<details"));
    assert!(!html.contains("open"));
}

#[test]
fn test_details_open_presence_only() {
    let result = container! {
        details open {
            summary { "Test" }
            div { "Content" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("<details"));
    assert!(html.contains("open"));
}

#[test]
fn test_details_open_string_true() {
    let result = container! {
        details open="true" {
            summary { "Test" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("open"));
}

#[test]
fn test_details_open_string_false() {
    let result = container! {
        details open="false" {
            summary { "Test" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(!html.contains("open"));
}

#[test]
fn test_details_no_open_attribute() {
    let result = container! {
        details {
            summary { "Test" }
        }
    };

    let html = result[0].display_to_string_default(true, false).unwrap();
    assert!(html.contains("<details"));
    assert!(!html.contains("open"));
}
