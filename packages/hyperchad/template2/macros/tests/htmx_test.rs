use hyperchad_template2_macros::container;

#[test]
fn test_htmx_get_route() {
    let containers = container! {
        Div hx-get="/test-route" hx-trigger="load" {
            "Hello World"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the route is set correctly
    assert!(container.route.is_some());
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Get {
                route,
                trigger,
                swap: _,
            } => {
                assert_eq!(route, "/test-route");
                assert_eq!(trigger.as_deref(), Some("load"));
            }
            _ => panic!("Expected Route::Get variant"),
        }
    }
}

#[test]
fn test_htmx_post_route_with_swap() {
    let containers = container! {
        Button hx-post="/submit" hx-swap="children" {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the route is set correctly
    assert!(container.route.is_some());
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Post {
                route,
                trigger,
                swap,
            } => {
                assert_eq!(route, "/submit");
                assert_eq!(*trigger, None);
                assert_eq!(*swap, hyperchad_transformer_models::SwapTarget::Children);
            }
            _ => panic!("Expected Route::Post variant"),
        }
    }
}

#[test]
fn test_htmx_delete_route_with_id_swap() {
    let containers = container! {
        Button hx-delete="/delete/item" hx-swap="#item-list" {
            "Delete"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the route is set correctly
    assert!(container.route.is_some());
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Delete {
                route,
                trigger,
                swap,
            } => {
                assert_eq!(route, "/delete/item");
                assert_eq!(*trigger, None);
                assert_eq!(
                    *swap,
                    hyperchad_transformer_models::SwapTarget::Id("item-list".to_string())
                );
            }
            _ => panic!("Expected Route::Delete variant"),
        }
    }
}

#[test]
fn test_htmx_expression_route() {
    let _base_url = "/api";
    let _item_id = 123;

    let containers = container! {
        Div hx-get={(format!("{}/items/{}", _base_url, _item_id))} hx-trigger="click" {
            "Load Item"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that the route is set correctly
    assert!(container.route.is_some());
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Get {
                route,
                trigger,
                swap: _,
            } => {
                assert_eq!(route, "/api/items/123");
                assert_eq!(trigger.as_deref(), Some("click"));
            }
            _ => panic!("Expected Route::Get variant"),
        }
    }
}

#[test]
fn test_mixed_attributes() {
    let containers = container! {
        Div
            hx-post="/submit"
            hx-trigger="submit"
            hx-swap="this"
            padding=10
            background="red" {
            "Form"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check route
    assert!(container.route.is_some());
    if let Some(route) = &container.route {
        match route {
            hyperchad_transformer_models::Route::Post {
                route,
                trigger,
                swap,
            } => {
                assert_eq!(route, "/submit");
                assert_eq!(trigger.as_deref(), Some("submit"));
                assert_eq!(*swap, hyperchad_transformer_models::SwapTarget::This);
            }
            _ => panic!("Expected Route::Post variant"),
        }
    }

    // Check other attributes still work
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
}
