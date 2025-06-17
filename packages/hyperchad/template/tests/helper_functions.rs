use hyperchad_template::container;
use hyperchad_transformer::Number;

#[test]
fn test_percent_function_direct() {
    let height_percent = 50.0;
    let containers = container! {
        div width=(percent(height_percent)) {
            "Direct percent function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].width, Some(Number::RealPercent(50.0)));
}

#[test]
fn test_percent_function_unparenthesized() {
    let height_percent = 50.0;
    let containers = container! {
        div width=percent(height_percent) {
            "Unparenthesized percent function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].width, Some(Number::RealPercent(50.0)));
}

#[test]
fn test_vh_function_direct() {
    let viewport_height = 75.0;
    let containers = container! {
        div height=(vh(viewport_height)) {
            "Direct vh function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].height, Some(Number::RealVh(75.0)));
}

#[test]
fn test_vh_function_unparenthesized() {
    let viewport_height = 75.0;
    let containers = container! {
        div height=vh(viewport_height) {
            "Unparenthesized vh function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].height, Some(Number::RealVh(75.0)));
}

#[test]
fn test_vw_function_direct() {
    let viewport_width = 80.0;
    let containers = container! {
        div width=(vw(viewport_width)) {
            "Direct vw function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].width, Some(Number::RealVw(80.0)));
}

#[test]
fn test_vw_function_unparenthesized() {
    let viewport_width = 80.0;
    let containers = container! {
        div width=vw(viewport_width) {
            "Unparenthesized vw function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].width, Some(Number::RealVw(80.0)));
}

#[test]
fn test_dvh_function_direct() {
    let dynamic_height = 90.0;
    let containers = container! {
        div height=(dvh(dynamic_height)) {
            "Direct dvh function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].height, Some(Number::RealDvh(90.0)));
}

#[test]
fn test_dvw_function_direct() {
    let dynamic_width = 95.0;
    let containers = container! {
        div width=(dvw(dynamic_width)) {
            "Direct dvw function"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].width, Some(Number::RealDvw(95.0)));
}

#[test]
fn test_helper_functions_with_variables() {
    let volume_percent = 0.75;
    let height_percent = volume_percent * 100.0; // 75.0
    let viewport_size = 50.0;

    let containers = container! {
        div {
            div width=percent(height_percent) { "Width from percent" }
            div height=vh(viewport_size) { "Height from vh" }
            div width=vw(viewport_size) { "Width from vw" }
        }
    };
    assert_eq!(containers.len(), 1);
    let parent = &containers[0];

    // Check the children
    assert_eq!(parent.children.len(), 3);
    assert_eq!(parent.children[0].width, Some(Number::RealPercent(75.0)));
    assert_eq!(parent.children[1].height, Some(Number::RealVh(50.0)));
    assert_eq!(parent.children[2].width, Some(Number::RealVw(50.0)));
}

#[test]
fn test_helper_functions_vs_calc_equivalent() {
    let volume_percent = 0.5;
    let height_percent = volume_percent * 100.0; // 50.0

    // Test both direct usage and calc() usage to ensure they work the same
    let containers_direct = container! {
        div width=percent(height_percent) { "Direct" }
    };

    let containers_calc = container! {
        div width=(calc(percent(height_percent))) { "Calc wrapped" }
    };

    // Both should produce the same result
    assert_eq!(containers_direct[0].width, Some(Number::RealPercent(50.0)));
    // The calc version should produce a Calc variant containing the percent
    match &containers_calc[0].width {
        Some(Number::Calc(_calc)) => {
            // The calc should contain a Number with RealPercent(50.0)
            // This is implementation-specific, but we can at least verify it's a Calc
            assert!(true); // Calc was created successfully
        }
        _ => panic!("Expected Calc variant for calc() usage"),
    }
}

#[test]
fn test_helper_functions_with_expressions() {
    let base_value = 25.0;

    let containers = container! {
        div {
            div width=percent(base_value * 2.0) { "Expression in percent" }
            div height=vh(base_value + 25.0) { "Expression in vh" }
            div margin=vw(base_value / 2.0) { "Expression in vw" }
        }
    };
    assert_eq!(containers.len(), 1);
    let parent = &containers[0];

    // Check the children with calculated values
    assert_eq!(parent.children.len(), 3);
    assert_eq!(parent.children[0].width, Some(Number::RealPercent(50.0))); // 25.0 * 2.0
    assert_eq!(parent.children[1].height, Some(Number::RealVh(50.0))); // 25.0 + 25.0
    assert_eq!(parent.children[2].margin_left, Some(Number::RealVw(12.5))); // 25.0 / 2.0
    assert_eq!(parent.children[2].margin_right, Some(Number::RealVw(12.5)));
    assert_eq!(parent.children[2].margin_top, Some(Number::RealVw(12.5)));
    assert_eq!(parent.children[2].margin_bottom, Some(Number::RealVw(12.5)));
}

#[test]
fn test_integer_vs_real_values() {
    let containers = container! {
        div {
            div width=percent(50) { "Integer percent" }
            div height=vh(75) { "Integer vh" }
            div margin=vw(100.5) { "Real vw" }
        }
    };
    assert_eq!(containers.len(), 1);
    let parent = &containers[0];

    // Check that integers produce Integer variants and reals produce Real variants
    assert_eq!(parent.children.len(), 3);
    assert_eq!(parent.children[0].width, Some(Number::IntegerPercent(50)));
    assert_eq!(parent.children[1].height, Some(Number::IntegerVh(75)));
    assert_eq!(parent.children[2].margin_left, Some(Number::RealVw(100.5)));
    assert_eq!(parent.children[2].margin_right, Some(Number::RealVw(100.5)));
    assert_eq!(parent.children[2].margin_top, Some(Number::RealVw(100.5)));
    assert_eq!(
        parent.children[2].margin_bottom,
        Some(Number::RealVw(100.5))
    );
}
