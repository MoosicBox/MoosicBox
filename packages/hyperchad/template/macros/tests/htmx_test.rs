use hyperchad_template_macros::container;

#[test]
fn test_htmx_get_route() {
    let containers = container! {
        div hx-get="/test-route" hx-trigger="load" {
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
                target: _,
                strategy: _,
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
        button hx-post="/submit" hx-swap="children" {
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
                target,
                strategy,
            } => {
                assert_eq!(route, "/submit");
                assert_eq!(*trigger, None);
                assert_eq!(*target, hyperchad_transformer_models::Selector::SelfTarget);
                assert_eq!(
                    *strategy,
                    hyperchad_transformer_models::SwapStrategy::Children
                );
            }
            _ => panic!("Expected Route::Post variant"),
        }
    }
}

#[test]
fn test_htmx_delete_route_with_id_target() {
    let containers = container! {
        button hx-delete="/delete/item" hx-target="#item-list" hx-swap="delete" {
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
                target,
                strategy,
            } => {
                assert_eq!(route, "/delete/item");
                assert_eq!(*trigger, None);
                assert_eq!(
                    *target,
                    hyperchad_transformer_models::Selector::Id("item-list".to_string())
                );
                assert_eq!(
                    *strategy,
                    hyperchad_transformer_models::SwapStrategy::Delete
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
        div hx-get={(format!("{}/items/{}", _base_url, _item_id))} hx-trigger="click" {
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
                target: _,
                strategy: _,
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
        div
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
                target,
                strategy,
            } => {
                assert_eq!(route, "/submit");
                assert_eq!(trigger.as_deref(), Some("submit"));
                assert_eq!(*target, hyperchad_transformer_models::Selector::SelfTarget);
                assert_eq!(*strategy, hyperchad_transformer_models::SwapStrategy::This);
            }
            _ => panic!("Expected Route::Post variant"),
        }
    }

    // Check other attributes still work
    assert!(container.padding_top.is_some());
    assert!(container.background.is_some());
}
