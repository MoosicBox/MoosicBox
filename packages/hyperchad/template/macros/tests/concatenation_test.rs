use hyperchad_template_macros::container;

#[test]
fn test_string_literal_concatenation() {
    let item_id = "123";

    let containers = container! {
        div hx-get={"/api/items/"(item_id)} {
            "Item details"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the route was parsed correctly with concatenation
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Get { route, .. } => {
                assert_eq!(route, "/api/items/123");
            }
            _ => panic!("Expected GET route"),
        }
    } else {
        panic!("Expected route to be set");
    }
}

#[test]
fn test_multiple_concatenation() {
    let api_source = "spotify";
    let action = "scan";

    let containers = container! {
        button hx-post={"/music-api/"(action)"?apiSource="(api_source)} {
            "Execute"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the route was parsed correctly with multiple concatenations
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Post { route, .. } => {
                assert_eq!(route, "/music-api/scan?apiSource=spotify");
            }
            _ => panic!("Expected POST route"),
        }
    } else {
        panic!("Expected route to be set");
    }
}

#[test]
fn test_id_concatenation() {
    let prefix = "settings";
    let suffix = "error";

    let containers = container! {
        div id={(prefix)"-"(suffix)} {
            "Error message"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the ID was concatenated correctly
    assert_eq!(container.str_id, Some("settings-error".to_string()));
}
