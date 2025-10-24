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
