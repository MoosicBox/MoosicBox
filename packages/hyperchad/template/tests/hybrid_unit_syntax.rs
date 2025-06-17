use hyperchad_template::container;

#[test]
fn test_concise_viewport_unit_syntax() {
    let containers = container! {
        div width=vw50 height=vh100 max-width=dvw90 min-height=dvh60 {
            "Concise syntax test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("50vw"), "Should contain vw50 as 50vw");
    assert!(html.contains("100vh"), "Should contain vh100 as 100vh");
    assert!(html.contains("90dvw"), "Should contain dvw90 as 90dvw");
    assert!(html.contains("60dvh"), "Should contain dvh60 as 60dvh");
}

#[test]
fn test_function_style_viewport_syntax() {
    let dynamic_width = 75;
    let responsive_height = 80;
    let base_size = 45;

    let containers = container! {
        div width=vw(dynamic_width) height=vh(responsive_height)
            max-width=dvw(base_size + 5) min-height=dvh(base_size / 2) {
            "Function syntax test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("75vw"), "Should contain vw(75) as 75vw");
    assert!(html.contains("80vh"), "Should contain vh(80) as 80vh");
    assert!(html.contains("50dvw"), "Should contain dvw(50) as 50dvw");
    assert!(html.contains("22dvh"), "Should contain dvh(22) as 22dvh");
}

#[test]
fn test_mixed_unit_syntax() {
    let responsive_height = 80;
    let base_size = 45;

    let containers = container! {
        div width=vw50 height=vh(responsive_height)
            max-width=dvw90 min-height=dvh(base_size) {
            "Mixed syntax test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("50vw"), "Should contain vw50 as 50vw");
    assert!(html.contains("80vh"), "Should contain vh(80) as 80vh");
    assert!(html.contains("90dvw"), "Should contain dvw90 as 90dvw");
    assert!(html.contains("45dvh"), "Should contain dvh(45) as 45dvh");
}

#[test]
fn test_complex_expressions_in_function_syntax() {
    let dynamic_width = 75;
    let responsive_height = 80;
    let base_size = 45;

    let containers = container! {
        div width=vw(if dynamic_width > 50 { 100 } else { 50 })
            height=vh(responsive_height + 20)
            max-width=dvw(base_size * 2)
            min-height=dvh(base_size.min(30)) {
            "Complex expressions test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(
        html.contains("100vw"),
        "Should contain conditional result 100vw"
    );
    assert!(
        html.contains("100vh"),
        "Should contain arithmetic result 100vh"
    );
    assert!(
        html.contains("90dvw"),
        "Should contain multiplication result 90dvw"
    );
    assert!(html.contains("30dvh"), "Should contain min() result 30dvh");
}

#[test]
fn test_traditional_syntax_still_works() {
    let containers = container! {
        div width=800 height=600 padding=20 margin=100% {
            "Traditional syntax test"
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("800"), "Should contain plain number 800");
    assert!(html.contains("600"), "Should contain plain number 600");
    assert!(html.contains("20"), "Should contain plain number 20");
    assert!(html.contains("100%"), "Should contain percentage 100%");
}

#[test]
fn test_comprehensive_hybrid_syntax() {
    let dynamic_width = 75;
    let responsive_height = 80;
    let base_size = 45;

    let containers = container! {
        div class="demo-container" {
            // Concise syntax
            div width=vw50 height=vh100 max-width=dvw90 min-height=dvh60
                background="blue" padding=20 {
                "Concise test"
            }

            // Function syntax
            section width=vw(dynamic_width) height=vh(responsive_height)
                    max-width=dvw(base_size + 5) min-height=dvh(base_size / 2)
                    background="green" margin=10 {
                "Function test"
            }

            // Mixed syntax
            div width=vw50 height=vh(responsive_height)
                max-width=dvw90 min-height=dvh(base_size)
                background="red" opacity=0.8 {
                "Mixed test"
            }

            // Traditional syntax
            div width=800 height=600 padding=20 margin=100%
                background="orange" {
                "Traditional test"
            }
        }
    };

    assert_eq!(containers.len(), 1, "Should generate exactly one container");

    let html = containers[0].to_string();

    // Verify all syntax types are present
    assert!(html.contains("50vw"), "Should contain concise vw syntax");
    assert!(html.contains("75vw"), "Should contain function vw syntax");
    assert!(html.contains("90dvw"), "Should contain dvw syntax");
    assert!(
        html.contains("800"),
        "Should contain traditional number syntax"
    );
    assert!(html.contains("100%"), "Should contain percentage syntax");

    // Verify content
    assert!(
        html.contains("Concise test"),
        "Should contain concise test content"
    );
    assert!(
        html.contains("Function test"),
        "Should contain function test content"
    );
    assert!(
        html.contains("Mixed test"),
        "Should contain mixed test content"
    );
    assert!(
        html.contains("Traditional test"),
        "Should contain traditional test content"
    );
}
